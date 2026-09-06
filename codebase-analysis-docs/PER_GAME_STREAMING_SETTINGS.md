# Per-game streaming settings implementation

Date: 2026-09-06. Branch: `codex/per-game-streaming-settings`.

This is the implementation reference. The earlier
[feasibility study](PER_GAME_STREAMING_SETTINGS_FEASIBILITY.md) records the design
investigation, not the current completion state.

## Behavior

Open a game's existing context menu, using right-click or controller X, and choose
**Streaming Settings**. This opens the existing settings controls with an isolated
configuration for that game. Opening the page alone writes nothing.

Changing values creates sparse custom settings. Untouched values follow the
**active profile**, not necessarily the profile marked Default. Back and window
close save the draft. The context-menu caption includes **Custom** when overrides
exist. The editor footer lists customized settings; **Use Profile Setting** resets
the selected field/group. **Reset All** and the game's **Remove Custom Settings**
menu action remove the entire customization after confirmation.

Profile/application controls (language, GUI window mode, mDNS, Rich Presence,
network-block detection) are hidden in game mode. Streaming controls retain their
platform visibility and capability rules. Identity and pairing are unchanged.
Streaming face-button layout may differ per game; navigation keeps the profile's
mapping. Changes apply at the next stream launch/resume, not live.

The controller retains its existing mapping: X opens the game menu; A activates
an item (Return in grids/menus, Space in settings); up/down traverses settings
using Tab/Shift+Tab; B returns or dismisses a popup. Returning restores the edited
game by app ID, with a nearest-item fallback if it disappeared. Settings become
a single column below 1100px to keep controls inside the window.

### STATE BLOCK - UI

- Implemented: editor, automatic customization, reset one/all, menu indicator,
  controller focus recovery, narrow layout and Italian translations.
- Reused: SettingsView, AutoResizingComboBox, NavigableMenu/MessageDialog and
  SdlGamepadKeyNavigation. There is no second settings-control implementation.

## Storage and resolution

QSettings stores only overrides below:

```text
profiles/<profileId>/gameStreamingSettings/<hostUuid>/<appId>/<settingKey>
```

The `gameFields` table in `streamingpreferences.cpp` shares keys with profile serialization and
allowlists supported properties. `validatedGameValues()` rejects invalid numeric
ranges, enums, booleans and incomplete resolution pairs. Unsafe profile/host path
segments and nonpositive app IDs are rejected. No profile version was added.

`GameStreamingSettings::pendingOverrides()` compares the effective draft with its
opening baseline and tracks user-edited fields through property notifications.
Untouched overrides remain explicit, even if the profile now
matches them. Changed values matching the base are removed. Resolution is one
width/height group. Manual bitrate stores bitrate and disabled automatic adjustment;
automatic bitrate stores policy only and derives the value from effective
resolution/FPS/YUV444. Without relevant video overrides, the profile bitrate is
preserved exactly.

`reset()` updates the draft; `save()` commits the sparse map and avoids duplicate
writes on destruction. Saving rejects a different active profile or invalidated
editor. `ComputerManager::deleteHost()` invalidates affected editors and removes
that host's settings using its captured profile ID. Profile deletion already removes
the parent subtree. Temporary app disappearance and renaming do not delete overrides.
Host app IDs may be reused; explicit reset handles that case.

### STATE BLOCK - Persistence

- Implemented: profile/host/app isolation, sparse storage, validation, bitrate and
  resolution coupling, no-op editing, reset, stale-save guard and host cleanup.
- Limitation: QSettings is not transactional; write failures are logged. Do not
  store settings in the downloaded NvApp cache or derive keys from titles.

## Runtime and ownership

```text
Active profile -> game overrides -> explicit CLI options -> Session validation
```

`StreamingPreferences::createTransientCopy()` copies QObject properties plus
non-QML packet size. Transients cannot save/reload profile settings or retranslate
the app. Enum definitions and bitrate calculation stay in the original class.

`AppModel::createSessionForApp()` resolves settings before creating Session. Normal
launch, resume, hidden apps and direct launch share this path. Quit-then-launch
captures the next app ID and resolves its current index when confirmation is
accepted, avoiding stale row indexes.

`Session` owns a detached snapshot through deferred cleanup. Constructor-time
fullscreen selection and runtime quit-after mutations use only that session's
settings, never the shared singleton.

The CLI validates arguments early on a transient, then reapplies the existing
parser after resolving the host/app. Explicit options win without duplicating
option mappings. Bitrate calculation runs after YUV444 parsing; explicit bitrate
remains highest priority. These two passes can repeat CLI warnings; neither saves.

### STATE BLOCK - Runtime

- Implemented: GUI/CLI resolution, session-owned lifetime, default/direct launch
  reuse, explicit CLI precedence and next-game identity preservation.
- Unchanged: Sunshine/NVIDIA protocol, server configuration, decoder fallback,
  packet-size UI and profile identity.

## Source map

| Responsibility | Source and key functions |
| --- | --- |
| Metadata and copies | [streamingpreferences.cpp](../app/settings/streamingpreferences.cpp): `gameFields`, `gameValues`, `validatedGameValues`, `createTransientCopy` |
| Persistence/draft | [gamestreamingsettings.cpp](../app/settings/gamestreamingsettings.cpp): `load`, `resolve`, `pendingOverrides`, `save`, `reset`, `removeHost` |
| Editor/footer | [GameSettingsView.qml](../app/gui/GameSettingsView.qml): `activate`, `resetSetting`, StackView callbacks |
| Reused controls | [SettingsView.qml](../app/gui/SettingsView.qml): `preferences`, `gameMode`, `activate`, `deactivate` |
| Menu/focus | [AppView.qml](../app/gui/AppView.qml): `openGameSettings`, `settingsAppId`, `removeSettingsDialog` |
| Model/launch | [appmodel.cpp](../app/gui/appmodel.cpp): `createSessionForApp`, `createGameSettings`, `indexOfApp` |
| CLI | [startstream.cpp](../app/cli/startstream.cpp): `LauncherPrivate::handleEvent`; [commandlineparser.cpp](../app/cli/commandlineparser.cpp): `StreamCommandLineParser::parse` |
| Session lifetime | [session.cpp](../app/streaming/session.cpp): `Session::Session`, `DeferredSessionCleanupTask` |
| Host cleanup | [computermanager.cpp](../app/backend/computermanager.cpp): `deleteHost`, `hostRemoved` |
| Regression suite | [tests/game-settings](../tests/game-settings/README.md), [tests/navigation](../tests/navigation/README.md) |

## Verification and external checks

Local checks: complete Windows x64 Release compilation with Qt 6.11/MSVC;
14 behavior cases in the new suite and 8 navigation cases, excluding setup/teardown;
new-suite coverage also run with MinGW. Tests render actual QML and exercise the
keys generated by controller navigation. Publication has separate fixture-based
workflow tests. Windows CI runs both Qt suites through
[test-settings.bat](../scripts/test-settings.bat) before builds/publication can succeed.

No real Sunshine host, physical controller, HDR display, ARM64 device, macOS or
Linux decoder was exercised locally. Platform builds run in GitHub; hardware
validation must confirm negotiated values on launch/resume. Desktop or Steam is
one app entry with one configuration for its entire stream, not a configuration
per process launched inside it.

### STATE BLOCK - Verification

- Local implementation and regression tests complete; CI is allowed only after
  final checks and the completed implementation commit.
- Premature build cancelled. Intermediate work did not trigger CI.
- Hardware and multi-platform outcomes must be reported from actual execution.
