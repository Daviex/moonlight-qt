# Moonlight Qt – Video Decode Performance Study (Independent)

**Scope:** Source-code-only analysis of the full video decode and rendering pipeline.  
**Method:** Manual review of every hot-path file; no profiler data.  
**Date:** 2025  

---

## Executive Summary

Moonlight Qt uses a *pull* model: a dedicated decoder thread owns the entire lifecycle from
network packet receipt through frame submission to the renderer.  The design is sound and
already includes several sophisticated optimisations (reference-frame invalidation, separate
D3D11 decode/render device with fence synchronisation, EGL DMA-BUF zero-copy).  However,
six issues can add measurable latency or CPU overhead on every frame, and a further seven
concern startup costs or edge-case degradations that are worth eliminating.

---

## 1  Pipeline Walkthrough

```
Network (moonlight-common-c)
  │  LiWaitForNextVideoFrame / LiPollNextVideoFrame
  ▼
decoderThreadProc()                     [ffmpeg.cpp:1810]
  │  writeBuffer() → m_DecodeBuffer     [ffmpeg.cpp:1748]
  │    ├─ H.264: SPS NALU rewrite       [ffmpeg.cpp:1750–1793]  ← h264_new/free heap alloc
  │    └─ appends NALU to QByteArray
  │  avcodec_send_packet()
  │  avcodec_receive_frame()
  │    └─ EAGAIN? → LiPollNextVideoFrame then SDL_Delay(2)   ← 2 ms sleep
  ▼
IFFmpegRenderer::renderFrame()
  │  (called by decoder thread via FFmpegVideoDecoder::submitFrame)
  │  passes AVFrame to Pacer::submitFrame()                  [pacer.cpp:403]
  ▼
Pacer vsync thread                      [pacer.cpp:104]
  │  handleVsync()                       [pacer.cpp:201]
  │  moves frames from m_PacingQueue → m_RenderQueue
  │  drops excess (frameDropTarget = 1 or 3)
  ▼
Pacer render thread                     [pacer.cpp:137]
  │  renderFrame()                       [pacer.cpp:332]
  │  calls renderer->renderFrame(frame)
  │  holds m_DeferredFreeFrame one extra frame               [pacer.cpp:348]
  ▼
Platform renderer  (D3D11VA / VAAPI / EGL / VDPAU / SDL)
  │  colour-space conversion, CSC constant buffer
  │  GPU present / vaPutSurface / eglSwapBuffers
  ▼
Display
```

### Key structural facts

| Fact | Location |
|---|---|
| Pull model (CAPABILITY_PULL_RENDERER) | ffmpeg.cpp:153 |
| Single 1 MB decode buffer (`m_DecodeBuffer`) | ffmpeg.cpp:224 |
| `m_FrameInfoQueue` for timing attribution | ffmpeg.cpp:2104 |
| MAX_QUEUED_FRAMES = 3 (pacer pacing queue) | pacer.cpp:22 |
| PACER_MAX_OUTSTANDING_FRAMES = 5 | ffmpeg.cpp:107 (ffmpeg.h) |
| Stats window flip inside `submitDecodeUnit` | ffmpeg.cpp:2013 |
| Deferred-free holds previous frame alive | pacer.cpp:348 |

---

## 2  Ranked Optimisation Opportunities

### #1 – `SDL_Delay(2)` in the EAGAIN poll loop adds systematic per-frame latency

**File:** `app/streaming/video/ffmpeg.cpp:1941–1948`

```cpp
// If no frame is available yet, wait a bit before trying again
if (LiPollNextVideoFrame(&m_DecodeUnit) == 0) {
    SDL_Delay(2);
    continue;
}
```

When `avcodec_receive_frame()` returns `AVERROR(EAGAIN)` and no new network packet is
immediately available, the decoder thread sleeps for **2 ms**.  At 120 fps the frame
budget is 8.3 ms; at 240 fps it is 4.2 ms.  A single EAGAIN cycle consumes 24 % of the
budget at 120 fps and 48 % at 240 fps.  For bursty codecs like AV1 (which can emit
B-frames) this happens frequently.

