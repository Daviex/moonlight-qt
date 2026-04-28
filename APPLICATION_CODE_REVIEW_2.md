# Moonlight Qt — Application code review (second pass)

This is an independent second review of the application sources under `app`. It deliberately focuses on areas that the first review (`APPLICATION_CODE_REVIEW.md`) did not cover or covered only briefly: pairing/cryptography, HTTP/TLS plumbing, network update/compat/mapping fetchers, exception handling discipline, broad `catch (...)` swallows, and shutdown/teardown semantics.

Findings are numbered and have **Severity**, **Files**, problem description, and a **Possible solution** section. Severities are calibrated against real-world impact for a paired LAN/WAN streaming client where the user has explicitly trusted the host.

## Findings

### 1. OpenSSL primitive return values are not checked in `NvPairingManager`

**Severity:** Medium

**Files:**
- `app/backend/nvpairingmanager.cpp:50` (`RAND_bytes`)
- `app/backend/nvpairingmanager.cpp:64-71` (`EVP_EncryptInit`, `EVP_EncryptUpdate`)
- `app/backend/nvpairingmanager.cpp:89-96` (`EVP_DecryptInit`, `EVP_DecryptUpdate`)
- `app/backend/nvpairingmanager.cpp:165-167` (`EVP_DigestVerifyInit`, `EVP_DigestVerifyUpdate`)
- `app/backend/nvpairingmanager.cpp:182-189` (`EVP_DigestSignInit`, `EVP_DigestSignUpdate`, `EVP_DigestSignFinal` × 2)

Every call into the OpenSSL EVP and `RAND_bytes` family ignores the documented return code. `RAND_bytes()` can return `0` on entropy failure leaving the buffer with stack garbage. `EVP_*Init`/`EVP_*Update` can fail (out of memory, FIPS policy violation, unsupported algorithm). The pairing AES key, signing operations, and signature verification all proceed regardless.

The most serious concrete failure mode: `generateRandomBytes` is used to derive `aesKey` (via `saltPin`) and `randomChallenge`. If `RAND_bytes` fails, `aesKey` becomes deterministic stack garbage — pairing will still appear to succeed against an attacker who controls the response, since the derived key is what both sides use.

**Possible solution:** Wrap each EVP call in an `if (call(...) != 1) throw std::runtime_error(...)` helper, mirror the same for `RAND_bytes`. Promote `generateRandomBytes` to throw on failure rather than silently returning whatever was on the stack.

**Thoughts:** Valid. The code currently checks allocation of contexts but not operation success, which is the wrong boundary for OpenSSL error handling. I would slightly soften the attacker model: if `RAND_bytes()` fails, an attacker does not automatically know the stack bytes, but cryptographic code must treat entropy failure as fatal. The proposed helper-based solution is appropriate and should also check `EVP_CIPHER_CTX_set_padding()` and both `EVP_DigestSignFinal()` calls.

**Re-review:** Thoughts are accurate. One refinement: cryptographic code should treat *every* EVP_* return != 1 as fatal, not log-and-continue, since silently completing a failed crypto op is the worst possible outcome — the helper should `throw` rather than warn.

---

### 2. `Q_ASSERT` is used as input validation on attacker-controlled data during pairing

**Severity:** Medium

**Files:**
- `app/backend/nvpairingmanager.cpp:72` (`ciphertextLen == ciphertext.length()` after `EVP_EncryptUpdate`)
- `app/backend/nvpairingmanager.cpp:97` (`plaintextLen == plaintext.length()` after `EVP_DecryptUpdate`)
- `app/backend/nvpairingmanager.cpp:251` (`!unverifiedServerCert.isNull()` on parsed `plaincert`)
- `app/backend/nvhttp.cpp:471` (`baseUrl.port(0) != 0`)
- `app/backend/nvhttp.cpp:423` (`!m_ServerCert.isNull()` inside `handleSslErrors`)

`Q_ASSERT` is compiled out in release builds. Several of these assertions guard on data that comes from the network: the server's `plaincert` field, the encrypted/decrypted payload sizes (which are echoes of the server-supplied ciphertext length when AES-128-ECB padding is disabled), and the URL state derived from server-supplied `HttpsPort`. A misbehaving or hostile host can drive the program past these assertions in release with no error — the post-assertion code then operates on inconsistent state (e.g. an already-null `unverifiedServerCert` is still passed to `setServerCert`, then `verifySignature` is invoked against an empty cert).

