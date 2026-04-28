# Video Decode Performance Study

## Executive summary

Moonlight Qt already has a performance-oriented video pipeline. It prefers hardware decoding, tries to keep decoded frames on the GPU when possible, uses a pull-based decoder thread to avoid blocking the network/depacketizer path, and has renderer-specific paths for D3D11VA, DXVA2, Vulkan/libplacebo, VAAPI, VDPAU, DRM, EGL, CUDA, VideoToolbox, and SDL fallback.

The most promising optimization work is not "make decoding hardware accelerated" because that is already the normal path. The best candidates are smaller latency and overhead reductions around the decode loop, frame/packet memory handling, renderer diagnostics, D3D11VA bind-vs-copy decisions, and high-refresh frame pacing. These should be measured before changing behavior, because this code intentionally optimizes for low latency and some "performance" changes could make streaming feel worse.

Recommended first implementation candidates after measurement:

1. Improve FFmpeg decoder-loop handling around `EAGAIN` and the current `SDL_Delay(2)` fallback.
2. Add better decode-path diagnostics to show whether the stream is zero-copy, copyback, D3D11VA direct-bind, D3D11VA copy, SDL texture upload, or CPU color conversion.
3. Audit packet/frame allocation and copy behavior, especially `m_DecodeBuffer` growth and per-frame `AVFrame` allocation.
4. Measure and tune `Pacer` behavior for 120 Hz, 144 Hz, 240 Hz, and mismatched stream/display rates.
5. Add user-facing warnings or guidance when settings force slow paths such as software decode, YUV 4:4:4 without a hardware renderer, or SDL CPU conversion.

**Thoughts:** All five recommendations are well-targeted and verifiable in the current source. The ordering is correct: diagnostics (item 2) should land before any behavioral change so improvements can be measured rather than guessed. Item 1 (the `EAGAIN` / `SDL_Delay(2)` loop) is the highest-confidence latency win because the FIXME and the fixed sleep are still in `app\streaming\video\ffmpeg.cpp:1935-1948` exactly as described. Items 3 and 4 are correctly framed as conditional on measurement; nothing here is speculative refactoring for its own sake. The summary explicitly avoids the common trap of "make decoding hardware accelerated" when that is already the default path, which is the right framing.

**Re-review:** This comment is valid and keeps the work grounded. One refinement: diagnostics should be considered candidate 1 in implementation order even if `EAGAIN` remains the first true optimization candidate. That distinction matters because it prevents a latency fix from being judged without knowing whether the run was hardware zero-copy, copyback, SDL conversion, or software decode.

## Plain-language model of the pipeline

Video streaming has three separate pieces that can each affect performance:

1. **Decode** - compressed H.264, HEVC, or AV1 data is turned into raw video frames. This is fast when done by the GPU/decoder block and much slower when done in software on the CPU.
2. **Render** - decoded frames are drawn to the window. The best case is zero-copy GPU rendering. Slower paths copy frames between GPU resources, read frames back to CPU memory, or convert colors on the CPU before uploading to a texture.
3. **Pacing** - frames are released to the display at the right time. Good pacing reduces stutter, but too much queueing increases latency.

The code already tracks these pieces in `VIDEO_STATS`: received/decoded/rendered frames, network drops, pacer drops, decode time, pacer queue time, render time, RTT, and FPS values (`app\streaming\video\decoder.h:11-34`). The debug overlay formats those values in `FFmpegVideoDecoder::stringifyVideoStats()` (`app\streaming\video\ffmpeg.cpp:805-977`).

**Thoughts:** The three-stage decompose (Decode / Render / Pacing) is the right mental model for this codebase and matches how the source is split (`FFmpegVideoDecoder`, `IFFmpegRenderer` implementations, and `Pacer`). The stats coverage claim is accurate: timing values are tracked, but path classification (zero-copy vs copyback, bind vs copy, swscale active) is not, which directly motivates opportunity #2 below. This is a good place to call out that "rendering" here also covers color conversion, which is where SDL fallback can silently become CPU-bound — that nuance is captured later in section 9 but is worth keeping in mind here.

**Re-review:** Agreed. The added thought correctly identifies the missing observability layer. I would also include packet/queue behavior under this model when reading later sections: decode timing alone will not explain stutter if the issue is depacketizer availability, pacer queue delay, or render-thread scheduling.

## How it works now

### 1. Session setup chooses stream settings and decoder capabilities

`Session::initialize()` reads streaming preferences, initializes SDL video, creates a hidden test window for decoder probing, initializes Limelight video/audio callbacks, sets resolution/FPS/bitrate, and builds a prioritized list of supported video formats (`app\streaming\session.cpp:592-845`).

The initial codec order favors advanced formats and profiles:

