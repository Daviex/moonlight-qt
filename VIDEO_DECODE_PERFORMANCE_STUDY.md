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

## Plain-language model of the pipeline

Video streaming has three separate pieces that can each affect performance:

1. **Decode** - compressed H.264, HEVC, or AV1 data is turned into raw video frames. This is fast when done by the GPU/decoder block and much slower when done in software on the CPU.
2. **Render** - decoded frames are drawn to the window. The best case is zero-copy GPU rendering. Slower paths copy frames between GPU resources, read frames back to CPU memory, or convert colors on the CPU before uploading to a texture.
3. **Pacing** - frames are released to the display at the right time. Good pacing reduces stutter, but too much queueing increases latency.

The code already tracks these pieces in `VIDEO_STATS`: received/decoded/rendered frames, network drops, pacer drops, decode time, pacer queue time, render time, RTT, and FPS values (`app\streaming\video\decoder.h:11-34`). The debug overlay formats those values in `FFmpegVideoDecoder::stringifyVideoStats()` (`app\streaming\video\ffmpeg.cpp:805-977`).

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

### 2. FFmpeg uses a pull-based decoder thread

`FFmpegVideoDecoder::getDecoderCapabilities()` always adds `CAPABILITY_PULL_RENDERER` (`app\streaming\video\ffmpeg.cpp:88-155`). Limelight documents this mode as a renderer-managed decode/render thread model where the client calls `LiWaitForNextVideoFrame()`, `LiPollNextVideoFrame()`, and `LiCompleteVideoFrame()` (`moonlight-common-c\moonlight-common-c\src\Limelight.h:950-961`).

Because this capability is set, `Session::populateDecoderProperties()` does not provide a push callback to Limelight (`app\streaming\session.cpp:508-553`). Instead, `moonlight-common-c` queues decode units in `VideoDepacketizer.c` (`moonlight-common-c\moonlight-common-c\src\VideoDepacketizer.c:512-539`) and Moonlight Qt pulls them from `FFmpegVideoDecoder::decoderThreadProc()` (`app\streaming\video\ffmpeg.cpp:1810-1985`).

This design is good for latency isolation: network packet handling is not forced to do decode/render work directly.

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

### 4. FFmpeg context is configured for low latency

`completeInitialization()` sets FFmpeg decoder flags for low delay, output of corrupt/missing-reference frames, and error recognition so Moonlight can request a key frame on decode failure (`app\streaming\video\ffmpeg.cpp:513-527`).

Software decode uses slice threading with up to four slices/threads (`app\streaming\video\ffmpeg.cpp:529-537`). The capabilities code also asks the host encoder for up to four slices per frame when software decoding is active (`app\streaming\video\ffmpeg.cpp:101-107`). Hardware decode uses one FFmpeg thread and relies on the GPU decoder.

The context also sets `extra_hw_frames = PACER_MAX_OUTSTANDING_FRAMES` to avoid starving hardware decoder surface pools while frames are queued for pacing/rendering (`app\streaming\video\ffmpeg.cpp:546-548`).

### 5. Incoming frames are copied into a contiguous packet buffer

`FFmpegVideoDecoder::submitDecodeUnit()` receives a decode unit from Limelight. The decode unit contains a chain of buffers, not one contiguous allocation. The code computes the required size, reserves `m_DecodeBuffer`, copies each entry through `writeBuffer()`, sets up one `AVPacket`, and calls `avcodec_send_packet()` (`app\streaming\video\ffmpeg.cpp:1987-2108`).

This copy is probably necessary because FFmpeg packet input is contiguous and the Limelight decode-unit buffers are only valid during the callback/pull completion window. Still, this path is worth auditing because it happens for every frame.

### 6. Decoded frames are handed to Pacer

`decoderThreadProc()` waits for queued input, sends it to FFmpeg, receives decoded frames, attaches HDR metadata if needed, applies small cropping fixes for encoder padding, records decode timing, and calls `m_Pacer->submitFrame(frame)` (`app\streaming\video\ffmpeg.cpp:1810-1985`).

