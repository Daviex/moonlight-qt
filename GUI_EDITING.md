# GUI editing guide

## Current direction

If the goal is to make the GUI easier to change, keep the existing **Qt Quick / QML** frontend and simplify it from inside that stack rather than trying to swap in a web frontend or another UI toolkit first.

That is the lowest-risk path because the current GUI is already tightly integrated with the native backend:

- `app/main.cpp` decides between GUI and CLI startup, registers QML types/singletons, and loads `qrc:/gui/main.qml`
- `app/gui/main.qml` owns the app shell, `StackView`, polling lifecycle, and focus/visibility behavior
- `app/gui/PcView.qml`, `AppView.qml`, `SettingsView.qml`, `StreamSegue.qml`, and the CLI segue pages are the main screens
- `ComputerManager`, `StreamingPreferences`, `SystemProperties`, `SdlGamepadKeyNavigation`, `ComputerModel`, `AppModel`, and `Session` are the main C++ entry points the UI talks to

## How the current GUI is put together

### Shell and navigation

- `app/gui/main.qml` is the top-level `ApplicationWindow`
- It pushes the initial page from the `initialView` context property set in `main.cpp`
- It starts/stops host polling based on `visible` and `active`
- It routes toolbar, back, menu, and escape behavior through a single `StackView`

### Main pages

- `PcView.qml` shows discovered/paired hosts using `ComputerModel`
- `AppView.qml` shows the selected host's apps using `AppModel`
- `SettingsView.qml` edits `StreamingPreferences`
- `StreamSegue.qml` is the bridge from UI into `Session`
- `CliStartStreamSegue.qml`, `CliPair.qml`, and related files are headless-action pages for CLI-triggered flows

### Reusable UI primitives

The GUI has a small set of shared controls that already encode keyboard/gamepad behavior:

- `CenteredGridView.qml`
- `NavigableItemDelegate.qml`
- `NavigableToolButton.qml`
- `NavigableMenu.qml`
- `NavigableMenuItem.qml`
- `NavigableMessageDialog.qml`
- `ErrorMessageDialog.qml`
- `AutoResizingComboBox.qml`

If you are simplifying the GUI, reuse these first instead of inventing a parallel set of controls.

## Safe ways to edit the current GUI

### Layout, text, and visual changes

For copy, spacing, grouping, button placement, and dialog changes, edit the QML files directly under `app\gui\`.

### Adding a new screen

1. Create the new QML file under `app\gui\`
2. Add it to `app\qml.qrc`
3. Push it from the existing `StackView` in the same style as `PcView.qml` and `AppView.qml`
4. Preserve focus behavior so keyboard/gamepad navigation still works

### Adding new data or actions

If the screen needs new backend data:

1. Extend the existing C++ model/singleton when possible (`ComputerModel`, `AppModel`, `StreamingPreferences`, `SystemProperties`, `ComputerManager`)
2. If the new type must be visible to QML, register it in `app/main.cpp`
3. Keep QML thin: presentation in QML, networking/stateful orchestration in C++

### Adding images or other bundled UI assets

Add them through the existing Qt resource system (`resources.qrc` / `qml.qrc`) and refer to them with `qrc:/...` URLs.

## What a refactor must not accidentally break

These are the parts that matter more than the exact page structure:

- `main.qml` controls when `ComputerManager.startPolling()` / `stopPollingAsync()` run
- `SettingsView.qml` saves preferences on deactivation/destruction
- `StreamSegue.qml` disables GUI gamepad navigation before streaming and re-enables it afterward
- CLI-triggered flows still go through QML segue pages, not a separate frontend
- Runtime QML files are loaded from Qt resources, not from loose files on disk

If you simplify the GUI, preserve these behaviors first and clean up layout structure second.

## Current Qt-specific quirks to preserve

There are a few workarounds in the current GUI that exist because of real Qt behavior, not because the code is arbitrary:

- `app\gui\main.qml` constrains `ToolTip.toolTip.contentWidth` using a hidden `Text` helper instead of `TextMetrics`, because `TextMetrics` produced line wrapping that did not match the real tooltip layout.
- `app\gui\NavigableDialog.qml` forces focus back to `stackView` when dialogs close. Removing that breaks keyboard/gamepad navigation after dialogs.
- `app\gui\main.qml` keeps the stream termination dialog (`streamSegueErrorDialog`) at app-shell scope instead of inside `StreamSegue.qml`, because destroying the parent while the dialog is still alive can crash on older Qt 5.12-based paths.
- Runtime language changes still avoid returning through stale `AppView` pages after `retranslate()`. The fallback is now scoped, but it is still intentional.
- `SettingsView.qml` now saves preferences asynchronously on page deactivation to avoid UI stalls from `QSettings`, but still performs a synchronous save on destruction so shutdown does not lose the latest values.

If you change any of these, retest dialog focus recovery, tooltip wrapping, runtime language switching, and stream error dismissal rather than assuming a visual-only refactor is safe.

## Recommended simplification strategy

If the current QML feels too hard to edit, the safest cleanup order is:

1. Keep `main.cpp`, `main.qml`, and the existing registered C++ services
2. Break large pages like `PcView.qml`, `AppView.qml`, and `SettingsView.qml` into smaller presentational components
3. Keep navigation, dialogs, and gamepad/focus handling on the existing shared primitives
4. Only move logic out of QML when it is clearly stateful/business logic and belongs in C++

That gives you a simpler GUI without taking on a platform migration.

## If you want to rebuild the GUI in a totally different way

It **can** be done, but there are two very different levels of change.

### 1. New GUI structure, same Qt Quick foundation

This is the realistic "big rewrite" path.

You can replace most of the existing pages and components while still keeping:

- `app/main.cpp` as the composition root
- the registered C++ singletons/models
- the Qt resource system
- the existing backend and streaming code

This is still a large rewrite, but it keeps the native integration points intact.

### 2. Different UI technology entirely

This is possible, but it is no longer just a GUI rewrite.

The biggest current blocker is that `Session` is wired to Qt Quick via:

```cpp
Q_INVOKABLE bool initialize(QQuickWindow* qtWindow);
```

That means a non-Qt-Quick frontend would require refactoring the streaming startup boundary, not just replacing QML files.

A totally different UI stack would also need to reimplement:

- startup flow selection between GUI and CLI
- host polling tied to window focus/visibility
- gamepad-oriented focus/navigation behavior
- settings editing against `StreamingPreferences`
- app/host browsing against `ComputerModel` and `AppModel`
- stream/quit/pair segue behavior and error dialogs
- packaging and deployment assumptions in the Qt-based build scripts

## If you still want a totally different GUI, migrate in this order

1. Keep the backend/service layer (`ComputerManager`, `StreamingPreferences`, `Session`, `NvHTTP`, models) unchanged
2. Rebuild only the shell and page structure first
3. Reconnect host list and app list views to the existing models
4. Recreate stream/pair/quit flows
5. Only then decide whether the `QQuickWindow` dependency in `Session` should be abstracted away

Do **not** start by changing the streaming/session layer first unless the goal is a full platform migration.

## Practical recommendation

If you want a GUI that is easier to maintain, aim for:

- smaller QML components
- clearer page ownership
- less inline page logic
- continued use of the current C++ services and QML resource loading

That gets most of the benefit without turning the project into a frontend-framework port.