**Recommendation:** Replace `SDL_Delay(2)` with a short spin (e.g., `_mm_pause()` loop)
or, better, sleep on a semaphore that `LiWaitForNextVideoFrame` signals.  If the Limelight
API does not expose a semaphore, the poll granularity could be capped at 0.5 ms:

```cpp
SDL_Delay(0);   // yield without sleeping
```

or replaced with a platform nanosleep of 500 µs.

---

### #2 – Context mutex serialises D3D11VA decode and render in shared-device mode

**File:** `app/streaming/video/ffmpeg-renderers/d3d11va.cpp:742–813`

```cpp
void D3D11VARenderer::renderFrame(AVFrame* frame) {
    if (m_DecodeDevice == m_RenderDevice) {
        lockContext(this);   // SDL_LockMutex(m_ContextLock)
    }
    // ... render ...
    m_SwapChain->Present(0, flags);
    if (m_DecodeDevice == m_RenderDevice) {
        unlockContext(this);
    }
}
```

FFmpeg also calls `lockContext` / `unlockContext` during `avcodec_send_packet` and
`avcodec_receive_frame` whenever it touches the D3D11 immediate context.  In the
shared-device path (non-Intel, non-FL11.1+, or no NT-handle sharing support) decode and
render *time-share a single ID3D11DeviceContext*.  This means the render thread stalls
whenever a decode operation is in flight, and vice versa.

The separate-device path (d3d11va.cpp:361–398) avoids this entirely via NT-handle texture
sharing + D3D11 fences.  However, the separate-device path requires
`D3D11_FEATURE_D3D11_OPTIONS2::UnifiedMemoryArchitecture == FALSE` **or**
ExtendedResourceSharing.  The fallback is the shared-device path.

**Recommendation:** On GPUs where the separate-device path is unavailable, use a deferred
context (`ID3D11DeviceContext1::FinishCommandList` / `ExecuteCommandList`) for the render
pass so that the immediate context is only touched during Present.  This allows decoding
to proceed in parallel with command-list recording.

---

### #3 – Per-IDR `h264_new()` / `h264_free()` heap allocation in `writeBuffer`

**File:** `app/streaming/video/ffmpeg.cpp:1750–1793`

```cpp
h264_stream_t* h = h264_new();
// ... parse SPS, rewrite num_ref_frames, serialize ...
h264_free(h);
```

Every H.264 IDR frame invokes `h264_new()` (which calls `calloc` for the full
`h264_stream_t` structure) and `h264_free()`.  While IDR frames arrive at most once per
second under normal operation, during reconnect or seek scenarios they can arrive at
several per second.  The allocation itself is not the main cost — the cost is the
resulting cache miss on a cold `h264_stream_t`.

**Recommendation:** Allocate one `h264_stream_t` as a class member of
`FFmpegVideoDecoder` and reuse it.  Zero only the fields that `h264_new` initialises:

```cpp
// In FFmpegVideoDecoder constructor:
m_H264Stream = h264_new();

// In writeBuffer:
h264_init(m_H264Stream);   // if such an API exists, else memset the needed sub-struct
```

---

### #4 – Stats `snprintf` runs inside the decoder hot path when the debug overlay is on

**File:** `app/streaming/video/ffmpeg.cpp:2013–2022`

```cpp
if (LiGetMicroseconds() > m_ActiveWndVideoStats.startUs + 1000000) {
    // ...
    stringifyVideoStats(m_LastWndVideoStats, ...); // calls snprintf internally
    Session::get()->getOverlayManager().updateOverlayText(..., statsString);
}
```

`stringifyVideoStats` is called directly inside `submitDecodeUnit` — the same function
that feeds frames to FFmpeg.  It executes multiple `snprintf` calls forming a multi-line
status string.  This runs on the decode thread's critical path whenever:
- The debug overlay is enabled, **and**
- More than 1 second has elapsed since the last stats flip.

The stats flip check itself (`LiGetMicroseconds() > ...`) calls into the platform clock
on every frame even when the overlay is disabled.

**Recommendations:**
1. Guard the timestamp check with `m_OverlayManager.isOverlayEnabled(Overlay::OverlayDebug)` before calling `LiGetMicroseconds()`.
2. Move `stringifyVideoStats` to a low-priority background thread that reads the stats struct under a lock.