1. AV1 10-bit 4:4:4
2. AV1 Main10
3. HEVC RExt 10-bit 4:4:4
4. HEVC Main10
5. AV1 8-bit 4:4:4
6. AV1 Main8
7. HEVC RExt 8-bit 4:4:4
8. HEVC Main
9. H.264 4:4:4
10. H.264

Auto mode then deprioritizes formats based on decoder availability, HDR mode, hardware support, and platform-specific behavior (`app\streaming\session.cpp:726-827`). For example, HEVC may be deprioritized if it is not hardware accelerated, and AV1 is deprioritized unless it is useful for HDR or a platform-specific HEVC limitation.

`Session::chooseDecoder()` first tries `SLVideoDecoder` when built, then `FFmpegVideoDecoder` (`app\streaming\session.cpp:286-347`). On normal desktop builds, FFmpeg is the major path.

**Thoughts:** Accurate. The codec ordering preferring 10-bit and 4:4:4 first is intentional but risky on weak hardware because, if probing accepts the format but the renderer falls back to CPU conversion, the user pays a hidden cost. That is exactly what opportunity #8 (slow-path warnings) addresses, and the link between this section and #8 is correct. Nothing to add to the implementation order here.

**Re-review:** Valid, with one nuance: "weak hardware" is not the only risk. A powerful machine can still hit the slow path if the selected renderer lacks native format support or if a platform build falls back to SDL. The warning logic should therefore key off the actual selected path, not a device-strength assumption.

### 2. FFmpeg uses a pull-based decoder thread

`FFmpegVideoDecoder::getDecoderCapabilities()` always adds `CAPABILITY_PULL_RENDERER` (`app\streaming\video\ffmpeg.cpp:88-155`). Limelight documents this mode as a renderer-managed decode/render thread model where the client calls `LiWaitForNextVideoFrame()`, `LiPollNextVideoFrame()`, and `LiCompleteVideoFrame()` (`moonlight-common-c\moonlight-common-c\src\Limelight.h:950-961`).

Because this capability is set, `Session::populateDecoderProperties()` does not provide a push callback to Limelight (`app\streaming\session.cpp:508-553`). Instead, `moonlight-common-c` queues decode units in `VideoDepacketizer.c` (`moonlight-common-c\moonlight-common-c\src\VideoDepacketizer.c:512-539`) and Moonlight Qt pulls them from `FFmpegVideoDecoder::decoderThreadProc()` (`app\streaming\video\ffmpeg.cpp:1810-1985`).

This design is good for latency isolation: network packet handling is not forced to do decode/render work directly.

**Thoughts:** Confirmed. `CAPABILITY_PULL_RENDERER` is unconditionally set in `getDecoderCapabilities()`, and the Limelight contract for pull mode is what the decoder thread implements. The latency-isolation framing is correct: the pull design is precisely what makes opportunity #1 (decoder loop tuning) a localized change rather than a cross-cutting refactor.

**Re-review:** This remains correct. The important implementation constraint is that any replacement for `SDL_Delay(2)` should respect the pull-renderer ownership model: Limelight still owns decode-unit lifetime until `LiCompleteVideoFrame()` is called, so wait/drain changes must not hold frames indefinitely or complete them too early.

### 3. FFmpeg decoder and renderer probing is multi-tiered

`FFmpegVideoDecoder::initialize()` tries decoders in this broad order (`app\streaming\video\ffmpeg.cpp:1614-1746`):

1. User-selected environment variable decoder hints.
2. Normal hardware-acceleration decoders, first pass.
3. Non-standard hardware decoders with zero-copy formats.
4. Non-standard hardware decoders even if copyback may be required.
5. Remaining hardware-acceleration passes.
6. Software decoders, unless hardware-only mode is forced.

Hardware renderer selection prefers platform-specific zero-copy/direct paths:

- Windows: D3D11VA first, DXVA2 later (`app\streaming\video\ffmpeg.cpp:991-1115`).
- Linux/BSD: VAAPI, VDPAU, DRM, CUDA, Vulkan/libplacebo, EGL, SDL depending build support (`app\app.pro:238-437`).
- macOS: VideoToolbox with Metal preferred, AVSampleBuffer fallback (`app\streaming\video\ffmpeg.cpp:1006-1061`).

`tryInitializeRenderer()` may run a test decode before real streaming. For some decoders it creates a separate test decoder because dimensions or codec state cannot be safely reused (`app\streaming\video\ffmpeg.cpp:1117-1237`).

**Thoughts:** The multi-tier probing description is accurate. The non-obvious takeaway is that the chosen decoder/renderer combo at runtime is the result of multiple fallbacks, which is exactly why opportunity #2 (path diagnostics) matters more than it first looks: without surfacing what was actually selected, users and developers can be measuring an unintended path. No change to the recommendation order needed.

**Re-review:** Agreed. This also means the diagnostics should report both the requested/preferred path and the final selected path when possible. A log that only says "hardware decode requested" would be misleading if probing later falls back to software decode, copyback, or SDL rendering.