**Possible solution:** Replace assertions on network-derived state with explicit error paths that return `PairState::FAILED` (and in `handleSslErrors`, refuse to ignore errors when no cert is pinned — never just return). Keep `Q_ASSERT` only for invariants that are entirely under the application's own control.

**Thoughts:** Valid with nuance. The `ciphertextLen`/`plaintextLen` assertions mostly detect internal assumptions about no-padding ECB block lengths, but the certificate parsing and HTTPS-port assertions are clearly reachable from network-provided data. The fix should not remove all assertions; it should add release-build checks before using parsed certificates, decrypted buffers, or URL state.

**Re-review:** Thoughts are accurate. The `handleSslErrors` path at `nvhttp.cpp:423` is the most critical: returning early without rejecting the connection when no cert is pinned is silent-fail security behavior, not just a missing assertion, and should be the first item addressed under this finding.

---

### 3. `throw e;` slices exception polymorphism

**Severity:** Low

**Files:**
- `app/backend/nvhttp.cpp:157`
- `app/backend/computermanager.cpp:802`

Re-throwing with `throw e;` copies the caught reference and re-throws it as the static type. If a derived exception type is caught by reference (e.g. `GfeHttpResponseException` could in theory be derived in the future), the dynamic type is lost. Today neither site has subclasses, so the bug is latent — but it pollutes patterns that get copied elsewhere.

**Possible solution:** Use the bare `throw;` to rethrow the active exception object intact. This is also slightly faster (no copy).

**Thoughts:** Valid but Low. There is no evidence of current subclass slicing, so this is a maintainability bug rather than a present runtime failure. It is still worth fixing because `throw;` is the idiomatic and safer construct, and the change is mechanical.

**Re-review:** Thoughts are correct. Mechanical change with no behavioral risk; no further analysis needed.

---

### 4. `delete reply;` instead of `deleteLater()` for `QNetworkReply` objects

**Severity:** Medium

**Files:**
- `app/backend/nvhttp.cpp:458` (success path of `openConnectionToString`)
- `app/backend/nvhttp.cpp:542, 547, 552` (error paths of `openConnection`)

`QNetworkReply` inherits `QIODevice` and is owned by the `QNetworkAccessManager`'s internal pipeline. Deleting it directly from a slot or a function reached from a `QEventLoop::exec()` can land while the NAM still has queued signal emissions or pending socket-level callbacks targeting the reply, producing use-after-free in the Qt event loop. Qt's documented disposal pattern is `reply->deleteLater()`. Crashes here are timing-dependent and tend to surface only under load (large XML responses, slow networks) or with HTTP/2 multiplexing.

**Possible solution:** Replace every `delete reply;` with `reply->deleteLater();`. For exception paths that throw, compute the exception object first and call `deleteLater()` immediately before `throw`.

**Thoughts:** Partially valid and somewhat overstated. These deletes happen in a synchronous helper after its local event loop has returned, not directly inside a `finished` slot, so direct deletion is less dangerous than the finding implies. However, the rest of the codebase already uses `deleteLater()` for asynchronous replies, and using it here would reduce dependence on subtle Qt event-delivery behavior. If changed, make sure callers that immediately read from the returned `QNetworkReply*` still own a live object until after the read completes.

**Re-review:** Thoughts are correct and the downgrade is justified. All `delete reply` sites are reached only after `QEventLoop::exec()` returns in the same call frame, so the original "use-after-free" framing was overstated. Switching to `deleteLater()` remains defensible for codebase consistency, not safety.

---

### 5. `boxartmanager.cpp` swallows all exceptions during cache population

**Severity:** Medium

**Files:**
- `app/backend/boxartmanager.cpp:113-115` (`catch (...) {}` around `loadBoxArtFromNetwork()` work)

A bare `catch (...) {}` discards every exception including `std::bad_alloc` and the project's own `GfeHttpResponseException` / `QtNetworkReplyException`. The only signal that anything went wrong is the *absence* of art in the UI; logs are silent. This makes box-art issues effectively unreportable by users.