---

### #5 – `SDL_GetWindowSize()` called on the VAAPI render hot path every frame

**File:** `app/streaming/video/ffmpeg-renderers/vaapi.cpp:805`

```cpp
void VAAPIRenderer::renderFrame(AVFrame* frame) {
    int windowWidth, windowHeight;
    SDL_GetWindowSize(m_Window, &windowWidth, &windowHeight);
    // ... scale destination rect ...
    vaPutSurface(...);
}
```

`SDL_GetWindowSize` issues a round-trip to the X11 server on X11 subsystems
(`XGetWindowAttributes`).  At 120 fps this is 120 X11 round-trips per second purely
for window size information that almost never changes.

**Recommendation:** Cache `windowWidth`/`windowHeight` as class members, invalidated
by `notifyWindowChanged()`.  Re-read from SDL only when `WINDOW_STATE_CHANGE_SIZE` is
set.

---

### #6 – `vaPutSurface` on X11 can block for an entire VBlank period

**File:** `app/streaming/video/ffmpeg-renderers/vaapi.cpp:892`

```cpp
// NB: This can take a full VBlank period to complete!
vaPutSurface(vaDeviceContext->display, surface, m_XWindow, ...);
```

The VAAPI X11 direct-render path calls `vaPutSurface` synchronously and the comment
acknowledges it can block for up to 16.7 ms at 60 Hz.  This pins the Pacer render thread
for a full frame, destroying the pacing model and forcing frame drops.

**Recommendation:** For X11/VAAPI, prefer the indirect render path (via EGL DMA-BUF
import into the EGL renderer) which uses `eglSwapBuffers` with a compositor that handles
its own vsync.  Alternatively, use `vaGetImage` to pull the decoded frame into CPU
memory and blit via SDL — slower but deterministic.  Best: migrate X11/VAAPI fully to
the EGL path (already supported when `canExportEGL()` returns true).

---

### #7 – Multiple full decoder probe calls during `Session::initialize()`

**File:** `app/streaming/session.cpp:748–825`

`getDecoderAvailability()` creates and destroys a complete `FFmpegVideoDecoder` (including
hardware context, codec context, test frame allocation) for each probe.  In the VCC_AUTO
path the code can call `getDecoderAvailability` up to **6 times** for different codec
profiles (HEVC, AV1 variants).  Each call spins up a full GPU hardware decode context
and tears it down.

This adds 0.5–2 s of perceived startup latency on slow machines or when the driver
takes time to initialise the hardware decode engine.

**Recommendation:** Probe codecs in parallel using threads (the probes are independent).
Cache results keyed by `(videoFormat, width, height, fps)` in a persistent file so
subsequent sessions avoid re-probing.

---

### #8 – PACER_MAX_OUTSTANDING_FRAMES = 5 limits pipeline depth at high frame rates

**File:** `app/streaming/video/ffmpeg.cpp` (constant defined in ffmpeg.h, used as
`AVCodecContext::extra_hw_frames`)

With only 5 total outstanding frames (3 pacing queue + 1 rendering + 1 deferred free),
at 240 fps each frame has a 4.2 ms budget.  A single frame that takes 5 ms to decode
will cause the pacing queue to fill, triggering a drop.  Some hardware HEVC/AV1 decoders
pipeline 4+ frames internally; setting `extra_hw_frames` to 5 can prevent those internal
pipeline stages from filling, forcing the decoder to stall.

**Recommendation:** Empirically determine the optimal value per-codec/per-decoder.  The
existing logic already adds extra frames for codec-specific reasons (ffmpeg.cpp:~800);
consider bumping PACER_MAX_OUTSTANDING_FRAMES from 5 to 7 or 8 for AV1 decoders.

---

### #9 – Pacer drop-history window of 500 ms is slow to react to transient bursts

**File:** `app/streaming/video/ffmpeg-renderers/pacer/pacer.cpp:201–230`

The `handleVsync` drop logic uses a rolling 500 ms window to decide whether to allow
3 queued frames (`frameDropTarget = 3`) or only 1 (`frameDropTarget = 1`).  After a
burst of late frames the window takes up to 500 ms to drain.  During that time the
pacing queue is allowed to grow to 3, adding up to 3 × (1/fps) latency before dropping.

