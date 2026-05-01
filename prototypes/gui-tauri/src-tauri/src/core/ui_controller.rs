use super::events::{BridgeEvent, BridgeEventKind, ControllerAction};
use crate::logger;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::Emitter;

const BRIDGE_EVENT: &str = "moonlight-bridge-event";
const POLL_INTERVAL: Duration = Duration::from_millis(8);
const DISABLED_POLL_INTERVAL: Duration = Duration::from_millis(50);
const AXIS_THRESHOLD: i16 = 16_000;
const AXIS_INITIAL_REPEAT: Duration = Duration::from_millis(280);
const AXIS_REPEAT: Duration = Duration::from_millis(120);

static UI_CONTROLLER_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_ui_controller_enabled(enabled: bool) {
    UI_CONTROLLER_ENABLED.store(enabled, Ordering::Release);
}

pub fn start_ui_controller(app_handle: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("ui-controller-navigation".into())
        .spawn(move || run_ui_controller(app_handle))
        .unwrap_or_else(|error| {
            logger::log(format!(
                "failed to start SDL3 UI controller navigation thread: {error}"
            ));
        });
}

fn run_ui_controller(app_handle: tauri::AppHandle) {
    let sdl = match sdl3::init() {
        Ok(sdl) => sdl,
        Err(error) => {
            logger::log(format!(
                "SDL3 UI controller navigation unavailable: init failed: {error}"
            ));
            return;
        }
    };
    let gamepad = match sdl.gamepad() {
        Ok(gamepad) => gamepad,
        Err(error) => {
            logger::log(format!(
                "SDL3 UI controller navigation unavailable: gamepad subsystem failed: {error}"
            ));
            return;
        }
    };
    load_sdl3_game_controller_mappings(&gamepad);
    let mut event_pump = match sdl.event_pump() {
        Ok(event_pump) => event_pump,
        Err(error) => {
            logger::log(format!(
                "SDL3 UI controller navigation unavailable: event pump failed: {error}"
            ));
            return;
        }
    };
    let mut state = UiControllerState::new(gamepad);
    let mut was_enabled = false;
    logger::log("SDL3 UI controller navigation thread started");

    loop {
        let enabled = UI_CONTROLLER_ENABLED.load(Ordering::Acquire);
        if !enabled {
            if was_enabled {
                state.close_controllers();
                logger::log("SDL3 UI controller navigation paused for active stream");
            }
            was_enabled = false;
            std::thread::sleep(DISABLED_POLL_INTERVAL);
            continue;
        }
        if !was_enabled {
            state.enumerate_controllers();
            logger::log("SDL3 UI controller navigation active");
        }
        was_enabled = true;

        for event in event_pump.poll_iter() {
            state.handle_event(event, &app_handle);
        }
        state.emit_axis_repeat(&app_handle);
        std::thread::sleep(POLL_INTERVAL);
    }
}

struct UiControllerState {
    subsystem: sdl3::GamepadSubsystem,
    controllers: HashMap<u32, sdl3::gamepad::Gamepad>,
    left_x: i16,
    left_y: i16,
    axis_action: Option<ControllerAction>,
    next_axis_action_at: Instant,
}

impl UiControllerState {
    fn new(subsystem: sdl3::GamepadSubsystem) -> Self {
        Self {
            subsystem,
            controllers: HashMap::new(),
            left_x: 0,
            left_y: 0,
            axis_action: None,
            next_axis_action_at: Instant::now(),
        }
    }

    fn enumerate_controllers(&mut self) {
        match self.subsystem.gamepads() {
            Ok(gamepads) => {
                logger::log(format!(
                    "SDL3 UI controller navigation enumerated {} gamepad(s)",
                    gamepads.len()
                ));
                for gamepad_id in gamepads {
                    self.open_controller(gamepad_id);
                }
            }
            Err(error) => logger::log(format!(
                "SDL3 UI controller navigation failed to enumerate gamepads: {error}"
            )),
        }
    }

    fn open_controller(&mut self, gamepad_id: sdl3::joystick::JoystickId) {
        let key = u32::from(gamepad_id);
        if self.controllers.contains_key(&key) {
            return;
        }
        match self.subsystem.open(gamepad_id) {
            Ok(gamepad) => {
                let name = gamepad.name().unwrap_or_else(|| "Unknown gamepad".into());
                self.controllers.insert(key, gamepad);
                logger::log(format!(
                    "SDL3 UI controller navigation opened gamepad id={key}; name={name}"
                ));
            }
            Err(error) => logger::log(format!(
                "SDL3 UI controller navigation failed to open gamepad id={key}: {error}"
            )),
        }
    }

    fn close_controllers(&mut self) {
        self.controllers.clear();
        self.left_x = 0;
        self.left_y = 0;
        self.axis_action = None;
    }

    fn handle_event(&mut self, event: sdl3::event::Event, app_handle: &tauri::AppHandle) {
        match event {
            sdl3::event::Event::ControllerDeviceAdded { which, .. } => {
                self.open_controller(sdl3::sys::joystick::SDL_JoystickID(which));
            }
            sdl3::event::Event::ControllerDeviceRemoved { which, .. } => {
                self.controllers.remove(&which);
                logger::log(format!(
                    "SDL3 UI controller navigation removed gamepad id={which}"
                ));
            }
            sdl3::event::Event::ControllerButtonDown { button, repeat, .. } => {
                if !repeat {
                    if let Some(action) = button_action(button) {
                        emit_controller_action(app_handle, action);
                    }
                }
            }
            sdl3::event::Event::ControllerAxisMotion { axis, value, .. } => {
                self.update_axis(axis, value, app_handle);
            }
            _ => {}
        }
    }