**Possible solution:** Catch `std::exception const&` and log `e.what()`; let other unknown types propagate (or log and continue). Consider a more granular `catch (const GfeHttpResponseException& e)` for HTTP errors that produces structured warnings.

**Thoughts:** Valid. This is a real diagnostics problem more than a user-visible correctness issue because missing box art is non-fatal. I would avoid letting unknown exceptions escape a background `QRunnable` because that could terminate the process; log known exceptions and add a final catch that logs an unknown failure instead of swallowing it silently.

**Re-review:** Thoughts are correct. Letting an exception escape a `QRunnable::run()` calls `std::terminate`, so the existing `catch (...)` is intentionally defensive but should at minimum log via `qWarning()` before swallowing.

---

### 6. `CompatFetcher` persists arbitrary HTTPS-server output into `QSettings` without bounds or validation

**Severity:** Low

**Files:**
- `app/settings/compatfetcher.cpp:137-140`

```cpp
QString version = QString(reply->readAll()).trimmed();
QSettings settings;
settings.setValue(COMPAT_KEY COMPAT_VERSION, version);
```

The endpoint is HTTPS to a Moonlight-controlled server, so this is not an external-attacker concern — but `readAll()` has no size cap and the output is written verbatim into the user's persistent settings. A misconfigured server response (e.g. an HTML error page, a several-MB blob) will be persisted and reloaded forever, slowing settings reads and producing nonsensical version-comparison failures.

**Possible solution:** Cap `readAll()` to a small ceiling (a few KB), reject empty trimmed payloads, and reject anything that isn't a dotted version (`^\d+(\.\d+)*$`). Log a warning and don't persist on failure.

**Thoughts:** Valid and correctly Low. The endpoint is trusted HTTPS, but persistent settings should not accept arbitrary payloads forever. The proposed dotted-version validation matches how `isGfeVersionSupported()` later parses the value and would prevent HTML/error-page responses from becoming sticky state.

**Re-review:** Thoughts are correct. Validating against the same dotted-version grammar the consumer expects is the cleanest fix and avoids silent acceptance of HTML error bodies.

---

### 7. `MappingFetcher` writes the server response to disk with no length cap

**Severity:** Low

**Files:**
- `app/settings/mappingfetcher.cpp:90-93`

```cpp
QByteArray data = reply->readAll();
if (!data.isEmpty()) {
    Path::writeCacheFile("gamecontrollerdb.txt", data);
}
```

Same shape as finding 6 but writes to a file rather than `QSettings`. The realistic exposure is a misbehaving CDN/origin returning a giant body, which would consume disk and blow out the cache path. Combined with the silent-failure issue from finding #14 of the first review, the user has no way to know it happened.

**Possible solution:** Cap response length (e.g. 4 MB); if exceeded, log a warning and skip the write. Validate that the file at least begins with a non-empty line.

**Thoughts:** Valid, though the chosen cap should be based on expected controller database size with margin. A simple length cap plus non-empty check is enough; validating the full SDL mapping grammar would be overkill for a cache that is still parsed by SDL later.

**Re-review:** Thoughts are correct. SDL parses `gamecontrollerdb.txt` downstream, so pre-validating the grammar would just duplicate that work. A generous size cap is sufficient.

---

### 8. `AutoUpdateChecker::parseStringToVersionQuad` silently coerces malformed components to 0

**Severity:** Low

**Files:**
- `app/backend/autoupdatechecker.cpp:60-64`

```cpp
QStringList list = string.split('.');
for (const QString& component : std::as_const(list)) {
    version.append(component.toInt()); // bool* ok ignored
}
```

A version string containing non-numeric components (e.g. `5.1.0-beta`, `5..1`, `5.1.x`) becomes `[5, 1, 0]` rather than reporting an error. In `compareVersion` this can hide an "update available" notification or, conversely, present an unexpected upgrade. The same pattern is duplicated in `compatfetcher.cpp:89-104`, which at least checks `ok` — apply the same care to `AutoUpdateChecker`.

**Possible solution:** Use `toInt(&ok)` and abort comparison on parse failure with a `qWarning()`.

**Thoughts:** Valid. `compatfetcher.cpp` already demonstrates the better pattern with `toInt(&ok)` and non-negative checks, so applying the same pattern to update manifest versions is consistent. This is not security-sensitive because the update URL itself is only opened in a browser, but it can prevent wrong update prompts.

