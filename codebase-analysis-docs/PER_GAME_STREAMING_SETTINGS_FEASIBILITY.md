# Per-game streaming settings: feasibility and implementation plan

Date: 2026-09-06. Inspected baseline: `1fa39f193d8b766bd85dc5df771bb99ce0e77d6f`.
Study branch: `codex/per-game-streaming-settings`, created from `feature/profile-manager` after the navigation fix.
Status: feasibility study only; the per-game feature described below is not implemented.

## 1. Decision and intended behavior

**Feasible within the existing Qt/QML client architecture.** Session already accepts a `StreamingPreferences*` argument. The main work is providing an isolated preferences instance, persisting field overrides, and adapting the current settings screen to edit that instance. No new streaming protocol or server endpoint is required by this design.

User-visible behavior:

1. Open a game's context menu using right-click or the controller's existing menu button (X, subject to the profile's face-button mapping).
2. Choose **Streaming Settings**. The title identifies the game and host. Existing settings controls show the effective values, initially inherited from the active profile.
3. Changing a setting creates a customization automatically. No separate enable-custom-settings switch is needed. Untouched settings continue to inherit from the profile.
4. Leaving the settings page saves the user's changes, matching the existing profile settings workflow. Merely opening and closing the page writes nothing.
5. A reset action for an overridden field/group returns it to **Use Profile Setting**. **Remove Custom Settings** in the game's context menu, and a reset-all action in the editor, remove all overrides after a confirmation.
6. Return to the same game with controller focus restored. An optional small settings badge indicates an existing customization.
7. The next launch or resume uses the effective configuration. An already connected stream is not reconfigured live.

The base is the **active profile**, including when it is not the profile marked Default. Per-game settings do not change the profile identity used for Sunshine pairing.

"Game" means a host-provided `NvApp` entry. A Steam or Desktop entry has one configuration for that entire stream. Automatically recognizing a different executable opened inside the stream would require additional host integration and is outside this design.

### STATE BLOCK - Phase 1

- Complete: requested behavior, feasibility conclusion, scope, baseline, and branch.
- Confirmed reusable elements: session preferences injection, profile-scoped QSettings, app IDs, settings controls, navigable context menus.
- Next: persistence boundaries, session lifetime, and UI side effects.

## 2. Evidence from the current implementation

All paths below are relative to the repository root. Links point directly to the inspected sources; function names identify the relevant implementation.

| Source | Existing implementation | Consequence |
| --- | --- | --- |
| [ProfileManager](../app/backend/profilemanager.cpp), `profileSettingsGroup()`, `beginProfileSettings()` | Groups settings under `profiles/<profileId>`; supports an explicit profile ID. | Reuse the existing namespace; capture the ID at editor creation. |
| [StreamingPreferences](../app/settings/streamingpreferences.cpp), `get()`, `reload()`, `save()` | One shared QObject; loads/saves all profile preferences. `save()` uses the active profile at call time. | A per-game editor must not write through this singleton or call its ordinary profile save path. |
| [StreamingPreferences header](../app/settings/streamingpreferences.h) | Constructor is private; contains directly accessible members and QML properties, not a copyable value type. `packetSize` is not a QML property. | Add an explicit transient-copy factory; ordinary QObject copying is not available. Account for non-QML members. |
| [AppModel](../app/gui/appmodel.cpp), `initialize()`, `createSessionForApp()` | Has the selected `NvComputer` and `NvApp`; creates `new Session(m_Computer, app)`. | Resolve per-game settings here for normal launch, resume, and direct launch. |
| [Session](../app/streaming/session.cpp), constructor, `initialize()` | Optional preferences pointer already exists; fullscreen state is derived in the constructor. | Supply effective settings before construction, not only before `start()`. |
| [Session](../app/streaming/session.cpp), `setShouldExit()`, `DeferredSessionCleanupTask::run()` | Mutates `quitAppAfter` and reads preferences during asynchronous cleanup. The destructor currently does not own/delete the pointer. | Session must own its detached configuration until cleanup completes; UI lifetime is insufficient. |
| [AppView](../app/gui/AppView.qml), `appContextMenu`, `launchOrResumeSelectedApp()`, `quitAppDialog.quitApp()` | Shared menu and launch/resume paths; the quit-then-launch path prepares the next Session separately. | Add editing/reset menu actions and cover every launch route. |
| [SettingsView](../app/gui/SettingsView.qml), activation/deactivation/destruction handlers | Controls reference `StreamingPreferences` directly; saves on both deactivation and destruction. Some initialization code invokes `activated()`. | Reuse controls with an injected preferences target and an explicit commit function. Initialization must not create overrides. |
| [SettingsView](../app/gui/SettingsView.qml), bitrate/resolution/FPS controls | Resolution/FPS changes can recalculate bitrate; slider movement disables automatic bitrate. | Persist and resolve coupled options consistently. |
| [NvApp](../app/backend/nvapp.h), [NvHTTP](../app/backend/nvhttp.cpp), `getAppList()` | Host supplies an integer app ID; titles are labels. | Identify a game by host UUID plus app ID, within a profile. |
| [NvComputer](../app/backend/nvcomputer.cpp), `updateAppList()`, `serialize()` | Replaces refreshed app lists, preserving only selected client attributes; serializes cached apps under hosts. | Store stream overrides separately from the downloaded/cached app list. |
| [CLI launcher](../app/cli/startstream.cpp), `LauncherPrivate::handleEvent()` | Creates a Session after resolving the requested host/app. | Add the same resolution step for CLI launch. |
| [CLI parser](../app/cli/commandlineparser.cpp), `StreamCommandLineParser::parse()`; [main](../app/main.cpp) | Applies command-line settings before host/app discovery and may calculate bitrate as a side effect. | Retain explicitly supplied options and apply them after the per-game layer. |

### STATE BLOCK - Phase 2

- Complete: entry points, ownership, persistence, CLI, and UI reuse mapped to source.
- Main constraints: singleton mutation, constructor-time reads, deferred cleanup, initialization callbacks, bitrate coupling.
- Evidence limit: client source was inspected; behavior on each Sunshine version and hardware configuration still needs integration testing.

## 3. Storage and inheritance

Use a sparse QSettings subtree:

```text
profiles/<profileId>/
    width, height, fps, bitrate, ...                 existing profile preferences
    gameStreamingSettings/<hostUuid>/<appId>/
        width, height                               only if resolution overridden
        fps                                         only if overridden
        ...                                         other overridden fields
```

Reuse existing serialization key names and enum values. Do not introduce profile version fields or copy certificates, pairing keys, host records, or identity values into this subtree. The local shape is illustrated in [the example fixture](assets/per_game_streaming_settings.example.json); that JSON is documentation, not a proposed second storage backend.

Resolve in this order:

```text
Current profile values -> stored game overrides -> explicit CLI options -> existing session validation/negotiation
```

An absent key means inherit. Presence must be tested with `QSettings::contains()` or map membership: `false`, zero, and an enum's zero value are valid overrides for applicable fields. Do not use truthiness to detect an override.

### Save and reset rules

- Maintain a draft plus the original override map and user-edited fields. At commit, write only actual user changes. Programmatic initialization and capability detection do not mark the draft dirty.
- If a user changes a field back to its current profile value, remove that field's override. Untouched existing overrides remain explicit, even if the profile later happens to match them.
- Treat resolution width/height as one override group, so a later profile resolution change cannot produce a mixed pair.
- Treat a manually selected bitrate as a group containing `autoadjustbitrate=false` and `bitrate`. Resetting bitrate removes both overrides. The editor must distinguish **Use Profile Setting** from the existing calculated **Use Default (N Mbps)** action.
- If automatic bitrate is effective and resolution/FPS/YUV444 is overridden, calculate bitrate from the effective values using the existing `getDefaultBitrate()`. Do not store a stale calculated bitrate as a fixed override. A deliberately fixed profile bitrate remains inherited unless the game overrides bitrate policy.
- With no game overrides and no CLI options, preserve current profile behavior exactly, including its stored bitrate. Normalizing an unrelated field must not change launch parameters.
- Remove an empty per-game group. Removing the complete group immediately restores inheritance for future streams.
- Freeze `(profileId, hostUuid, appId)` in the editor and store APIs. If that profile or host is deleted before commit, discard the stale draft or show an error; never recreate deleted settings or write to whichever profile happens to be active.
- Validate known keys, types, enum membership, positive dimensions, numeric ranges and coupled options. Apply appropriate existing UI limits and platform checks; invalid persisted values fall back to the profile and produce a concise diagnostic.
- Validate/encode host and profile identifiers before using them as QSettings path segments. Do not require all profile IDs to be UUIDs: the initial profile uses `default`. Never construct storage paths from game titles.

Example: a profile is 1080p/60 FPS with automatic bitrate. A game overrides only FPS to 120. Changing the profile to 1440p updates that game's resolution, keeps its 120 FPS, and recalculates automatic bitrate. Removing the FPS override makes it follow the profile completely again.

### Cleanup and identity changes

`ProfileManager::removeProfile()` already removes the whole profile settings group, so nested game settings follow profile deletion automatically. Add targeted removal for a host's subtree to the host deletion path in `ComputerManager::deleteHost()`/`DeferredHostDeletionTask`, using the manager's captured profile ID rather than ambient active-profile state.

Do not erase overrides merely because a refresh temporarily omits an app or the host is offline. Renames/reordering preserve settings while the host UUID and app ID are unchanged. A changed ID means a new entry; do not guess matches by title. The inspected client does not guarantee that a server will never change or reuse an app ID. Retention on temporary disappearance is the proposed policy, with explicit reset available if a server reuses an ID.

### STATE BLOCK - Phase 3

- Complete: composite key, sparse storage, inheritance, no-op edits, resets, validation, and cleanup policy.
- Decisions: existing QSettings backend; field/group overrides; no new profile version mechanism.
- Residual external constraint: app ID stability depends on the host's app catalog.

## 4. Reusing the current settings UI

Adapt `SettingsView.qml` to accept a preferences object (defaulting to the current singleton) and an editing context identifying profile-wide or per-game editing. Keep enum constants and pure helpers on `StreamingPreferences`; redirect instance reads/writes to the supplied object. Audit `Component.onCompleted`, `activated(currentIndex)`, `onToggled`, and slider callbacks so that rendering the screen cannot create a customization.

Keep one set of resolution, FPS, bitrate, codec, HDR, audio and input controls. Filter application-wide controls in game mode individually: some useful streaming controls are currently placed inside the UI Settings group, so hiding that whole group would hide too much.

| Settings | Proposed scope |
| --- | --- |
| Resolution, FPS, bitrate mode/value, bitrate limit, V-sync, frame pacing | Per-game; honor the coupled rules above. |
| Codec, HDR, YUV444, decoder and renderer selection | Per-game; reuse capability visibility, validation and fallback. |
| Streaming window mode, audio configuration, host audio, mute on focus loss | Per-game; retain the existing note that changing host audio may require restarting a running game. |
| Game optimization, quit app after disconnect | Per-game; preserve existing server/platform semantics and warnings. |
| Mouse mode, touch mode, mouse buttons/scroll, key capture, gamepad mapping options, background input | Per-game for the stream. The navigation controller continues using profile preferences. |
| Stream performance overlay, connection/configuration warnings, keep awake | May be overridden for the session; application startup checks still use profile preferences. |
| GUI display mode, language, host discovery, pairing/identity, profiles/defaults, controller device mappings, Rich Presence privacy preference | Profile/application scope; not editable in game mode. |
| Packet size and network-block detection | Keep current profile/CLI scope for the first version; there is no existing packet-size UI to reuse. |

The per-game route must carry host/app identity and use an instance-specific navigation target. `main.qml::navigateTo()` currently deduplicates by type, so reusing it unchanged for two SettingsView instances could return to the wrong editor. Route through a dedicated per-game wrapper or extend lookup with the editing key.

Use `NavigableMenu`, `NavigableMenuItem`, existing message dialogs, and `AutoResizingComboBox`. The game editor uses settings-style controller navigation while active and restores grid navigation when leaving. The toolbar, B/Escape, reset confirmation and repeated Back must obey the transition/focus rules fixed in `1fa39f19`.

Returning to the grid restores selection by **app ID**, not an old row index. App lists are sorted and refreshed while settings are open. If the app has disappeared, focus the nearest valid item or the empty-grid fallback. A reset updates the customization indicator through a model notification without resetting the entire grid.

Profile settings keep their current saving semantics. Game settings commit through the per-game store, with one idempotent commit routine shared by normal Back and window close. The routine must not run a second save that recreates settings after reset, or save under a newly selected profile during destruction.

## 5. Session integration and lifetime

Recommended small additions (names are proposed, not existing APIs):

- `StreamingPreferences::createTransientCopy()`: create an independent object with the current effective fields, without attaching the global QML engine or reading/saving another profile. Keep enum definitions and bitrate helpers in the existing class. Include `packetSize` and other non-QML members in the copy.
- `GameStreamingSettings`: a store/resolver using explicit profile/host/app IDs, with `loadOverrides()`, `saveChanges()`, `removeOverrides()`, `hasOverrides()` and `resolvePreferences()`. Share serialization metadata with the existing preferences code rather than scattering duplicate string-to-field conversions through models and QML.
- `AppModel`: expose editor creation/reset operations and a `hasCustomStreamingSettings` role. Capture app IDs on invocation and validate indexes in release builds before dereferencing.
- A session-owned preferences snapshot. Preserve existing callers' borrowed-pointer contract unless explicitly updating them; the supplied input can be cloned into a `std::unique_ptr<StreamingPreferences>` owned by Session, with `m_Preferences` referring to that clone. Place ownership before members initialized from the preferences.

Transient preferences must not execute the ordinary profile `save()` path. Make persistence explicitly unavailable/guarded for a transient instance, and let the store commit only the sparse override map. They must also not run UI retranslation.

Do not temporarily apply game values to `StreamingPreferences::get()` and restore them later. Besides contaminating another game or the editor, `Session::setShouldExit()` mutates `quitAppAfter`, and deferred cleanup reads that flag after the visible stream has ended. Separate storage and an owned snapshot avoid persisting runtime mutations.

Once a session starts, configuration is detached from future profile/editor changes. Lifetime extends through `DeferredSessionCleanupTask` and `readyForDeletion`, not merely until the settings screen or AppView is popped. Leave capability probing, validation warnings, codec negotiation, renderer selection and fallback in their current Session paths. Fallback modifies the running configuration without rewriting the user's requested override.

See [the Mermaid data-flow diagram](assets/per_game_streaming_settings.mmd).

### Every launch path

| Path | Integration requirement |
| --- | --- |
| Normal launch and context-menu launch | Resolve using the selected app at `AppModel::createSessionForApp()`. |
| Resume | Resolve the resumed app; use the existing `Session` and `NvHTTP::startApp()` request path. |
| Quit another game, then launch | `quitAppDialog.quitApp()` must pass the **next app's** resolved configuration, not the running app's. |
| Direct Launch | Uses AppView's ordinary launch path; test it together with default profile and default host navigation. |
| Hidden app opened via View All Apps | Resolve by app ID, independent of the filtered list/index. |
| Command-line stream | Resolve after the host and app are found, then overlay only explicitly supplied CLI options before constructing Session. |

The CLI parser currently mutates a preferences object early, while the app name is not yet resolved to an ID. Extend it to retain a parsed override map or split argument parsing from applying options. Do not detect explicit flags by diffing against profile defaults: a supplied `--fps 60` remains intentional even if the profile is already 60. Existing resolution/FPS-driven bitrate behavior and explicit bitrate precedence must survive this change, with bitrate calculated after effective YUV444 is known.

The wire request already carries app ID, mode, HDR, host audio and controller information through `NvHTTP::startApp()`; client-side decoder/input preferences are applied locally. This is customization of the client's stream request and behavior, not general editing of arbitrary Sunshine server settings. A running game's own graphics options or host preparation scripts are not guaranteed to be reapplied on resume. Validate launch and resume against the supported hosts rather than promising identical host-side effects.

### STATE BLOCK - Phase 4

- Complete: UI reuse, controller behavior, proposed APIs, Session ownership, and complete launch-path inventory.
- Key change boundaries: settings class/store, AppModel/AppView, SettingsView routing, Session ownership, CLI parser/launcher, host deletion.
- No per-game implementation has been added on the study branch.

## 6. Work plan

Each step is intended to be independently reviewable. The navigation fix is already committed on the base branch.

| Step | Work | Completion check |
| --- | --- | --- |
| 1. Detached preferences | Add explicit transient-copy support and persistence guards to StreamingPreferences. Share field metadata where useful; do not duplicate the whole settings schema. | Copy includes all runtime fields; mutations/save attempts cannot alter profile data. |
| 2. Sparse store and resolver | Add GameStreamingSettings, composite-key validation, group handling, reset and targeted change notifications. | Unit tests for inheritance, isolation, valid false values, no-op edits, coupled bitrate and deletion. |
| 3. GUI session integration | Supply effective preferences through AppModel and define Session ownership before constructor-time reads. | Launch, resume, quit-then-launch and direct launch receive the correct app's values; cleanup has no dangling preferences. |
| 4. CLI integration | Preserve explicit flags and apply profile -> game -> CLI precedence after discovery. | Unspecified options inherit; explicit options win even if equal to the base profile; no CLI writes to saved settings. |
| 5. Shared editor and game menu | Parameterize SettingsView; add per-game route, editing context, reset affordances and customization indicator. | Open/close unchanged creates nothing; changes affect only that game; B/A/X and mouse work throughout. |
| 6. Lifecycle and cleanup | Integrate profile/host deletion, frozen editing IDs, stale draft prevention and ID-based focus restoration. | Refresh, rename, hidden games, offline hosts and rapid Back cannot misapply settings or lose focus. |
| 7. Platform checks and documentation | Register sources/resources in app.pro/qml.qrc; add translatable UI strings; document inheritance and resume limits. | Qt unit/UI tests pass, supported platform builds pass, real Sunshine launch/resume checks completed. |

Estimated effort: approximately 5-8 engineering days, plus access to Windows/Linux/macOS and HDR/audio-capable hardware for integration checks. This is a planning estimate, not a measured delivery commitment. The shared settings screen and CLI precedence have the largest regression surface; the persistent store is comparatively small.

## 7. Acceptance matrix

| Scenario | Required result |
| --- | --- |
| No custom settings | Exactly the active profile's behavior; no extra stored game group. |
| Open and leave editor without changing anything | No write and no customization indicator. |
| Change one field; change profile later | Only the overridden field stays fixed; all other fields inherit the updated profile. |
| Different profiles, same host/app | Fully isolated overrides. |
| Different hosts, same numeric app ID | Fully isolated overrides. |
| Reset one group; reset all | Group inheritance restored; complete reset removes the game group and indicator. |
| Reset, then close/destroy editor | Removed values are not written back by a second save. |
| Change resolution/FPS/YUV444 with automatic bitrate | Bitrate follows effective values; fixed manual bitrate stays intentional. |
| Unsupported HDR/codec/renderer | Existing validation/fallback applies; requested overrides are not rewritten by negotiation. |
| Offline host, missing app, filtered list, rename/reordering | No index-based misassociation; drafts/settings retain valid identity. |
| Delete host/profile while a draft exists | Targeted deletion; stale draft cannot resurrect the data or affect another profile. |
| Launch/resume/quit-then-launch/direct launch/CLI | All use the intended app's effective snapshot. CLI explicit flags take precedence. |
| Disconnect with quit-host-app shortcut | Session-local mutation does not persist `quitAppAfter=true` into the profile/game. |
| Controller menu -> editor -> reset dialog -> rapid Back | Focus stays on the visible page; return selects the original game when it still exists. |
| Normal profile settings page | Existing controls, global language/UI actions and save behavior remain functional. |

Add unit tests for store/resolver using an isolated temporary QSettings location. Extend the existing [navigation regression target](../tests/navigation/README.md) to cover editing, menu reset, list reordering and profile switches. Test actual Session parameter propagation separately; the navigation fixtures deliberately do not perform streaming.

## 8. Completed prerequisite and remaining verification

Navigation fix: `1fa39f19`, committed and pushed to `feature/profile-manager`. It guards overlapping navigation, prevents hidden profile focus/activation, ignores stale host-loss navigation, gives removed game views/models an owned lifetime, and handles grid detachment safely.

Verification on Qt 6.11.0 / Windows: the regression target reproduced four failing cases before the fix; the final suite reports 10 passes including setup/teardown (8 behavior cases). Input is injected as the Qt keys used by the controller adapter. A physical-controller/Sunshine end-to-end test and a complete application/platform build were not performed in this task.

The Build workflow for the fix was requested to stop immediately after push and confirmed **cancelled**: [GitHub run 34046003042](https://github.com/Daviex/moonlight-qt/actions/runs/34046003042). Workflow configuration remains enabled for subsequent development. This documentation-only study can be committed with `[skip ci]` to avoid starting an unnecessary release build.

### STATE BLOCK - Phase 5

- Complete: feasibility study, source map, inheritance semantics, UI plan, launch/cleanup integration, work sequence, test matrix and Mermaid asset.
- Implemented prerequisite: rapid-back controller/focus fix on the parent profile branch.
- Pending work: steps 1-7 above. Per-game settings remain a design, with no runtime implementation on this branch.
- Proposed policies requiring no blocker now: sparse inheritance, save on leaving, explicit field/all reset, retention for temporarily missing apps, CLI flags highest priority.
- Remaining validation: physical controllers, supported platform builds, Sunshine/GFE behavior and real decoder/audio combinations.