### 4. FFmpeg context is configured for low latency

`completeInitialization()` sets FFmpeg decoder flags for low delay, output of corrupt/missing-reference frames, and error recognition so Moonlight can request a key frame on decode failure (`app\streaming\video\ffmpeg.cpp:513-527`).

Software decode uses slice threading with up to four slices/threads (`app\streaming\video\ffmpeg.cpp:529-537`). The capabilities code also asks the host encoder for up to four slices per frame when software decoding is active (`app\streaming\video\ffmpeg.cpp:101-107`). Hardware decode uses one FFmpeg thread and relies on the GPU decoder.

The context also sets `extra_hw_frames = PACER_MAX_OUTSTANDING_FRAMES` to avoid starving hardware decoder surface pools while frames are queued for pacing/rendering (`app\streaming\video\ffmpeg.cpp:546-548`).

**Thoughts:** Correct, and worth emphasizing: `extra_hw_frames` is tied to `PACER_MAX_OUTSTANDING_FRAMES`, which itself is tied to `MAX_QUEUED_FRAMES` via the `static_assert` in pacer.cpp. That coupling means opportunity #6 (pacer tuning) cannot freely change `MAX_QUEUED_FRAMES` without also revisiting the hardware surface pool size. The study should keep that invariant in mind during Phase 3, and the Phase 3 plan should explicitly call it out.

**Re-review:** Strong and important. This is not just a documentation concern: if Phase 3 reduces or increases queue depth without adjusting the hardware-frame allowance, it can either waste surfaces or reintroduce decoder starvation. The implementation plan should treat queue depth and hardware surface budget as one coordinated setting.

### 5. Incoming frames are copied into a contiguous packet buffer

`FFmpegVideoDecoder::submitDecodeUnit()` receives a decode unit from Limelight. The decode unit contains a chain of buffers, not one contiguous allocation. The code computes the required size, reserves `m_DecodeBuffer`, copies each entry through `writeBuffer()`, sets up one `AVPacket`, and calls `avcodec_send_packet()` (`app\streaming\video\ffmpeg.cpp:1987-2108`).

This copy is probably necessary because FFmpeg packet input is contiguous and the Limelight decode-unit buffers are only valid during the callback/pull completion window. Still, this path is worth auditing because it happens for every frame.

**Thoughts:** Verified. `m_DecodeBuffer` is a `QByteArray` constructed at 1 MiB (`ffmpeg.cpp:224`) and `submitDecodeUnit()` calls `reserve()` rather than `resize()` before writing through `data()` (`ffmpeg.cpp:2057-2065`). On all currently supported Qt 6 versions `data()` is non-null after `reserve()` for a non-empty `QByteArray`, so this works in practice, but it is technically writing past `size()`. Switching to `resize()` (or using a raw `std::vector<uint8_t>`) would make the intent unambiguous. This is a correctness-clarity improvement rather than a perf win, which is exactly how opportunity #3 frames it.

**Re-review:** The conclusion is right, but the reason should be stated more carefully. The code is safe from a null pointer mostly because `m_DecodeBuffer` starts with a 1 MiB size, not because `reserve()` alone guarantees a writable range. The real issue is that when a packet grows beyond the current `size()`, `reserve()` increases capacity but not logical size, so writing through `data()` relies on capacity rather than the array's defined size. `resize(requiredBufferSize + AV_INPUT_BUFFER_PADDING_SIZE)` is the clearer fix.

### 6. Decoded frames are handed to Pacer

`decoderThreadProc()` waits for queued input, sends it to FFmpeg, receives decoded frames, attaches HDR metadata if needed, applies small cropping fixes for encoder padding, records decode timing, and calls `m_Pacer->submitFrame(frame)` (`app\streaming\video\ffmpeg.cpp:1810-1985`).

When `avcodec_receive_frame()` returns `EAGAIN`, it polls Limelight for another frame and submits that if available. If no input or output is available, it sleeps for 2 ms (`app\streaming\video\ffmpeg.cpp:1935-1948`). There is an existing FIXME for proper `EAGAIN` handling on `avcodec_send_packet()` (`app\streaming\video\ffmpeg.cpp:1941-1943`).

**Thoughts:** Verified — line numbers and FIXME match. The 2 ms sleep is reached only when both the FFmpeg output queue and the Limelight input queue are empty, which is the steady-state idle case at low load. Where it bites is when the decoder briefly stalls or when input arrives just after the sleep starts. A condition variable or short bounded wait on the depacketizer queue would be the proper fix, but it must not regress the steady-state idle case into a busy loop. Phase 2's plan correctly flags this risk.

**Re-review:** Valid. The added thought correctly lowers the alarm level from "every frame pays 2 ms" to "occasional sleeps can become visible at high FPS." For implementation, the safest intermediate step may be to first drain FFmpeg correctly on send/receive `EAGAIN`, then separately evaluate whether the idle wait needs replacing.