**Re-review:** Thoughts are correct. The fix is a copy of an in-tree pattern, so no new design work is needed.

---

### 9. `NvHTTP::openConnection` connects `aboutToQuit` to a nested event loop without disconnecting

**Severity:** Low

**Files:**
- `app/backend/nvhttp.cpp:505-514`

The local `QEventLoop loop` connects `QCoreApplication::aboutToQuit` to `loop.quit()` but never disconnects it. `aboutToQuit` is emitted once per application lifetime, so the dangling connection is harmless after the loop is destroyed (Qt cleans up automatically when the receiver is destroyed). However, if the helper is ever extended to re-enter the loop or used from a long-lived nested context, it would be easy to subtly leak. More importantly, the *intent* — abort outstanding requests at shutdown — is currently mixed with timeout handling (the same `loop.quit()` slot fires for both), and the caller cannot tell whether the reply finished because of completion, timeout, or shutdown. The post-loop branch only checks `reply->isFinished()`.

**Possible solution:** Track an explicit "aborted-due-to-shutdown" flag on the loop and, when set, throw a distinct exception type (or short-circuit further retries upstream). Keep the `aboutToQuit` connection scoped to the loop's lifetime via `QObject::connect(...)` returning a `QMetaObject::Connection` that is disconnected on the way out.

**Thoughts:** Mostly valid, but the connection lifetime itself is not a leak because Qt disconnects when the local `QEventLoop` receiver is destroyed. The stronger point is semantic: timeout, application shutdown, and successful completion all collapse to "the local event loop quit." Tracking the reason would make shutdown paths less likely to run normal retry/error-recovery logic.

**Re-review:** Thoughts are correct. The semantic conflation between completion / timeout / shutdown is the actionable issue; the connection-lifetime framing in the original finding is largely a non-issue thanks to Qt's automatic cleanup.

---

### 10. `clearAccessCache()` is called on every legacy-Qt request, racing other concurrent requests

**Severity:** Medium

**Files:**
- `app/backend/nvhttp.cpp:525-528`

Under Qt < 6.3, after every single request the code clears the entire access cache of the shared `QNetworkAccessManager`. Comments elsewhere note that GFE refuses persistent connections and that `clearAccessCache()` "tears down the NAM's global thread each time". `ComputerManager` runs multiple polling tasks concurrently, each constructing its own `NvHTTP` but sharing the per-thread NAM lifecycle in older Qt versions. Tearing down the cache from one request while another is mid-handshake can manifest as transient `RemoteHostClosedError` or sporadic SSL handshake failures that retry and recover — visible mostly as flaky pairing/quitting against GFE.

**Possible solution:** Most users are now on Qt ≥ 6.3 where this branch is bypassed. For the legacy branch, gate `clearAccessCache()` behind a per-`NvHTTP` reference count (only clear when no other request is pending on the same NAM) or simply accept the connection-keepalive behaviour of older Qt by switching to a fresh NAM per request, which costs more but avoids the global cache thrash.

**Thoughts:** This finding is weaker than written. The polling thread intentionally shares a `QNetworkAccessManager`, but its `NvHTTP::openConnection()` calls are synchronous, so it is not obviously clearing the cache while another request on that same NAM is mid-flight. The legacy branch may still be inefficient or cause flaky behavior on older Qt, but I would downgrade this to Low and validate with runtime logs before changing architecture.

**Re-review:** Thoughts' downgrade is correct. Synchronous calls on the same thread can't overlap, and the `Q_GLOBAL_STATIC` NAM is per-thread; cross-thread races on a shared cache require a multi-NAM picture that this code does not have. Treat as Low and only act on observed log evidence.

---

### 11. `openConnectionToString` reads the full reply via `QTextStream` with no length cap

**Severity:** Low

**Files:**
- `app/backend/nvhttp.cpp:446-460`

The function performs `stream.readAll()` against a paired host's HTTPS response. Since the cert is pinned post-pairing, the only attacker model is a host the user already trusts going rogue or being compromised — but a malformed/oversized `applist` response will allocate unbounded memory in the GUI thread (this call is synchronous from the UI). In practice GFE responses are small (tens of KB), so this is mostly a robustness concern, not a security one.

