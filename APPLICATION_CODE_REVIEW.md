# Application code review

This is a static review of the Moonlight Qt application code under `app`, including backend, settings, CLI, GUI/QML, streaming, platform glue, startup, and resource integration. The findings below are potential problems observed from source inspection; some should be confirmed with targeted runtime tests before implementation.

## Summary

| Severity | Count | Main areas |
| --- | ---: | --- |
| High | 3 | Path safety, controller event handling, out-of-bounds access |
| Medium | 8 | Threading/lifetime, model validation, QML creation, persistence validation |
| Low | 3 | File I/O diagnostics, randomness error handling, signal lifecycle robustness |

## Findings

### 1. Host-provided UUIDs are used as cache directory names without validation

**Severity:** High  
**Files:** `app\backend\nvcomputer.cpp:140`, `app\backend\boxartmanager.cpp:28-36`, `app\backend\boxartmanager.cpp:90-97`

`NvComputer` assigns `uuid` directly from the host's `uniqueid` XML field. `BoxArtManager` later uses `computer->uuid` as a directory name and calls `dir.cd(computer->uuid)` and `dir.removeRecursively()` for deletion. A malicious or malformed host could provide a UUID containing path separators or special components such as `..`, causing cache writes or recursive deletes outside the intended per-host box-art directory.

**Possible solution:** Treat host IDs as data, not paths. Validate `uniqueid` against the expected UUID/hex format before storing it, or derive cache directory names from a safe encoding/hash of the UUID. In `BoxArtManager`, reject names containing path separators and verify the final canonical path remains under `Path::getBoxArtCacheDir()` before writing or recursively deleting.

**Thoughts:** Confirmed valid. `nvcomputer.cpp:140` assigns `uuid` directly from `getXmlString(serverInfo, "uniqueid")` with no validation, and `boxartmanager.cpp` calls `dir.cd(computer->uuid)`, `dir.mkdir(computer->uuid)`, and `dir.removeRecursively()` with that string. The attack surface is somewhat limited (the user must already trust the host enough to pair, and hosts under attacker control are rare), but the failure mode of recursive deletion outside the cache directory is severe enough that I would still rank this High. A minimal mitigation is a regex validation `^[0-9a-fA-F-]{1,64}$` at the point of ingestion in `NvComputer::NvComputer(NvHTTP&, QString)`; the canonical-path check in `BoxArtManager` is good defense-in-depth.

**Comment:** I agree with this assessment. The important nuance is that the host-provided `uniqueid` is trusted not only for writes but also for `removeRecursively()`, so validating only at delete time would be incomplete. The best solution is to sanitize once at ingestion and still keep a canonical-path guard in `BoxArtManager` because older persisted host entries may already contain unsafe values.

**Re-review:** The Comment's point about pre-existing persisted entries is the most useful net-new observation across all three layers and justifies keeping the canonical-path guard in `BoxArtManager` even after ingestion validation lands. No correction to the Thoughts or Comment.

### 2. Adaptive trigger event handling can crash or leak memory

**Severity:** High  
**Files:** `app\streaming\session.cpp:254-274`, `app\streaming\input\gamepad.cpp:903-916`

`Session::clSetAdaptiveTriggers()` allocates a `DualSenseOutputReport` with `SDL_malloc()` and immediately dereferences it with `SDL_zero(*state)` without checking for allocation failure. The pointer is handed to SDL via `SDL_PushEvent()`, but the push return value is ignored; if the event queue rejects the event, the allocated report is leaked. The consumer frees the report in `SdlInputHandler::setAdaptiveTriggers()`, but only if the event is delivered.

There is also an off-by-one range check in `SdlInputHandler::setAdaptiveTriggers()`:

```cpp
if (controllerNumber <= MAX_GAMEPADS &&
    m_GamepadState[controllerNumber].controller != nullptr &&
    ...)
```

The valid index range is `0..MAX_GAMEPADS - 1`; `controllerNumber == MAX_GAMEPADS` accesses one element past the array.

**Possible solution:** Check the allocation before dereferencing, check `SDL_PushEvent()` and free `state` on failure, and change the controller range check to `controllerNumber < MAX_GAMEPADS`. Consider wrapping event payload ownership in a small helper so every early-return path frees the allocation.