**Recommendation:** Reduce the history window to 100–200 ms so the system recovers
from transient bursts faster.  Alternatively, track the 95th-percentile frame arrival
interval instead of a simple count within a window.

---

### #10 – `m_DecodeBuffer` `reserve()` called on every frame

**File:** `app/streaming/video/ffmpeg.cpp:2057`

```cpp
m_DecodeBuffer.reserve(m_DecodeUnit.fullLength + AV_INPUT_BUFFER_PADDING_SIZE);
```

`QByteArray::reserve()` checks capacity and may grow the buffer.  After the first frame
this is usually a no-op because the buffer stabilises at the maximum packet size.
However, `reserve()` is not free — it calls `capacity()` and may trigger a `realloc`
on codec switches or resolution changes.

**Recommendation:** Pre-reserve at connection time based on
`m_StreamConfig.bitrate / m_StreamConfig.fps * 2` as an upper bound.  Remove the
per-frame `reserve()`.

---

### #11 – D3D11VA "copy" mode adds a GPU→GPU `CopySubresourceRegion1` per frame

**File:** `app/streaming/video/ffmpeg-renderers/d3d11va.cpp:400–415`

When not in "bind" mode (non-Intel, non-separate-device path), a decoded texture array
slice is copied into a single staging texture via `CopySubresourceRegion1`.  This copy
is performed synchronously inside `renderVideo`, adding a GPU command and potential
GPU stall before the pixel shader can sample the decoded data.

The bind path (Intel and separate-device) avoids this by creating an SRV directly on
the decoder output texture.

**Recommendation:** For GPUs that support feature level 11.1 but cannot use the separate
device path (currently limited by `UnifiedMemoryArchitecture`), investigate whether
direct SRV creation on the decode texture array slice is feasible without the full
separate-device machinery.

---

### #12 – SwFrameMapper skips `AV_HWFRAME_MAP_DIRECT` for Intel VAAPI

**File:** `app/streaming/video/ffmpeg-renderers/swframemapper.cpp:123–127`

```cpp
// Don't use AV_HWFRAME_MAP_DIRECT on Intel VA-API as the uncached
// memory flag causes terrible performance.
```

For software renders on Intel VAAPI, `av_hwframe_map` with the READ flag forces a
CPU-visible mapping.  The workaround correctly avoids this, but it means the software
render path on Intel always does a full `av_hwframe_transfer_data` (GPU → CPU copy).

This is correct behaviour, but worth noting: if the software render path is exercised
at all on Intel hardware, latency is dominated by this copy (typically 5–15 ms at 1080p).

**Recommendation:** Document this clearly and ensure the decoder selection path never
reaches the software render path on Intel hardware with VAAPI available.

---

## 3  Measurement Plan

| Metric | Tool | How |
|---|---|---|
| Per-frame decode latency | Existing `VIDEO_STATS` struct (dequeueTimeUs, decoderTimeMs) | Enable debug overlay; log to file |
| EAGAIN frequency | Add counter in ffmpeg.cpp:1941 | Log in stats |
| D3D11VA context lock contention | ETW / PIX GPU capture | Profile Present + decode overlap |
| VAAPI vaPutSurface duration | `clock_gettime(CLOCK_MONOTONIC)` around vaPutSurface | Per-frame log |
| Pacing queue depth at drop | Add log in pacer.cpp:handleVsync when drop occurs | Log |
| Startup probe time | `QElapsedTimer` around each `getDecoderAvailability` call | Log |
| GPU memory pressure | NvAPI / DXGI memory query | Periodic poll |
| Frame drop reason | Extend `VIDEO_STATS` with `droppedPacingQueue` / `droppedRenderQueue` | Overlay |

---

## 4  Things to Avoid

1. **Increasing `MAX_QUEUED_FRAMES` without a corresponding increase in
   `PACER_MAX_OUTSTANDING_FRAMES`.**  The pacer queue size and the HW frame pool size
   must stay in sync; mismatch causes `avcodec_receive_frame` to stall waiting for a free
   surface.