### 7. Pacer controls queue depth, frame drops, and render timing

`Pacer` has two queues: one for pacing and one for rendering. It caps queued frames and drops old frames when queues grow too deep (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:16-23`, `app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:394-419`).

If frame pacing is enabled and a platform V-sync source exists, Pacer creates a V-sync thread. On Windows it can use `DxVsyncSource`; on Wayland it can use `WaylandVsyncSource` (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:262-325`). Rendering uses a dedicated render thread when supported (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:137-177`). Otherwise, it posts an SDL user event and renders on the main thread (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:179-197`).

This is already a good low-latency design, but the queue thresholds and timer slack should be measured at high refresh rates.

**Thoughts:** Accurate. The pacing queue wait at `pacer.cpp:246` uses `SDL_max(timeUntilNextVsyncMillis, TIMER_SLACK_MS) - TIMER_SLACK_MS`, so the 3 ms slack is subtracted from every wait — at 240 Hz that subtracts ~72% of the frame interval, which means the pacer effectively never sleeps and burns CPU at very high refresh. That is a stronger argument for opportunity #6 than the study currently makes; Phase 3 should consider scaling `TIMER_SLACK_MS` from the OS timer resolution (e.g., 1 ms on modern Windows with `timeBeginPeriod`/high-resolution waitable timers) rather than a flat 3 ms.

**Re-review:** The direction is correct, but "effectively never sleeps" is too strong. At 240 Hz there can still be a short wait when the computed time until the next V-sync is above the slack; the problem is that the remaining wait can be very small and therefore CPU-sensitive. The concrete recommendation should be "measure wakeups, CPU usage, and queue delay before lowering slack," not simply "lower the constant."

### 8. Windows D3D11VA is already optimized, but has several mode-dependent paths

D3D11VA tries to use the GPU attached to the display first to avoid inter-GPU copies, then falls back to other GPUs if necessary (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:507-530`).

It decides whether to use separate decode/render devices based on extended resource sharing and fence support (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:361-398`). It then decides whether to bind decoder output textures directly or copy into a render texture. The current heuristic binds output textures on Intel GPUs or when using separate devices; otherwise it uses copy mode (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:400-421`).

The direct-bind path indexes into shader resource views based on FFmpeg's texture index. The copy path calls `CopySubresourceRegion1()` every rendered frame (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:1017-1039`).

D3D11VA presents with `Present(0, flags)` and uses tearing flags when V-sync is disabled and the OS/GPU supports it (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:573-606`, `app\streaming\video\ffmpeg-renderers\d3d11va.cpp:766-809`). Swapchain buffer count is deliberately high enough to avoid starvation, while comments explain why maximum frame latency is not forced to one (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:542-558`).

**Thoughts:** Verified at `d3d11va.cpp:414` (`m_BindDecoderOutputTextures = adapterDesc.VendorId == 0x8086 || separateDevices`). The vendor-id heuristic is exactly the kind of code that should not be casually changed: it's a workaround derived from observed behavior on specific drivers. Opportunity #5 correctly classifies this as high-risk and gates it on measurement plus user-facing override. The existing `D3D11VA_FORCE_BIND` and `D3D11VA_FORCE_SEPARATE_DEVICES` env vars (`d3d11va.cpp:400`, `:361-415`) are the right escape hatch for testing without code changes, which means Phase 5 (or the eventual #5 work) can lean on them rather than introducing new heuristics.

**Re-review:** Agreed. One small correction: these environment variables are best suited for developer testing and advanced troubleshooting, not general user-facing guidance. For normal users, diagnostics should explain the active bind/copy mode without telling them to force a mode unless a specific bug workaround is documented.

### 9. SDL fallback can become much slower

SDL rendering is a fallback and compatibility path. For 8-bit 4:2:0 formats, SDL can upload YUV/NV12/NV21 and let the renderer backend do conversion (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:96-131`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:456-523`).

For 10-bit or YUV 4:4:4 formats, SDL cannot render those formats natively, so the code uses swscale to convert to RGB on the CPU, then uploads the converted texture (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:98-113`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:524-560`). The code already enables up to four swscale threads on newer FFmpeg/libswscale (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:343-390`).

If SDL is used as a frontend for hardware decoded frames, frames may need to be mapped or copied from GPU memory to CPU memory first (`app\streaming\video\ffmpeg-renderers\swframemapper.cpp:16-100`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:274-292`). This is a major slow path and should be visible to the user or debug overlay.

**Thoughts:** This section is the most important one for user-impacting wins. The combination of "10-bit or 4:4:4 → CPU swscale to RGB → texture upload" is exactly the failure mode that causes "performance is bad" reports on otherwise-capable hardware. Surfacing this path in diagnostics (Phase 1) and warning when settings force it (Phase 4) directly addresses it. The framing here is correct.