**Thoughts:** Confirmed valid. The off-by-one is the most clear-cut bug in the entire review: every other range check in `gamepad.cpp` (lines 794, 852, 866, 892) uses `>= MAX_GAMEPADS` while line 907 uses `<= MAX_GAMEPADS`, so this is almost certainly a typo. With `MAX_GAMEPADS == 16` and `m_GamepadState[16]` access, this writes one element past the array, but in practice `controllerNumber` is bounded by Sunshine/GFE which historically capped at 4, so it's unlikely to be exploited in normal flows — still a correctness fix worth doing. The `SDL_malloc` and `SDL_PushEvent` failures are theoretical (both are extremely unlikely under normal conditions), so I'd rank the leak/crash portion Medium and the off-by-one Medium-High, not as severe as the report suggests.

**Comment:** This thought is valid and properly separates the definite bug from the less likely allocation/event-queue cases. One addition: `SDL_PushEvent()` can fail for reasons other than memory pressure (for example if events are disabled or the subsystem is shutting down), so freeing the event payload on push failure should still be part of the fix even if that path is uncommon.

**Re-review:** Confirmed by direct inspection of `gamepad.cpp`: lines 794, 852, 866, 892 all use `>= MAX_GAMEPADS`, and only line 907 uses `<= MAX_GAMEPADS`, making the typo unambiguous. Thoughts and Comment are both accurate; the Comment's note about non-memory `SDL_PushEvent` failure modes is correct and worth preserving in the fix.

### 3. QML-facing models rely on debug-only assertions for indexes

**Severity:** High  
**Files:** `app\gui\appmodel.cpp:16-21`, `app\gui\appmodel.cpp:42-47`, `app\gui\appmodel.cpp:71-78`, `app\gui\computermodel.cpp:19-28`, `app\gui\computermodel.cpp:117-134`, `app\gui\computermodel.cpp:137-147`

Several QML-invokable model methods validate indexes only with `Q_ASSERT()` and then access `QVector` entries with `.at()` or `[]`. In release builds, invalid QML indexes can lead to out-of-range access rather than a controlled error. Examples include `AppModel::initialize()`, `AppModel::createSessionForApp()`, `AppModel::data()`, `ComputerModel::data()`, `ComputerModel::createSessionForCurrentGame()`, and `ComputerModel::deleteComputer()`.

**Possible solution:** Replace public/QML-facing assertion-only checks with runtime validation that logs a warning and returns a safe value (`nullptr`, `QVariant()`, or no-op) when indexes are invalid. Keep assertions if useful, but do not rely on them for input that can originate from QML state.