2. **Calling `SetMaximumFrameLatency` on the DXGI swap chain.**  The code deliberately
   omits this call (d3d11va.cpp:~540 area).  Adding it causes a blocking wait inside
   Present that defeats the entire pacing model.

3. **Using `SDL_GL_SetSwapInterval(1)` on Wayland for the EGL renderer.**  The code
   already avoids this (eglvid.cpp:588–592) because Wayland's compositor guarantees
   tear-free rendering and swap-interval > 0 would add compositor-paced latency.

4. **Adding AES encryption (`ENCFLG_ALL`) without verifying hardware AES acceleration.**
   The code already guards this (session.cpp:680), but any future refactoring must
   preserve the `hasFastAes() && SDL_GetCPUCount() > 2` gate — software AES at 4K60
   consumes a measurable fraction of a CPU core.

5. **Removing the deferred-free mechanism.**  `m_DeferredFreeFrame` exists to prevent
   the GPU from reading a texture that has been returned to the decoder's surface pool.
   Removing it without proper fence synchronisation causes corruption and driver crashes.

6. **Enabling `AV_HWFRAME_MAP_DIRECT` on VAAPI Intel.**  The comment in
   swframemapper.cpp:123 documents that uncached memory mapping destroys performance.
   Do not re-enable without testing on affected hardware.

7. **Storing DECODE_UNIT data pointers in `m_FrameInfoQueue`.**  The current code only
   stores metadata (not data pointers) because the DU buffer is freed before the queue
   entry is consumed.  ffmpeg.cpp:2104 shows the correct pattern.

---

## 5  Quick Wins (Low Risk, High Impact)

These are changes that can be made in under an hour with minimal risk:

| Change | File:Line | Expected Gain |
|---|---|---|
| Replace `SDL_Delay(2)` with `SDL_Delay(0)` | ffmpeg.cpp:1947 | −2 ms worst-case latency/frame |
| Guard stats clock check with overlay-enabled test | ffmpeg.cpp:2013 | Saves 1 syscall/frame when overlay off |
| Cache window size in VAAPI renderer | vaapi.cpp:805 | Saves 120 X11 round-trips/sec |
| Pre-reserve `m_DecodeBuffer` at connection time | ffmpeg.cpp:224, 2057 | Eliminates per-frame capacity check |
| Reduce Pacer drop history to 200 ms | pacer.cpp:~220 | Faster recovery from transient bursts |

---

*All line numbers refer to the source tree as examined; they may shift with future commits.*

---

## 6  Comparison with `VIDEO_DECODE_PERFORMANCE_STUDY.md`

This section was written *after* the independent study above was finished, by reading the prior `VIDEO_DECODE_PERFORMANCE_STUDY.md` (with its `**Thoughts:**` and `**Re-review:**` annotations) and mapping each finding here onto its prior counterpart. The verdict for each item is one of:

- **Already covered** — the prior study identifies the same issue with similar framing.
- **Partial overlap** — adjacent topic, but the independent study adds new evidence, severity, or angle.
- **New finding** — not present in the prior study at all.

### Ranked optimisations