**Re-review:** Correct. This is probably the clearest place where a non-invasive change can reduce support burden: users do not need lower-level decoder details, they need to know "this selected quality option is causing CPU conversion on your current renderer." The future UI warning should therefore reuse the same wording as the diagnostic path summary to avoid conflicting explanations.

## Ranked optimization opportunities

### 1. Improve decoder-loop `EAGAIN` handling and reduce sleep latency

**Why it matters:** The decoder thread sleeps for 2 ms when FFmpeg has no output and Limelight has no immediately available input (`app\streaming\video\ffmpeg.cpp:1935-1948`). At 60 FPS, 2 ms is about 12 percent of a frame interval. At 120 FPS it is about 24 percent. At 240 FPS it is about 48 percent. It may not happen often on healthy hardware, but if it does, it can directly increase frame queue delay.

**Current source evidence:**

- `decoderThreadProc()` loops around `avcodec_receive_frame()`.
- On `AVERROR(EAGAIN)`, it polls for another decode unit and calls `submitDecodeUnit()`.
- If no decode unit is available, it calls `SDL_Delay(2)`.
- A FIXME notes `EAGAIN` from `avcodec_send_packet()` is not handled properly.

**Possible approach:**

1. Treat FFmpeg's send/receive state machine more explicitly: drain available frames before sending more packets, and if send returns `EAGAIN`, receive frames until send can proceed.
2. Replace fixed `SDL_Delay(2)` with a wait strategy tied to Limelight's queue wakeup where possible.
3. Measure CPU usage to avoid turning the decoder thread into a busy spin.

**Expected impact:** Potentially lower frame queue delay and better high-FPS latency.

**Risk:** A wrong send/receive loop can deadlock, spin the CPU, or starve packet submission. This should be implemented with careful logging and tested with H.264, HEVC, AV1, hardware decode, and software decode.

**Thoughts:** Strong opportunity. The "12 / 24 / 48 percent of a frame interval" framing is correct math but slightly overstates the typical impact, because the 2 ms sleep only fires when both queues are empty, which is not every frame. Still, even one such stall every few frames at 240 FPS is enough to produce a visible micro-stutter, and the FIXME shows the original author already knew the path was unfinished. The proposed approach of explicit drain-then-send is the standard FFmpeg state-machine pattern and is the right shape. The risk callout (deadlock / spin) is real and is the reason this should ship behind a small, reviewable change rather than a rewrite.

**Re-review:** This is the best-balanced comment in the file: it validates the opportunity while correcting the severity. I agree with the implementation guardrail. The first patch should not try to redesign threading; it should make FFmpeg send/receive state transitions explicit and add enough logging to detect spin, starvation, or repeated `EAGAIN`.

### 2. Add decode-path diagnostics before changing heuristics

**Why it matters:** The code has several very different performance paths: zero-copy hardware, hardware decode with copyback, D3D11VA direct-bind, D3D11VA copy, SDL texture upload, and SDL CPU swscale conversion. Without clear diagnostics, it is hard to know whether a user has a true decode bottleneck or has accidentally selected a slow renderer/settings combination.

**Current source evidence:**

- D3D11VA logs `"Decoder texture access: bind/copy"` and fence type (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:417-421`).
- SDL logs CPU color conversion only when a texture is created for an unsupported format (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:327-333`).
- The performance overlay reports decode/pacer/render timing but not the chosen zero-copy/copyback/conversion mode (`app\streaming\video\ffmpeg.cpp:956-969`).

**Possible approach:**

1. Add a small renderer path summary string to `IFFmpegRenderer` or `FFmpegVideoDecoder`.
2. Include path details in startup logs and optionally in the debug overlay:
   - Codec and decoder name.
   - Renderer name.
   - Hardware vs software decode.
   - Zero-copy vs copyback.
   - D3D11VA bind vs copy.
   - CPU swscale active/inactive.
3. Keep it diagnostic-only at first.

**Expected impact:** No direct FPS improvement, but high value for finding real bottlenecks and avoiding wrong optimization work.

**Risk:** Low if implemented as logging/overlay only. Avoid adding per-frame string formatting on the hot path.

**Thoughts:** Highest leverage relative to risk. This is the right first implementation step because every other recommendation either depends on knowing the active path (5, 8) or benefits from being able to verify a change did not regress a different path (1, 3, 4, 6, 7). The hot-path warning is correct: the path summary should be a string built once at renderer init and stored, with the overlay just appending it — no per-frame formatting.

**Re-review:** Agreed. The only addition is that some flags may become known only after renderer initialization or first texture creation (for example SDL CPU conversion). The design should allow the summary to be updated when a renderer discovers its actual conversion/copy behavior, while still avoiding per-frame string building.

### 3. Audit packet reassembly and `m_DecodeBuffer` growth