**Thoughts:** Confirmed valid in principle, but the practical risk is lower than High. In normal QML flows the indexes come from `QAbstractListModel` itself or from menu-item `index` properties bound to model rows, so they are almost always valid. The realistic crash scenarios are: (a) a stale `computerIndex` after `deleteHost()` while a context menu is still open, and (b) `currentItem` being null when an empty model returns `directLaunchAppIndex >= 0` (which can't happen given current logic, but is fragile). I would rate this Medium and prioritize the methods that mutate state (`deleteComputer`, `setAppHidden`, `createSessionForApp`) over read-only ones.

**Comment:** I agree that High is overstated. The finding remains worth addressing because these are QML-invokable public methods, not private helper calls, and release builds remove the only guard. The solution should prefer safe no-op/null returns plus `qWarning()` over replacing every `Q_ASSERT`, so debug builds still catch internal logic errors.

**Re-review:** Comment is correct and is the right shape for the fix: keep `Q_ASSERT` for invariants the application controls and add release-build guards for any index that originates from QML. No further correction.

### 4. `Session::s_ActiveSession` is globally shared across callbacks without synchronized reads

**Severity:** Medium  
**Files:** `app\streaming\session.cpp:66-85`, `app\streaming\session.cpp:88-100`

Limelight callbacks such as `clStageStarting()`, `clStageFailed()`, and `clConnectionTerminated()` dereference `Session::s_ActiveSession` directly. The class has `s_ActiveSessionSemaphore`, but these callback reads are not protected by it. If callbacks overlap teardown or unexpected termination paths, the global pointer can become stale or null while still being used.

**Possible solution:** Centralize active-session access behind a helper that returns a guarded local pointer or uses a lock/semaphore consistently for all reads and writes. During teardown, clear callback entry points or mark the session as shutting down before releasing resources.

**Thoughts:** Partially valid but overstated. Limelight invokes these callbacks only between `LiStartConnection()` and `LiStopConnection()`, and `s_ActiveSession` is set before `LiStartConnection()` (line 1732) and cleared only after `LiStopConnection()` returns inside `DeferredSessionCleanupTask` (line 1241). So during the window callbacks are issued, `s_ActiveSession` is guaranteed valid by the streaming protocol contract — the semaphore exists to serialize *session creation*, not to protect callbacks. The real, smaller risk is that some Limelight callbacks (e.g., audio decode on its own thread) might fire one last time while teardown is in progress on the main thread; documenting that contract or adding an atomic "shutting down" flag is more proportionate than introducing a callback lock that could deadlock the streaming hot path.

**Comment:** This correction is valid. I would not implement a semaphore or mutex around every callback read because it could introduce deadlocks or latency in hot paths. If this is changed, the safer direction is to minimize the static global surface over time and add explicit shutdown-state checks only where callbacks emit user-visible errors or touch objects that may already be unwinding.

**Re-review:** Confirmed against source: `s_ActiveSession = this` at `session.cpp:1732` is set before `LiStartConnection()`, and `Session::s_ActiveSession = nullptr` at `session.cpp:1241` runs inside `DeferredSessionCleanupTask` after `LiStopConnection()`. The streaming-protocol contract guarantees the pointer is valid during the window callbacks fire. Thoughts and Comment are both accurate; original finding's High framing was overstated.

### 5. `Session` does not validate `SDL_CreateMutex()`

**Severity:** Medium  
**File:** `app\streaming\session.cpp:550-558`

`m_DecoderLock` is initialized with `SDL_CreateMutex()` in the constructor initializer list and is used later by decoder callbacks and teardown. SDL can return `nullptr` on allocation failure, which would turn later lock/unlock calls into crashes.

**Possible solution:** Move mutex creation into an initialization path that can fail cleanly, or immediately validate `m_DecoderLock` in the constructor and surface a startup error before the session can be used. Ensure teardown tolerates a null mutex if construction fails partway.

**Thoughts:** Confirmed but Low practical risk. `SDL_CreateMutex()` only fails under extreme memory pressure where the application is already in trouble. Still worth fixing because it's cheap and `m_DecoderLock` is touched on the video render hot path; a null deref there would be a hard crash. A `Q_ASSERT` plus a log + early bailout in `Session::initialize()` would be sufficient — no need for full graceful degradation.

**Comment:** I agree. The original Medium severity is too high if considered in isolation, but the fix is simple enough that it can be bundled with other session-initialization hardening. The runtime path should still avoid using only `Q_ASSERT`, since the failure matters only in release builds.

**Re-review:** Comment is correct. Bundling with other initialization hardening avoids churn on a low-likelihood failure. No further correction.

### 6. DRM master hook error paths can return untracked or invalid file descriptors

**Severity:** Medium  
**File:** `app\masterhook_internal.c:177-231`

When the SDL FD table is full, the code logs `"No unused SDL FD table entries!"` and returns the just-opened DRM FD with a "Hope for the best" comment. That FD is not inserted into the tracking table, so later close/master handling may not match the intended Qt/SDL DRM master arbitration. In the path that duplicates an existing tracked FD, `dup()` is assigned back to `fd` without checking for `-1` before the table insertion logic continues.

**Possible solution:** Treat FD table exhaustion and `dup()` failure as hard failures for the hook path. Close any newly opened FD that will not be tracked, return `-1` with `errno` preserved where appropriate, and only increment `g_SdlDrmMasterFdCount` after a valid FD is stored.

**Thoughts:** Confirmed valid. The `// Hope for the best` comment on line 185 is a self-acknowledged hack. In practice the table holds 8 entries (which is far more than SDL realistically opens), so exhaustion is unlikely. The `dup()` failure path is the more concrete bug: `fd = dup(...)` followed by `if (fd >= 0) { drmSetMaster(fd); g_SdlDrmMasterFds[freeFdIndex] = fd; }` actually does guard the assignment (line 224), so the table won't store -1, but the count and the original opened FD on the success-after-dup path are still leaked because `real_close(fd)` was already called on the freshly opened FD before the failed dup. Medium severity is right. This only affects the embedded/Steam Link build, not desktop, so it's narrow.

**Comment:** The overall conclusion is valid, but the wording about the `dup()` path should be interpreted carefully. The code guards against storing `-1`, so the problem is not an invalid descriptor in the table; it is inconsistent tracking and cleanup when the hook cannot create a replacement descriptor. The possible solution remains correct: fail closed rather than returning an FD that the hook cannot manage.

**Re-review:** Comment correctly disambiguates "invalid FD in table" from "untracked but valid FD," which the original finding conflated. This refinement is the most important correction in the chain — the actual bug is tracking/cleanup, not descriptor validity.

### 7. Delayed host flushing snapshots the host map without the host-map lock

**Severity:** Medium  
**File:** `app\backend\computermanager.cpp:249-310`

`DelayedFlushThread::run()` clears and rebuilds `m_LastSerializedHosts` while holding only `m_DelayedFlushMutex`, then iterates `m_KnownHosts` at lines 270-275. Other code mutates `m_KnownHosts` under `m_Lock`, not `m_DelayedFlushMutex` (`DeferredHostDeletionTask` removes entries at lines 514-519, and `PendingAddTask` inserts entries at lines 916-962). This creates a potential data race or iterator invalidation while the delayed flush thread snapshots host state.

**Possible solution:** Hold `m_Lock` while iterating `m_KnownHosts` for the snapshot, using the same lock order everywhere. If lock ordering becomes difficult, copy the host pointers under `m_Lock`, then copy individual host state under each `NvComputer::lock` after releasing the map lock.

**Thoughts:** Confirmed valid and the most subtle real concurrency bug in the review. The header file at `computermanager.h:283` explicitly documents `m_DelayedFlushMutex` as having a lock-ordering rule against `NvComputer::lock`, but says nothing about `m_Lock`, and the snapshot pass in `DelayedFlushThread::run()` lines 270-275 indeed iterates the QHash without holding `m_Lock`. A concurrent insert from `PendingAddTask::run()` or removal from `DeferredHostDeletionTask::run()` would be undefined behavior on QHash. The likely reason this hasn't been seen as crashes is timing — host adds/removes are rare events triggered by user action, while the flush thread idles most of the time — but the race is real. The proposed fix (acquire `m_Lock` before the loop) is straightforward; lock-ordering compatibility should be checked because the flush pass already takes `m_Lock` after releasing `m_DelayedFlushMutex`.

**Comment:** I agree fully. This is the strongest Medium finding because it involves a Qt container being iterated while other paths mutate it under a different lock. The safest implementation is to take a short-lived snapshot of pointers or value copies under `m_Lock`, then release the map lock before any slower serialization or per-host locking.

**Re-review:** Confirmed: `computermanager.h:283` documents lock ordering between `m_DelayedFlushMutex` and `NvComputer::lock` but is silent on `m_Lock`, exactly matching the Thoughts' claim. This remains the strongest concurrency finding in the document and the snapshot approach is the right fix.

### 8. Box-art load tasks retain raw host pointers across asynchronous work

**Severity:** Medium  
**Files:** `app\backend\boxartmanager.cpp:39-70`, `app\backend\boxartmanager.cpp:72-84`, `app\backend\computermanager.cpp:500-544`

`NetworkBoxArtLoadTask` stores raw `BoxArtManager*` and `NvComputer*` pointers, then performs network and disk work on a thread-pool thread. Host deletion is also asynchronous and eventually deletes the same `NvComputer` pointer. If a user deletes a host while box art is being fetched, the task can read a freed `NvComputer` through `NvHTTP http(computer)` or when computing the cache path.

**Possible solution:** Avoid passing `NvComputer*` into long-lived background tasks. Snapshot the immutable data needed for the request and cache path (`NvAddress`, HTTPS port, pinned certificate, sanitized UUID, app ID) before starting the task, or use a shared ownership/lifetime token that lets the task detect cancellation before dereferencing.

**Thoughts:** Confirmed valid. The interaction with `DeferredHostDeletionTask` is the concrete UAF risk: the deletion task does `delete m_Computer` after polling stops, but it does not wait for the box-art thread pool. Box-art tasks run on `m_ThreadPool` (max 4 threads) inside `BoxArtManager` which is owned by `AppModel`, while host deletion goes through `QThreadPool::globalInstance()`, so there is no synchronization between them. The window is small (deletion typically happens after the user leaves AppView and tears down the AppModel) but the lifetime contract is not enforced anywhere. The proposed fix — capture `NvAddress`, port, and cert by value at task creation — is the cleanest approach.

**Comment:** This thought is valid. Capturing immutable request/cache data by value is better than trying to extend `NvComputer` lifetime with shared ownership, because the task does not need live host state after it starts. The fix should include the sanitized UUID/cache path too; otherwise it still depends on the host object for filesystem naming.

**Re-review:** Comment correctly couples this fix to finding #1 (UUID sanitization). Implementing both together is more efficient than two passes, and the value-capture approach naturally makes path safety a constructor-time concern.

### 9. `AppModel` reads mutable `NvComputer` state without taking the per-computer lock

**Severity:** Medium  
**Files:** `app\gui\appmodel.cpp:16-21`, `app\gui\appmodel.cpp:71-87`, `app\gui\appmodel.cpp:148-206`

`ComputerModel::data()` correctly uses `QReadLocker lock(&computer->lock)`, but `AppModel` reads `m_Computer->currentGameId` and `m_Computer->appList` without taking `NvComputer::lock`. `ComputerManager` and polling threads update these fields under the per-computer lock, so `AppModel` can observe inconsistent state or race with updates.

**Possible solution:** Take `QReadLocker` whenever `AppModel` reads `NvComputer` fields. Prefer copying the app list and current game ID under lock, then updating the Qt model after releasing the lock to avoid holding locks while emitting model signals.

**Thoughts:** Confirmed valid. The contrast with `ComputerModel::data()` (which correctly does `QReadLocker lock(&computer->lock)` at line 28) makes this look like a missed pattern rather than an intentional design choice. The reads of `m_Computer->currentGameId` at line 84 and `m_Computer->appList` indirectly via `handleComputerStateChanged` are most exposed because polling threads write those fields. In practice tearing reads of an `int` and a `QVector<NvApp>` would more likely cause stale UI than crashes, but `QVector` reads concurrent with writes are still UB. Medium severity is appropriate.

**Comment:** I agree. The solution should be careful not to emit Qt model signals while holding `NvComputer::lock`, because that could create re-entrant reads from QML. Copying the needed app/current-game data under a read lock and then updating model state outside the lock is the right shape.

**Re-review:** Comment's reentrancy warning is the most important practical detail in this finding. Without it, a naive lock-then-emit fix could deadlock or cause QML to call back into `data()` while the writer still holds the lock.

### 10. Custom resolutions and persisted preferences are not range-validated consistently

**Severity:** Medium  
**Files:** `app\cli\commandlineparser.cpp:381-403`, `app\settings\streamingpreferences.cpp:124-170`

CLI parsing validates FPS, bitrate, and packet size ranges, but a custom `--resolution` only has format validation; values such as `0x0` or extremely large dimensions can be assigned directly. Persisted settings are also loaded directly from `QSettings` into dimensions, enum values, and packet/audio/video options without clamping or enum-range checks.

**Possible solution:** Add shared validation helpers for dimensions, FPS, bitrate, packet size, and enum ranges. Use them both after CLI parsing and after `StreamingPreferences::reload()`. For invalid stored settings, log once and fall back to documented defaults.

**Thoughts:** Confirmed valid but Low-Medium in real impact. Most invalid settings would just cause Moonlight to crash or refuse to start a stream, not corrupt anything persistent — and the stream stack already has its own clamping (e.g. `LiStartConnection` rejects insane resolutions). The `static_cast<EnumType>(int)` pattern on lines 154-170 of `streamingpreferences.cpp` is the more concerning piece because an out-of-range integer turns into an invalid enum that switch statements may silently mishandle. Adding a single `clampEnum<>` helper used everywhere would close that loophole cheaply.

**Comment:** This is a sound reassessment. I would prioritize enum validation over resolution limits, because out-of-range enum values can reach switch statements that were written assuming exhaustive known values. The CLI and persisted-settings paths should share the same validation so command-line overrides cannot bypass UI constraints.

**Re-review:** Comment is correct; the static_cast-from-int enum pattern is the higher-impact target. A shared `clampEnum<>` helper used by both CLI and `StreamingPreferences::reload()` is the cheapest way to prevent drift.

### 11. Dynamic QML component creation is not checked before use

**Severity:** Medium  
**Files:** `app\gui\PcView.qml:177-179`, `app\gui\PcView.qml:231-233`, `app\gui\AppView.qml:221-227`, `app\gui\AppView.qml:357-371`, `app\gui\CliStartStreamSegue.qml:15-22`, `app\gui\QuitSegue.qml:24-29`, `app\gui\StreamSegue.qml:52-57`

The UI frequently calls `Qt.createComponent()` and immediately calls `createObject()`/`stackView.push()` without checking `component.status`, `component.errorString()`, or whether `createObject()` returned a valid object. If a QML file is missing from `qml.qrc`, fails to parse, or cannot instantiate because a required property is invalid, the user gets a null push/replacement or a hard-to-diagnose runtime failure.

**Possible solution:** Add a small QML helper for component creation that checks `Component.Error`, handles `Component.Loading` if needed, checks the returned object, logs `errorString()`, and shows an error dialog or returns to the previous page instead of pushing null.

**Thoughts:** Valid but Low-priority in practice. All these QML files are compiled into the binary via `qml.qrc` and discovered at build time, so `Qt.createComponent()` failure is essentially impossible in a shipped build — failures would only occur during development if a `.qml` file is added without updating `qml.qrc`. The realistic value of this fix is developer ergonomics, not user reliability. Document the resource-list invariant in `qml.qrc` alongside the component-loading helper rather than scattering defensive checks everywhere.

**Comment:** I agree that this is not a major runtime risk. The best solution is probably a small shared helper for the few dynamic creation sites that navigate between views, not a broad defensive rewrite of every QML component load. That would preserve readability while still surfacing resource-registration mistakes during development.

**Re-review:** Comment is fine. Build-time `qml.qrc` registration makes user-facing failure essentially impossible in shipped builds, so the value is mostly developer ergonomics — appropriate for a small helper, not a sweeping change.

### 12. `StreamSegue` assumes a non-null `Session`

**Severity:** Medium  
**Files:** `app\gui\StreamSegue.qml:9-16`, `app\gui\StreamSegue.qml:111-130`, `app\gui\appmodel.cpp:42-47`, `app\gui\computermodel.cpp:117-134`

`StreamSegue` connects several signals on `session` immediately in `StackView.onActivated`. The session object is created by QML-invokable model methods that currently rely on assertions for preconditions and can return `nullptr` in at least one release path (`ComputerModel::createSessionForCurrentGame()` falls through to `return nullptr`). If a null session reaches `StreamSegue`, activation will fail at runtime.

**Possible solution:** Validate the session before pushing `StreamSegue`, and add a defensive null check in `StreamSegue.onActivated` that displays an error and returns to the previous view. Also make the model creation methods return explicit errors or null only after logging.

**Thoughts:** Confirmed valid but the `ComputerModel::createSessionForCurrentGame()` null path is genuinely unreachable in normal flow — it requires `currentGameId != 0` (asserted) plus the running game ID not being in `appList`, which would mean GFE returned an inconsistent app list. So this is mostly a defense-in-depth concern. The fix is one line in `StreamSegue.onActivated` and is worth doing for resilience but not urgent.

**Comment:** This thought is valid. The one-line QML guard is still worthwhile because it protects against future model changes and makes failures user-visible instead of becoming QML runtime errors. I would not spend time redesigning session creation solely for this finding unless the index-validation work in finding #3 is being done at the same time.

**Re-review:** Comment correctly couples this to finding #3. Doing both passes in one change minimizes touch on the model API surface.

### 13. Manual QML signal connections have fragile lifecycle cleanup

**Severity:** Low  
**Files:** `app\gui\AppView.qml:36-61`, `app\gui\QuitSegue.qml:36-55`, `app\gui\CliStartStreamSegue.qml:36-46`

Several QML views manually connect long-lived C++ or context-object signals during activation and disconnect during deactivation, while some CLI segue connections have no matching disconnect. This can be safe in common StackView flows, but it is fragile if a view is destroyed, replaced, or errors before normal deactivation. It also makes repeated activation behavior harder to audit.

**Possible solution:** Prefer declarative `Connections { target: ... }` objects with `enabled` bound to view activation state, or add `Component.onDestruction` cleanup for every manual connection. Keep connection setup idempotent to avoid duplicate handlers.

**Thoughts:** Confirmed but Low impact. The connections target either C++ context objects (which outlive the QML view) or model singletons; when the view is destroyed, Qt automatically disconnects slots associated with destroyed receivers, so the actual leak risk is small. The bigger concern is double-connection on re-activation, which would cause duplicate signal handling — but `StackView.onActivated` only fires once per push, so this is also unlikely. I'd treat this as a code-style cleanup rather than a bug.

**Comment:** I agree with the lower severity. Declarative `Connections` would make the lifecycle easier to audit, but this should not outrank concrete crashes or races. If touched, add `Component.onDestruction` disconnects only where manual connections remain.

**Re-review:** Comment is fine. Qt's automatic disconnection of destroyed receivers makes the actual leak risk small; readability gain is the main benefit.

### 14. Cache file operations silently ignore failures

**Severity:** Low  
**File:** `app\path.cpp:32-59`

`Path::writeCacheFile()` ignores `mkpath()` failure, ignores `QFile::open()` failure, and does not check whether `write()` wrote all bytes. `Path::deleteCacheFile()` ignores `remove()` failure. These are cache operations, so failure may be non-fatal, but silent failure makes troubleshooting stale compatibility/mapping data or unwritable cache directories difficult.

**Possible solution:** Log failed directory creation, open, partial write, flush, and remove operations with `qWarning()`. Return a boolean from write/delete helpers if callers need to react.

**Thoughts:** Confirmed valid but appropriately Low. These are cache files (compatibility data, gamepad mappings) that are re-fetched on next launch if missing. The real win is diagnosability — silent cache failures make user-reported issues harder to triage. A handful of `qWarning()` calls is the right scope.

**Comment:** This is accurate. I would keep the solution logging-only unless a caller actually needs to change behavior. Returning booleans everywhere may add noise; targeted warnings for failed create/open/write/remove operations are enough.

**Re-review:** Comment is fine. Logging-only keeps the API quiet while still making cache-write issues triageable from user-submitted logs.

### 15. Unique ID generation does not check `RAND_bytes()`

**Severity:** Low  
**File:** `app\backend\identitymanager.cpp:190-210`

`IdentityManager::getUniqueId()` calls `RAND_bytes()` to generate a new client ID but does not check the return value. If the random generator fails, the code still converts the uninitialized stack value to a persistent unique ID.

**Possible solution:** Check for `RAND_bytes(...) == 1`. On failure, log a fatal error or use a Qt/OpenSSL API that reports failure explicitly, then avoid persisting an invalid ID.

**Thoughts:** Confirmed but very Low. `uid` is a stack-allocated `uint64_t` which the compiler is not required to zero-initialize, so on `RAND_bytes` failure it would contain whatever was on the stack — non-cryptographic but not catastrophically predictable in practice. More importantly, this unique ID is not used as a security token: it's a Moonlight client identifier sent to GFE so multiple clients can quit each others' sessions, and `nvhttp.cpp:480` actually overrides it with a hardcoded `"0123456789ABCDEF"` string anyway. So a weak random ID has effectively no security consequence. Worth adding the check for cleanliness, not security.

**Comment:** I agree. This should be treated as correctness/cleanliness rather than a security issue. If the hardcoded `uniqueid` behavior is intentional for interoperability, the review should not imply that `IdentityManager::getUniqueId()` currently protects a sensitive protocol secret.

**Re-review:** Confirmed: `nvhttp.cpp:480` overrides `uniqueid` with the hardcoded `"0123456789ABCDEF"` for protocol-compatibility, so `IdentityManager::getUniqueId()` is not on any security-sensitive path. The Comment correctly reframes the fix as cleanliness rather than crypto.

## Suggested follow-up order

1. Fix the path-safety issue around host UUIDs and box-art directories first, because it crosses network input and recursive filesystem operations.
2. Fix the adaptive trigger allocation/range checks and add runtime bounds checks to QML-facing model methods.
3. Address threading/lifetime issues in `ComputerManager`, `AppModel`, `Session`, and box-art loading with small targeted changes and regression tests where possible.
4. Add QML component/session creation guards so UI failures surface as user-visible errors rather than null-object runtime failures.