**Possible solution:** Cap reads to a few MB; if exceeded, throw `QtNetworkReplyException` and treat the host as unreachable. Same fix applies to `openConnectionToData`.

**Thoughts:** Valid. This is a robustness guard against trusted-but-buggy hosts rather than a remote unauthenticated attack. A cap should be high enough for large app lists and artwork metadata, and the error should identify which command exceeded the limit so support logs are useful.

**Re-review:** Thoughts are correct. Cap should be generous (multi-MB) since GFE applist responses can be sizeable; the goal is preventing unbounded allocation, not enforcing a tight quota.

---

### 12. `QThreadPool::waitForDone(30000)` may silently abandon in-flight work at shutdown

**Severity:** Low

**Files:**
- `app/main.cpp:1037`

If a `QRunnable` is still running after 30 seconds (e.g. a stuck network call inside `DeferredHostDeletionTask`, or a long unpair attempt), `waitForDone` returns early and the process continues to teardown while the runnable is still executing on a pool thread. That thread will then touch already-destroyed singletons (`IdentityManager`, `Path` cache state) on its way out. The 30-second timeout was added to avoid a hang, but it converts a hang into a potential UAF crash.

**Possible solution:** Before exiting `app.exec()`, set an "is shutting down" flag observed by long-running tasks and have them check it on each network call boundary. Keep the 30-second cap as a *report* (log which task is still running) but issue an `_exit()` rather than a normal teardown if the cap is hit, so destructors don't race the threads.

**Thoughts:** Partially valid. A bounded wait can leave work running while teardown proceeds, so the concern is real, but jumping straight to `_exit()` is a heavy-handed remedy for a GUI client. A better first step is to make long-running `QRunnable`s cancellation-aware, log when the timeout is hit, and avoid destroying shared singletons until the global pool is actually done.

**Re-review:** Thoughts are correct. `_exit()` skips destructors and Qt cleanup which is too aggressive for a normally-exiting GUI client. Cancellation-aware tasks plus logged-and-extended waits is the right escalation order.

---

### 13. `NvPairingManager::saltPin` produces only 4-decimal-digit PIN entropy

**Severity:** Informational (protocol-level)

**Files:**
- `app/backend/nvpairingmanager.cpp:197-200, 222-227`

`saltPin` concatenates a 16-byte salt with the user-entered PIN and hashes (SHA-256 on Gen 7+). The PIN UI (`PairingDialog.qml`) restricts to four decimal digits (10⁴ = 10000 possibilities). An attacker on-path during the unencrypted Stage #1 ↔ #2 traffic captures the salt and the encrypted client challenge; brute-forcing the PIN offline is trivial. This is a property of the GFE pairing protocol that Moonlight cannot unilaterally fix without breaking interoperability — but it deserves to be acknowledged in the threat model and mitigated where possible.

**Possible solution:** Allow the user to optionally enter a longer PIN (the dialog could permit 6–8 digits and Moonlight already passes the PIN through verbatim). On the host side, only Sunshine could realistically widen this; for GFE compatibility the four-digit limit is fixed. At minimum, document the LAN-only assumption in the README/security guidance.

**Thoughts:** Partially valid but the UI detail is inaccurate. Moonlight generates the PIN with `QRandomGenerator::system()->bounded(10000)` in `ComputerManager::generatePinString()` and asks the user to enter it on the host; it does not expose a text field limited to four digits in `PairingDialog.qml`. The protocol-level entropy concern is still correct for the generated PIN, but the practical solution is not simply "let the user enter a longer PIN" unless the host protocol/UI accepts it. Documentation and Sunshine-side protocol support are the realistic paths.

**Re-review:** Thoughts correctly fix a factual error in the original finding. The 4-digit PIN is generated client-side by `bounded(10000)`, not constrained by a QML input field. Real mitigation requires Sunshine to widen the protocol; for GFE the entropy ceiling is fixed and only documentation is actionable.

---

### 14. `Session::s_ActiveSession` global is read from Limelight callbacks on threads we don't own