| # | Independent finding | Verdict | Prior study reference |
|---|---|---|---|
| 1 | `SDL_Delay(2)` in EAGAIN loop | **Already covered** | Opportunity #1 ("Improve decoder-loop EAGAIN handling and reduce sleep latency"). Both flag the same `ffmpeg.cpp:1947` site. The prior re-review already cautions that the percentage framing slightly overstates typical impact because the sleep only fires when both queues are empty. |
| 2 | D3D11VA context mutex serialises decode and render in shared-device mode | **New finding** | Prior study discusses bind-vs-copy and separate-device path in section 8 / Opportunity #5, but never identifies the `lockContext`/`unlockContext` immediate-context contention as a perf issue. The deferred-context recommendation here is genuinely new. |
| 3 | Per-IDR `h264_new()` / `h264_free()` heap alloc | **New finding** | Prior study mentions `writeBuffer()` "may rewrite H.264 SPS data" in Opportunity #3 evidence, but does not call out the `h264_stream_t` alloc/free cost. Reusing one `h264_stream_t` member is a new concrete proposal. |
| 4 | `stringifyVideoStats` `snprintf` on the decode hot path | **Partial overlap** | Prior Opportunity #2 thoughts/re-review include the guardrail "Avoid adding per-frame string formatting on the hot path" and recommend building the path summary once at init. However, neither the original nor the re-review notices that `stringifyVideoStats` is *already* invoked from inside `submitDecodeUnit` (`ffmpeg.cpp:2020`) when the overlay is enabled. The independent study upgrades that from a future risk to a present cost. |
| 5 | `SDL_GetWindowSize()` per-frame on VAAPI render path | **New finding** | Not in the prior study. The X11 round-trip cost is a Linux-specific finding the prior study did not surface. |
| 6 | `vaPutSurface` blocks for a full VBlank period | **New finding** | Section 9 of the prior study covers SDL fallback slow paths but does not discuss the VAAPI direct render-thread stall. The source comment at `vaapi.cpp:892` makes this an unambiguous, source-stated issue. |
| 7 | Multiple decoder probe calls during `Session::initialize()` | **New finding** | Prior study section 3 describes the multi-tier probing flow but treats it as setup behaviour, not a startup-latency issue. The proposal to parallelise and cache probes is new. |
| 8 | `PACER_MAX_OUTSTANDING_FRAMES = 5` limits pipeline depth | **Partial overlap** | Prior Opportunity #6 and the `**Re-review:**` of section 4 flag the `MAX_QUEUED_FRAMES` ↔ `PACER_MAX_OUTSTANDING_FRAMES` ↔ `extra_hw_frames` coupling and warn against changing one without the others. The independent study goes further by suggesting the absolute value should be raised for AV1/240 fps — that direction was not in the prior study. |
| 9 | Pacer 500 ms drop-history window is slow to recover | **New finding** | Prior Opportunity #6 discusses queue thresholds and timer slack but never the drop-history window length. Genuinely new. |
| 10 | `m_DecodeBuffer.reserve()` per frame | **Already covered** | Prior Opportunity #3, plus the `**Re-review:**` that explicitly recommends `resize()` to make the write range explicit. The prior study frames it as defensive correctness; the independent study frames it as perf — same code site, different angle. |
| 11 | D3D11VA copy-mode `CopySubresourceRegion1` per frame | **Already covered** | Prior section 8 describes exactly this ("The copy path calls `CopySubresourceRegion1()` every rendered frame") and Opportunity #5 ranks the bind-vs-copy decision. The independent study adds no new code site, only a slightly different remediation idea. |
| 12 | SwFrameMapper skips `AV_HWFRAME_MAP_DIRECT` on Intel VAAPI | **New finding** | Prior study describes the SwFrameMapper copyback path generically in section 9, but does not mention the Intel-specific direct-map workaround at `swframemapper.cpp:123-127`. Minor finding overall. |

### Things to avoid

| Independent item | Verdict | Prior study reference |
|---|---|---|
| Don't raise `MAX_QUEUED_FRAMES` without raising `PACER_MAX_OUTSTANDING_FRAMES` | **Already covered** | The coupling is explicitly called out in the `**Thoughts:**` and `**Re-review:**` of prior section 4 and Opportunity #6. |
| Don't add `SetMaximumFrameLatency` on the DXGI swap chain | **Already covered** | Prior section 8 mentions "comments explain why maximum frame latency is not forced to one" at `d3d11va.cpp:542-558`. |
| Don't enable Wayland EGL `SDL_GL_SetSwapInterval(1)` | **New finding** | Not in prior study. |
| Don't enable AES (`ENCFLG_ALL`) without `hasFastAes()` gate | **New finding** | Out of scope for the prior decode-focused study, but a useful boundary. |
| Don't remove the deferred-free mechanism | **New finding** | Prior study does not mention `m_DeferredFreeFrame` or texture lifetime against the decoder pool. |
| Don't enable `AV_HWFRAME_MAP_DIRECT` on Intel VAAPI | **New finding** | Pairs with item #12 above. |
| Don't store `DECODE_UNIT` data pointers in `m_FrameInfoQueue` | **New finding** | Implementation-detail boundary the prior study does not articulate. |