When `avcodec_receive_frame()` returns `EAGAIN`, it polls Limelight for another frame and submits that if available. If no input or output is available, it sleeps for 2 ms (`app\streaming\video\ffmpeg.cpp:1935-1948`). There is an existing FIXME for proper `EAGAIN` handling on `avcodec_send_packet()` (`app\streaming\video\ffmpeg.cpp:1941-1943`).

### 7. Pacer controls queue depth, frame drops, and render timing

`Pacer` has two queues: one for pacing and one for rendering. It caps queued frames and drops old frames when queues grow too deep (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:16-23`, `app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:394-419`).

If frame pacing is enabled and a platform V-sync source exists, Pacer creates a V-sync thread. On Windows it can use `DxVsyncSource`; on Wayland it can use `WaylandVsyncSource` (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:262-325`). Rendering uses a dedicated render thread when supported (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:137-177`). Otherwise, it posts an SDL user event and renders on the main thread (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:179-197`).

This is already a good low-latency design, but the queue thresholds and timer slack should be measured at high refresh rates.

### 8. Windows D3D11VA is already optimized, but has several mode-dependent paths

D3D11VA tries to use the GPU attached to the display first to avoid inter-GPU copies, then falls back to other GPUs if necessary (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:507-530`).

It decides whether to use separate decode/render devices based on extended resource sharing and fence support (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:361-398`). It then decides whether to bind decoder output textures directly or copy into a render texture. The current heuristic binds output textures on Intel GPUs or when using separate devices; otherwise it uses copy mode (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:400-421`).

The direct-bind path indexes into shader resource views based on FFmpeg's texture index. The copy path calls `CopySubresourceRegion1()` every rendered frame (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:1017-1039`).

D3D11VA presents with `Present(0, flags)` and uses tearing flags when V-sync is disabled and the OS/GPU supports it (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:573-606`, `app\streaming\video\ffmpeg-renderers\d3d11va.cpp:766-809`). Swapchain buffer count is deliberately high enough to avoid starvation, while comments explain why maximum frame latency is not forced to one (`app\streaming\video\ffmpeg-renderers\d3d11va.cpp:542-558`).

### 9. SDL fallback can become much slower

SDL rendering is a fallback and compatibility path. For 8-bit 4:2:0 formats, SDL can upload YUV/NV12/NV21 and let the renderer backend do conversion (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:96-131`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:456-523`).

For 10-bit or YUV 4:4:4 formats, SDL cannot render those formats natively, so the code uses swscale to convert to RGB on the CPU, then uploads the converted texture (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:98-113`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:524-560`). The code already enables up to four swscale threads on newer FFmpeg/libswscale (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:343-390`).