**Why it matters:** Every frame is copied into `m_DecodeBuffer` before FFmpeg receives it (`app\streaming\video\ffmpeg.cpp:2050-2077`). This may be necessary, but the implementation should be verified for allocation behavior and safe buffer sizing.

**Current source evidence:**

- `m_DecodeBuffer` is initialized to 1 MiB (`app\streaming\video\ffmpeg.cpp:220-239`).
- `submitDecodeUnit()` calls `m_DecodeBuffer.reserve(requiredBufferSize + AV_INPUT_BUFFER_PADDING_SIZE)` then writes to `m_DecodeBuffer.data()` through `writeBuffer()` (`app\streaming\video\ffmpeg.cpp:2050-2066`).
- `writeBuffer()` uses `memcpy()` for each buffer-list entry and may rewrite H.264 SPS data (`app\streaming\video\ffmpeg.cpp:1748-1802`).

**Possible approach:**

1. Confirm whether `reserve()` alone is sufficient for the intended `QByteArray::data()` writes on the supported Qt versions, or whether `resize()` should be used to make the write range explicit.
2. Track maximum packet size and allocation count during streams.
3. Consider only growing the buffer, never shrinking it during a session.
4. Investigate whether FFmpeg can safely accept ref-counted packet buffers backed by a single allocation from Limelight, but assume this is risky unless ownership can be proven.

**Expected impact:** Mostly reduced allocator overhead and improved correctness clarity. Copy removal may not be practical because FFmpeg wants contiguous packet data and Limelight buffer ownership is short-lived.

**Risk:** Medium. Packet lifetime mistakes can cause decoder corruption or crashes. Keep any first change limited to safer buffer sizing/reuse, not zero-copy packet ownership.

**Thoughts:** The framing is right. The single cheapest, safest change here is the `reserve()` → `resize()` clarification — it makes the write range explicit without changing behavior in practice. Anything beyond that (zero-copy from Limelight, custom allocators) carries lifetime risk that the study correctly downgrades. I would not chase this in isolation; bundle the buffer-sizing clarification into Phase 2 alongside the `EAGAIN` work and stop there for now.

**Re-review:** Correct. I would slightly raise the priority of the `resize()` change from "clarification" to "defensive correctness" because it removes dependence on writing into reserved capacity. It still should not become a broader packet ownership rewrite; that would be a separate and much riskier investigation.

### 4. Investigate AVFrame allocation churn

**Why it matters:** `decoderThreadProc()` allocates an `AVFrame` for output attempts and hands successful frames to `Pacer`, which frees them after rendering (`app\streaming\video\ffmpeg.cpp:1830-1934`, `app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:332-350`). This is straightforward and safe, but it allocates continuously during playback.

**Possible approach:**

1. Measure allocation overhead first. Hardware decode/render time may dwarf this cost.
2. If measurable, consider a small frame wrapper/pool where frames are `av_frame_unref()` and reused after Pacer/rendering releases them.
3. Preserve ownership boundaries: once a frame is queued to Pacer, the decoder thread must not reuse it.

**Expected impact:** Probably small for GPU-heavy paths, possibly useful on CPU-constrained systems.

**Risk:** Medium. Frame lifetime is shared across decode, Pacer, and renderer. A pool would need very clear ownership.

**Thoughts:** Correctly deprioritized. `av_frame_alloc()` is a small malloc; the heavy data buffers are reference-counted and reused by FFmpeg's internal pool already. A user-space pool would mostly save the small struct allocation, which is unlikely to be measurable against decode/render times. I'd defer this until Phase 1 diagnostics show allocation overhead actually showing up in profiles, and otherwise leave it alone.

**Re-review:** Agreed, and this should probably be removed from near-term implementation scope entirely unless profiling proves otherwise. The ownership handoff to Pacer/renderers is more important than saving the small `AVFrame` allocation. Pooling here would increase lifetime complexity in one of the most crash-sensitive paths.

### 5. Refine D3D11VA bind-vs-copy decisions only with measurements

**Why it matters:** D3D11VA has two hot paths:

- **Bind:** sample decoder output textures directly.
- **Copy:** copy the decoded texture to a render texture with `CopySubresourceRegion1()` before drawing.

The current heuristic uses direct binding on Intel GPUs or when separate decode/render devices are active, and copy mode otherwise (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:400-421`). Comments mention that binding avoids a significant extra-copy cost on Intel and improves render times on a Ryzen 3300U system when separate devices are used.

**Possible approach:**

1. Add diagnostics and optionally hidden timing counters for bind vs copy.
2. Test NVIDIA, AMD, Intel, hybrid laptop, and multi-GPU scenarios.
3. Consider an environment override or advanced setting only if users need a persistent workaround. Environment overrides already exist for `D3D11VA_FORCE_BIND` and `D3D11VA_FORCE_SEPARATE_DEVICES` (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:361-415`).