    fn update_axis(
        &mut self,
        axis: sdl3::gamepad::Axis,
        value: i16,
        app_handle: &tauri::AppHandle,
    ) {
        match axis {
            sdl3::gamepad::Axis::LeftX => self.left_x = value,
            sdl3::gamepad::Axis::LeftY => self.left_y = value,
            _ => return,
        }
        let next_action = axis_action(self.left_x, self.left_y);
        if next_action != self.axis_action {
            self.axis_action = next_action.clone();
            self.next_axis_action_at = Instant::now() + AXIS_INITIAL_REPEAT;
            if let Some(action) = next_action {
                emit_controller_action(app_handle, action);
            }
        }
    }

    fn emit_axis_repeat(&mut self, app_handle: &tauri::AppHandle) {
        let Some(action) = self.axis_action.clone() else {
            return;
        };
        if Instant::now() < self.next_axis_action_at {
            return;
        }
        emit_controller_action(app_handle, action);
        self.next_axis_action_at = Instant::now() + AXIS_REPEAT;
    }
}

fn button_action(button: sdl3::gamepad::Button) -> Option<ControllerAction> {
    match button {
        sdl3::gamepad::Button::South | sdl3::gamepad::Button::Start => {
            Some(ControllerAction::Accept)
        }
        sdl3::gamepad::Button::East | sdl3::gamepad::Button::Back => Some(ControllerAction::Back),
        sdl3::gamepad::Button::West => Some(ControllerAction::ContextMenu),
        sdl3::gamepad::Button::North | sdl3::gamepad::Button::Guide => {
            Some(ControllerAction::Settings)
        }
        sdl3::gamepad::Button::LeftShoulder => Some(ControllerAction::PreviousControl),
        sdl3::gamepad::Button::RightShoulder => Some(ControllerAction::NextControl),
        sdl3::gamepad::Button::DPadUp => Some(ControllerAction::Up),
        sdl3::gamepad::Button::DPadDown => Some(ControllerAction::Down),
        sdl3::gamepad::Button::DPadLeft => Some(ControllerAction::Left),
        sdl3::gamepad::Button::DPadRight => Some(ControllerAction::Right),
        _ => None,
    }
}

fn axis_action(left_x: i16, left_y: i16) -> Option<ControllerAction> {
    if left_x.unsigned_abs() > left_y.unsigned_abs() {
        if left_x > AXIS_THRESHOLD {
            Some(ControllerAction::Right)
        } else if left_x < -AXIS_THRESHOLD {
            Some(ControllerAction::Left)
        } else {
            None
        }
    } else if left_y > AXIS_THRESHOLD {
        Some(ControllerAction::Down)
    } else if left_y < -AXIS_THRESHOLD {
        Some(ControllerAction::Up)
    } else {
        None
    }
}

fn emit_controller_action(app_handle: &tauri::AppHandle, action: ControllerAction) {
    let message = format!("Controller action: {action:?}");
    if let Err(error) = app_handle.emit(
        BRIDGE_EVENT,
        BridgeEvent {
            kind: BridgeEventKind::ControllerAction,
            message,
            host_id: None,
            app_id: None,
            controller_action: Some(action),
            update_version: None,
            update_url: None,
        },
    ) {
        logger::log(format!(
            "SDL3 UI controller navigation failed to emit action: {error}"
        ));
    }
}

fn load_sdl3_game_controller_mappings(gamepad: &sdl3::GamepadSubsystem) {
    for path in sdl3_controller_mapping_candidates() {
        if path.is_file() {
            match gamepad.load_mappings(&path) {
                Ok(count) => logger::log(format!(
                    "loaded {count} SDL3 UI controller mappings from {}",
                    path.display()
                )),
                Err(error) => logger::log(format!(
                    "failed to load SDL3 UI controller mappings from {}: {error}",
                    path.display()
                )),
            }
            return;
        }
    }
    logger::log("SDL3 UI controller mapping database was not found");
}

fn sdl3_controller_mapping_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(
                exe_dir
                    .join("SDL_GameControllerDB")
                    .join("gamecontrollerdb.txt"),
            );
            candidates.push(exe_dir.join("gamecontrollerdb.txt"));
        }
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(
            current_dir
                .join("SDL_GameControllerDB")
                .join("gamecontrollerdb.txt"),
        );
        candidates.push(current_dir.join("gamecontrollerdb.txt"));
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::{axis_action, button_action};
    use crate::core::events::ControllerAction;

    #[test]
    fn maps_common_controller_buttons_to_ui_actions() {
        assert_eq!(
            Some(ControllerAction::Accept),
            button_action(sdl3::gamepad::Button::South)
        );
        assert_eq!(
            Some(ControllerAction::Back),
            button_action(sdl3::gamepad::Button::East)
        );
        assert_eq!(
            Some(ControllerAction::ContextMenu),
            button_action(sdl3::gamepad::Button::West)
        );
        assert_eq!(
            Some(ControllerAction::Settings),
            button_action(sdl3::gamepad::Button::North)
        );
        assert_eq!(
            Some(ControllerAction::NextControl),
            button_action(sdl3::gamepad::Button::RightShoulder)
        );
    }

    #[test]
    fn maps_left_stick_to_directional_ui_actions() {
        assert_eq!(Some(ControllerAction::Left), axis_action(-20_000, 0));
        assert_eq!(Some(ControllerAction::Right), axis_action(20_000, 0));
        assert_eq!(Some(ControllerAction::Up), axis_action(0, -20_000));
        assert_eq!(Some(ControllerAction::Down), axis_action(0, 20_000));
        assert_eq!(None, axis_action(8_000, 0));
    }
}