**Severity:** Low (refinement of finding #4 in the first review)

**Files:**
- `app/streaming/session.cpp:74-131, 178-211, 343-376, 2194`

Most callbacks are thread-confined to the streaming threads Limelight spawns, but several emit Qt signals (`stageStarting`, `stageFailed`, `displayLaunchError`, etc.) directly from those threads. Qt cross-thread signal emission to objects that live on the main thread is generally safe via queued connections, but the receivers in `main.qml` are connected via `Qt.AutoConnection` from QML, which evaluates connection type at emit time using the *receiver*'s thread affinity. If a callback fires after the QML object has been destroyed (window close races), Qt will route a queued event to a destroyed receiver. The first review identified the lifetime concern in general terms; this is the concrete shape of it.

**Possible solution:** During `DeferredSessionCleanupTask`, explicitly disconnect all signals from the `Session` object before clearing `s_ActiveSession`, and make every `emit s_ActiveSession->...` site a member function call where `this` ownership is unambiguous.

**Thoughts:** This is weaker than written. Qt automatically disconnects destroyed receivers and removes queued events for destroyed `QObject`s, so "queued event to a destroyed receiver" is not the main risk. The better concern is still the static global pointer and teardown ordering from the first review. If this area is changed, use explicit shutdown flags or `QPointer`/queued invocations to the owning `Session` object rather than broad signal disconnection that could hide late errors.

**Re-review:** Thoughts are correct. Qt's destroyed-receiver cleanup defeats the specific scenario in the finding; the residual concern overlaps with first-review #4 and should be tracked there rather than as a separate item.

---

### 15. `keyboard.cpp` maps `KP_ENTER` to the same scancode as `Return` with a self-questioning FIXME

**Severity:** Low

**Files:**
- `app/streaming/input/keyboard.cpp:252-254`

```cpp
case SDL_SCANCODE_KP_ENTER: // FIXME: Is this correct?
case SDL_SCANCODE_RETURN:
    keyCode = 0x0D;
    break;
```

Windows distinguishes VK_RETURN (0x0D) from the keypad enter via the extended-key flag in the LParam. Several applications (notably some IDEs, calculators, Excel formula entry) treat keypad-enter differently. Moonlight currently merges them, losing that distinction at the host. The FIXME has lived in the file since the initial keyboard mapping commit.

**Possible solution:** Send the same VK with `MODIFIER_KEY_EXT`-equivalent metadata if Limelight exposes it (or extend the key event to carry the extended flag). At minimum, audit other keypad keys (KP_PLUS, KP_DIVIDE) for the same conflation.

**Thoughts:** Valid as a behavior gap, but the proposed solution needs protocol confirmation. `moonlight-common-c` exposes `LiSendKeyboardEvent2()` with a Sunshine `SS_KBE_FLAG_NON_NORMALIZED` flag, but there is no obvious extended-key flag for keypad-enter in the current Limelight API. This should be handled as an input-compatibility investigation: first determine whether Sunshine/GFE can represent keypad enter distinctly, then update the mapping only if the protocol supports it.

**Re-review:** Thoughts are correct. Without a Limelight-protocol way to mark the extended-key bit, no host-side fix is possible; treat as upstream protocol investigation rather than a client-side change.

---

## Suggested follow-up order

1. **Findings #1, #2, #4** — pairing OpenSSL hardening, replacing release-build assertions on network data, and `deleteLater()` migration. Concrete correctness/UAF concerns.
2. **Finding #5** — narrow the `catch (...) {}` in `BoxArtManager` so failures become visible.
3. **Finding #12** — review the shutdown sequence in `main.cpp` against tasks that may be running on the global pool.
4. **Findings #6–#8, #11** — bound and validate network responses persisted into settings, cache files, and parsed for version comparison.
5. **Findings #3, #9, #10, #15** — code-quality and legacy-Qt cleanup.
6. **Findings #13, #14** — informational; document and refine where feasible.

## Out of scope

These were reviewed but found to be acceptable as-is or already covered by the first review:

- Pinned-cert HTTPS for paired hosts (`handleSslErrors` design): correct given the threat model; first review already acknowledged this is intentional.
- AES-128-ECB use in pairing: required by the GFE protocol; not changeable without breaking compatibility.
- HTTPS endpoints for `compatfetcher`/`mappingfetcher`/`autoupdatechecker`: use Qt's default CA store with HSTS enabled, which is appropriate for a public origin under Moonlight's control.
- The `try {} catch (...)` in `boxartmanager.cpp:115` is the only fully-silent swallower; the others (`computermanager.cpp:39, 66, 806`) at least set state or log on the failure path.