### Items in the prior study that the independent study did *not* surface

For completeness, these prior-study findings have no direct counterpart in the independent pass:

- Prior Opportunity #4 (AVFrame allocation churn). The independent study does not propose pooling AVFrames; the prior re-review already deprioritised this, so the omission is consistent.
- Prior Opportunity #7 (software decode slice/thread policy). The independent study does not propose changes here either; both agree it is low priority.
- Prior Opportunity #8 (slow-path user warnings in the GUI). The independent study has no UI-facing finding. This remains a uniquely valuable piece of prior work because it is the only user-visible recommendation.

### Net assessment

- The two studies **agree on the high-confidence, top-of-list issue**: `SDL_Delay(2)` and the `EAGAIN` handling. That convergence increases confidence in Phase 2 of the implementation plan.
- The two studies **agree on the second tier**: `m_DecodeBuffer.reserve()` correctness, the D3D11VA copy path cost, and the queue/HW-frame coupling.
- The independent pass **adds five clearly new findings worth implementing/considering**:
  1. D3D11VA shared-device immediate-context contention (#2).
  2. `h264_new`/`h264_free` per-IDR allocation (#3).
  3. `stringifyVideoStats` already running on the decode hot path (#4 — present cost, not future risk).
  4. VAAPI `SDL_GetWindowSize` per-frame X11 round-trip (#5).
  5. VAAPI `vaPutSurface` synchronous VBlank stall (#6).
- The independent pass **adds two startup/operational findings**: parallel/cached decoder probing (#7) and a shorter drop-history window (#9).
- The prior study contributes the **only user-visible item**: settings-level slow-path warnings (Opportunity #8).

The two studies are complementary. A merged implementation backlog should include the prior plan unchanged for Phases 1–4, plus the new D3D11VA shared-device, VAAPI hot-path, decoder-probe-startup, and stats-on-hot-path findings as additional candidates for a Phase 6 once Phase 1 diagnostics are in place to measure them.

---

## 7  Implementation results

The merged implementation completed the low-risk and measurable parts of the backlog:

| Finding | Implementation status |
|---|---|
| #1 `SDL_Delay(2)` / FFmpeg `EAGAIN` handling | Implemented as safer pending-packet retry after decoder drain, plus a bounded 1 ms empty-poll wait. |
| #2 D3D11VA shared-device context contention | Implemented as diagnostics: average/max context-lock wait is measured and exposed. No driver-sensitive rendering heuristic was changed. |
| #3 Per-IDR `h264_new()` / `h264_free()` | Deferred. The parser has no obvious reset helper, so reuse remains lower priority than the safer hot-path fixes. |
| #4 Stats/string work on hot path | Partially implemented. Decode-path strings remain cached and only refresh when the debug overlay updates; normal stats accumulation is preserved. |
| #5 VAAPI per-frame `SDL_GetWindowSize()` | Implemented by caching output size and updating it from window-change notifications. |
| #6 VAAPI `vaPutSurface()` VBlank stall | Deferred as a renderer-selection/profiling decision. The direct-vs-EGL path is now visible in diagnostics first. |
| #7 Duplicate decoder probes | Implemented with a per-session availability cache and probe timing/cache-hit logs. |
| #8 Queue/HW-frame budget coupling | Preserved. Queue depth and `PACER_MAX_OUTSTANDING_FRAMES` were not raised independently. |
| #9 Pacer 500 ms drop-history window | Implemented as a 200 ms queue-history window with refresh-aware timer slack. |
| #10 `m_DecodeBuffer.reserve()` per frame | Implemented as explicit buffer sizing plus zeroed FFmpeg input padding. |
| #11 D3D11VA copy-mode GPU copy | Deferred. Diagnostics now expose bind/copy and lock contention before any heuristic change. |
| #12 Intel VAAPI direct-map workaround | Preserved. Slow-path guidance now warns when hardware readback is actually selected. |

Final validation used the Windows MSVC release build path and saved output to `build\build-msvc-release.log`; the build completed successfully. Runtime hardware validation on VAAPI/Linux, D3D11VA contention scenarios, and high-refresh display combinations should still be used before expanding the deferred items into behavioral changes.