If SDL is used as a frontend for hardware decoded frames, frames may need to be mapped or copied from GPU memory to CPU memory first (`app\streaming\video\ffmpeg-renderers\swframemapper.cpp:16-100`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:274-292`). This is a major slow path and should be visible to the user or debug overlay.

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

### 4. Investigate AVFrame allocation churn

**Why it matters:** `decoderThreadProc()` allocates an `AVFrame` for output attempts and hands successful frames to `Pacer`, which frees them after rendering (`app\streaming\video\ffmpeg.cpp:1830-1934`, `app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:332-350`). This is straightforward and safe, but it allocates continuously during playback.

**Possible approach:**

1. Measure allocation overhead first. Hardware decode/render time may dwarf this cost.
2. If measurable, consider a small frame wrapper/pool where frames are `av_frame_unref()` and reused after Pacer/rendering releases them.
3. Preserve ownership boundaries: once a frame is queued to Pacer, the decoder thread must not reuse it.

**Expected impact:** Probably small for GPU-heavy paths, possibly useful on CPU-constrained systems.

**Risk:** Medium. Frame lifetime is shared across decode, Pacer, and renderer. A pool would need very clear ownership.

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

### 6. Tune Pacer for high refresh rates

**Why it matters:** Queue thresholds and timer slack are fixed constants. `Pacer` uses `MAX_QUEUED_FRAMES = 3` and `TIMER_SLACK_MS = 3` (`app\streaming\video\ffmpeg-renderers\pacer\pacer.cpp:16-30`). A 3 ms slack is reasonable at 60 FPS but becomes a large fraction of the frame interval at 240 FPS.

**Possible approach:**

1. Measure frame queue delay and pacer drops at 60, 120, 144, and 240 FPS.
2. Consider deriving timer slack from refresh interval instead of a fixed 3 ms.
3. Consider separate queue policy for stream FPS greater than display FPS, equal to display FPS, and below display FPS.
4. Keep latency as the primary metric; fewer drops are not better if latency rises.

**Expected impact:** Better high-FPS latency and stutter behavior.

**Risk:** Medium. Pacer changes can subtly affect all renderers and platforms.

### 7. Revisit software decode slice/thread policy only after measurement

**Why it matters:** Software decode is intentionally limited to four slices/threads (`MAX_SLICES = 4`, `app\streaming\video\decoder.h:7-10`; `app\streaming\video\ffmpeg.cpp:101-107`, `app\streaming\video\ffmpeg.cpp:529-537`). This avoids excessive latency and aligns with host encoder slice support. Modern CPUs may benefit from different policies at 4K/high FPS, but more threads can increase latency and overhead.

**Possible approach:**

1. Measure H.264, HEVC, and AV1 software decode at common resolutions/FPS.
2. Test whether increasing slice count helps or hurts latency.
3. Consider an advanced/automatic policy based on codec, resolution, CPU count, and measured decode time.

**Expected impact:** Useful only for software decode users or unsupported hardware paths.

**Risk:** Medium. More CPU parallelism can increase frame latency and reduce stream smoothness.

### 8. Add slow-path user warnings

**Why it matters:** Some settings look like quality upgrades but can force slow paths. For example, YUV 4:4:4 and 10-bit formats may fall back to CPU conversion under SDL (`app\streaming\video\ffmpeg-renderers\sdlvid.cpp:96-113`, `app\streaming\video\ffmpeg-renderers\sdlvid.cpp:524-560`). The UI marks AV1, HDR, and YUV 4:4:4 as experimental, but it does not explain decode/render cost in detail (`app\gui\SettingsView.qml:1530-1675`).

**Possible approach:**

1. After decoder probing, detect if selected settings result in software decode or CPU conversion.
2. Show a configuration warning before stream launch or in settings.
3. Prefer actionable wording: "This setting may increase CPU usage because your current renderer cannot display this format directly."

**Expected impact:** Fewer accidental slow configurations and easier support/debugging.

**Risk:** Low to medium. Avoid warning fatigue and avoid false warnings on capable hardware.

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

## Ideas to avoid for now

1. **Do not force a single "fastest" codec globally.** Codec choice depends on hardware support, HDR, 4:4:4, host behavior, driver support, and latency.
2. **Do not blindly increase software decode threads.** More threads can improve throughput but increase latency.
3. **Do not remove frame dropping from Pacer.** Dropping frames is sometimes the correct low-latency behavior.
4. **Do not replace D3D11VA heuristics without GPU-specific measurements.** The current code contains vendor and driver workarounds that likely came from real bugs.
5. **Do not assume Vulkan/libplacebo is always faster.** It can be best on some platforms and worse on others, and the code already avoids some known bad Vulkan paths.

## Proposed next step

The safest next engineering step is a diagnostics-only change: expose the active decode/render path in logs and optionally the performance overlay. That would make later optimization work measurable and lower risk.

After diagnostics are available, the first real optimization to prototype should be the FFmpeg decoder-loop `EAGAIN` handling, because it has direct source evidence, an existing FIXME, and a plausible latency impact at high FPS.