**Expected impact:** Potentially meaningful render-time reduction on some GPUs.

**Risk:** High if changed broadly. D3D11 decode-output synchronization is driver-sensitive, and the current code has vendor-specific safeguards for known broken paths.

**Thoughts:** This one I would explicitly mark "do not touch in this initiative." The current heuristic exists for documented, vendor-specific reasons, and the existing env-var overrides already give power users an escape valve. Phase 1 diagnostics surfacing the active mode (bind vs copy) is enough to satisfy the support and debugging case. Anything more invasive needs a multi-GPU test matrix that this project does not currently have.

**Re-review:** Strongly agreed. The wording is appropriately conservative. The only work that belongs in the current initiative is observability: expose bind/copy mode and maybe record render timing by mode. Changing the heuristic should be a future, evidence-backed task with GPU/vendor-specific validation.

### 6. Tune Pacer for high refresh rates

**Why it matters:** Queue thresholds and timer slack are fixed constants. `Pacer` uses `MAX_QUEUED_FRAMES = 3` and `TIMER_SLACK_MS = 3` (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:16-30`). A 3 ms slack is reasonable at 60 FPS but becomes a large fraction of the frame interval at 240 FPS.

**Possible approach:**

1. Measure frame queue delay and pacer drops at 60, 120, 144, and 240 FPS.
2. Consider deriving timer slack from refresh interval instead of a fixed 3 ms.
3. Consider separate queue policy for stream FPS greater than display FPS, equal to display FPS, and below display FPS.
4. Keep latency as the primary metric; fewer drops are not better if latency rises.

**Expected impact:** Better high-FPS latency and stutter behavior.

**Risk:** Medium. Pacer changes can subtly affect all renderers and platforms.

**Thoughts:** As noted in section 7 above, the `TIMER_SLACK_MS` of 3 ms is subtracted from the wait, which makes its impact at high refresh worse than a casual reading suggests. Deriving slack from refresh interval is sensible, but a simpler first step is just to lower the constant on platforms where the OS timer can reliably wake within ~1 ms (modern Windows with `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`, Linux/macOS with normal nanosleep). Also remember the `MAX_QUEUED_FRAMES` ↔ `PACER_MAX_OUTSTANDING_FRAMES` ↔ `extra_hw_frames` coupling — any queue change must be a coordinated edit.

**Re-review:** Mostly valid, but the implementation suggestion needs more caution. Lowering `TIMER_SLACK_MS` globally could improve CPU usage at high refresh but might increase missed wakeups or stutter on systems with coarse timers. A safer Phase 3 shape is to make slack derived from the target frame interval with min/max bounds, then compare 60/120/240 FPS latency, drops, and CPU usage.

### 7. Revisit software decode slice/thread policy only after measurement

**Why it matters:** Software decode is intentionally limited to four slices/threads (`MAX_SLICES = 4`, `app\streaming\video\decoder.h:7-10`; `app\streaming\video\ffmpeg.cpp:101-107`, `app\streaming\video\ffmpeg.cpp:529-537`). This avoids excessive latency and aligns with host encoder slice support. Modern CPUs may benefit from different policies at 4K/high FPS, but more threads can increase latency and overhead.

**Possible approach:**

1. Measure H.264, HEVC, and AV1 software decode at common resolutions/FPS.
2. Test whether increasing slice count helps or hurts latency.
3. Consider an advanced/automatic policy based on codec, resolution, CPU count, and measured decode time.

**Expected impact:** Useful only for software decode users or unsupported hardware paths.

**Risk:** Medium. More CPU parallelism can increase frame latency and reduce stream smoothness.

**Thoughts:** Correctly deprioritized. Software decode is already a fallback path; Moonlight's value proposition is hardware decode. Spending engineering time tuning slice counts here helps a small minority of users at the cost of latency complexity. Skip unless Phase 1 diagnostics surface a real population on this path.

**Re-review:** Valid. Keep the software-decode policy stable for now. The only near-term change I would accept here is better diagnostics that clearly say software decode is active and show the decoder name; tuning thread counts should stay out of scope.

### 8. Add slow-path user warnings

**Why it matters:** Some settings look like quality upgrades but can force slow paths. For example, YUV 4:4:4 and 10-bit formats may fall back to CPU conversion under SDL (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:96-113`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:524-560`). The UI marks AV1, HDR, and YUV 4:4:4 as experimental, but it does not explain decode/render cost in detail (`app\gui\SettingsView.qml:1530-1675`).

**Possible approach:**

1. After decoder probing, detect if selected settings result in software decode or CPU conversion.
2. Show a configuration warning before stream launch or in settings.
3. Prefer actionable wording: "This setting may increase CPU usage because your current renderer cannot display this format directly."

**Expected impact:** Fewer accidental slow configurations and easier support/debugging.

**Risk:** Low to medium. Avoid warning fatigue and avoid false warnings on capable hardware.

**Thoughts:** This is the highest user-visible payoff after diagnostics. The trick is to scope the warning narrowly: not "you enabled YUV 4:4:4" but "your current renderer cannot display YUV 4:4:4 directly and is converting on the CPU." That's only knowable after Phase 1 lands, which is why the phasing in the implementation plan is correct. Wording-wise, prefer a single in-settings inline note tied to the offending toggle rather than a modal — modal warnings get dismissed and ignored.

**Re-review:** Agreed. The warning should be path-specific and actionable, not a generic warning about advanced codecs. One extra constraint: settings UI may not know the final runtime renderer until probing, so the first version may need to show guidance in logs/overlay or stream-start status before it can be perfectly represented in static settings.

## Measurement plan

Before changing code, collect logs and overlay stats for representative cases:

1. 1080p60 H.264, HEVC, and AV1 where supported.
2. 4K60 HEVC and AV1 where supported.
3. 1080p120/144/240 if host/display support it.
4. HDR on/off.
5. YUV 4:4:4 on/off.
6. V-sync on/off.
7. Frame pacing on/off.
8. Automatic, force hardware, and force software decoder modes.

Record:

1. Selected codec.
2. Selected FFmpeg decoder name.
3. Selected renderer/frontend/backend.
4. Hardware vs software decode.
5. Zero-copy vs copyback.
6. D3D11VA bind vs copy.
7. CPU color conversion active/inactive.
8. Average decode time.
9. Average frame queue delay.
10. Average render time.
11. Pacer drops and network drops.
12. CPU/GPU usage if available from external tools.

Existing stats already cover many timing values. The main missing piece is path classification: the overlay/logs should say exactly which decode/render path is active.

**Thoughts:** The matrix is appropriately broad but probably too large to run in full for every change. A practical reduction for Phase 1 validation: 1080p60 H.264 (the universal baseline), 4K60 HEVC HDR (the heavy hardware path), and 1080p120 H.264 (the high-FPS path). Those three exercise hardware decode, HDR metadata, and high-refresh pacing, and they will catch most regressions without a full sweep. The full matrix can be reserved for Phase 5 final validation.

**Re-review:** Sensible reduction, but it should not replace at least one slow-path validation case. Because Phase 1 is specifically about path diagnostics, the reduced matrix should include one forced software decode or SDL conversion case to prove the new labels catch the failure modes the study cares about.

## Ideas to avoid for now

1. **Do not force a single "fastest" codec globally.** Codec choice depends on hardware support, HDR, 4:4:4, host behavior, driver support, and latency.
2. **Do not blindly increase software decode threads.** More threads can improve throughput but increase latency.
3. **Do not remove frame dropping from Pacer.** Dropping frames is sometimes the correct low-latency behavior.
4. **Do not replace D3D11VA heuristics without GPU-specific measurements.** The current code contains vendor and driver workarounds that likely came from real bugs.
5. **Do not assume Vulkan/libplacebo is always faster.** It can be best on some platforms and worse on others, and the code already avoids some known bad Vulkan paths.

**Thoughts:** All five anti-recommendations are well-aimed and worth keeping. Item 4 in particular ("Do not replace D3D11VA heuristics without GPU-specific measurements") aligns with my note on opportunity #5: those heuristics encode driver-bug knowledge, not architectural decisions, and treating them as architecture is how regressions are introduced.

**Re-review:** Agreed. These anti-recommendations are useful because they define boundaries for the implementation phases. They should remain in the study even after code work starts, because they explain why the plan avoids obvious-but-risky changes like codec preference rewrites or blanket D3D11VA heuristic changes.

## Proposed next step

The safest next engineering step is a diagnostics-only change: expose the active decode/render path in logs and optionally the performance overlay. That would make later optimization work measurable and lower risk.

After diagnostics are available, the first real optimization to prototype should be the FFmpeg decoder-loop `EAGAIN` handling, because it has direct source evidence, an existing FIXME, and a plausible latency impact at high FPS.

**Thoughts:** Agreed. This matches the implementation plan: Phase 1 diagnostics first, Phase 2 decoder loop next, Phase 3 pacer tuning, Phase 4 user-facing slow-path guidance, Phase 5 validation. Items 4, 5, and 7 should stay deferred unless Phase 1 telemetry surfaces a concrete need. The biggest risk to this plan is scope creep into D3D11VA heuristics or AVFrame pooling — both should be explicitly out of scope for the initial implementation.

**Re-review:** Confirmed. The next step remains correct after the second pass. I would adjust the implementation boundaries slightly: Phase 2 should include the `m_DecodeBuffer.resize()` safety fix because it is tightly coupled to packet submission, but AVFrame pooling and D3D11VA heuristic changes should stay excluded unless profiling provides a clear reason.
