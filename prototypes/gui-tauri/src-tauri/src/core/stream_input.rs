#![allow(dead_code)]

use super::error::CoreError;
use super::gamestream_sys;
use std::os::raw::{c_char, c_int};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    X1,
    X2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonAction {
    Press,
    Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyAction {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControllerType {
    Unknown,
    Xbox,
    PlayStation,
    Nintendo,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControllerState {
    pub controller_number: u8,
    pub active_gamepad_mask: u16,
    pub button_flags: i32,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub right_stick_x: i16,
    pub right_stick_y: i16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControllerCapabilities {
    pub analog_triggers: bool,
    pub rumble: bool,
    pub trigger_rumble: bool,
    pub touchpad: bool,
    pub accelerometer: bool,
    pub gyroscope: bool,
    pub battery_state: bool,
    pub rgb_led: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StreamInputSender;

impl StreamInputSender {
    pub fn is_linked(&self) -> bool {
        cfg!(moonlight_common_c_linked)
    }

    pub fn send_mouse_move(&self, delta_x: i16, delta_y: i16) -> Result<(), CoreError> {
        send_input("mouse move", || unsafe {
            gamestream_sys::LiSendMouseMoveEvent(delta_x, delta_y)
        })
    }

    pub fn send_mouse_position(
        &self,
        x: i16,
        y: i16,
        reference_width: i16,
        reference_height: i16,
    ) -> Result<(), CoreError> {
        send_input("mouse position", || unsafe {
            gamestream_sys::LiSendMousePositionEvent(x, y, reference_width, reference_height)
        })
    }

    pub fn send_mouse_button(
        &self,
        action: ButtonAction,
        button: MouseButton,
    ) -> Result<(), CoreError> {
        send_input("mouse button", || unsafe {
            gamestream_sys::LiSendMouseButtonEvent(action.as_raw(), button.as_raw())
        })
    }

    pub fn send_keyboard(
        &self,
        key_code: i16,
        action: KeyAction,
        modifiers: KeyModifiers,
        non_normalized: bool,
    ) -> Result<(), CoreError> {
        let flags = if non_normalized {
            gamestream_sys::SS_KBE_FLAG_NON_NORMALIZED
        } else {
            0
        };
        send_input("keyboard", || unsafe {
            gamestream_sys::LiSendKeyboardEvent2(
                key_code,
                action.as_raw(),
                modifiers.as_raw(),
                flags,
            )
        })
    }

    pub fn send_utf8_text(&self, text: &str) -> Result<(), CoreError> {
        let length = text.len().try_into().map_err(|_| {
            CoreError::Validation("Text input is too large to send to the stream.".into())
        })?;
        send_input("UTF-8 text", || unsafe {
            gamestream_sys::LiSendUtf8TextEvent(text.as_ptr().cast(), length)
        })
    }

    pub fn send_controller(&self, state: ControllerState) -> Result<(), CoreError> {
        if state.controller_number == 0 && state.active_gamepad_mask <= 1 {
            return send_input("controller", || unsafe {
                gamestream_sys::LiSendControllerEvent(
                    state.button_flags,
                    state.left_trigger,
                    state.right_trigger,
                    state.left_stick_x,
                    state.left_stick_y,
                    state.right_stick_x,
                    state.right_stick_y,
                )
            });
        }

        send_input("multi-controller", || unsafe {
            gamestream_sys::LiSendMultiControllerEvent(
                state.controller_number.into(),
                state.active_gamepad_mask as i16,
                state.button_flags,
                state.left_trigger,
                state.right_trigger,
                state.left_stick_x,
                state.left_stick_y,
                state.right_stick_x,
                state.right_stick_y,
            )
        })
    }

    pub fn send_controller_arrival(
        &self,
        controller_number: u8,
        active_gamepad_mask: u16,
        controller_type: ControllerType,
        supported_button_flags: u32,
        capabilities: ControllerCapabilities,
    ) -> Result<(), CoreError> {
        send_input("controller arrival", || unsafe {
            gamestream_sys::LiSendControllerArrivalEvent(
                controller_number,
                active_gamepad_mask,
                controller_type.as_raw(),
                supported_button_flags,
                capabilities.as_raw(),
            )
        })
    }

    pub fn send_scroll(&self, scroll_clicks: i8) -> Result<(), CoreError> {
        send_input("vertical scroll", || unsafe {
            gamestream_sys::LiSendScrollEvent(scroll_clicks)
        })
    }

    pub fn send_high_res_scroll(&self, scroll_amount: i16) -> Result<(), CoreError> {
        send_input("high resolution vertical scroll", || unsafe {
            gamestream_sys::LiSendHighResScrollEvent(scroll_amount)
        })
    }

    pub fn send_horizontal_scroll(&self, scroll_clicks: i8) -> Result<(), CoreError> {
        send_input("horizontal scroll", || unsafe {
            gamestream_sys::LiSendHScrollEvent(scroll_clicks)
        })
    }

    pub fn send_high_res_horizontal_scroll(&self, scroll_amount: i16) -> Result<(), CoreError> {
        send_input("high resolution horizontal scroll", || unsafe {
            gamestream_sys::LiSendHighResHScrollEvent(scroll_amount)
        })
    }
}

impl MouseButton {
    fn as_raw(self) -> c_int {
        match self {
            Self::Left => gamestream_sys::BUTTON_LEFT,
            Self::Middle => gamestream_sys::BUTTON_MIDDLE,
            Self::Right => gamestream_sys::BUTTON_RIGHT,
            Self::X1 => gamestream_sys::BUTTON_X1,
            Self::X2 => gamestream_sys::BUTTON_X2,
        }
    }
}

impl ButtonAction {
    fn as_raw(self) -> c_char {
        match self {
            Self::Press => gamestream_sys::BUTTON_ACTION_PRESS,
            Self::Release => gamestream_sys::BUTTON_ACTION_RELEASE,
        }
    }
}

impl KeyAction {
    fn as_raw(self) -> c_char {
        match self {
            Self::Down => gamestream_sys::KEY_ACTION_DOWN,
            Self::Up => gamestream_sys::KEY_ACTION_UP,
        }
    }
}

impl KeyModifiers {
    fn as_raw(self) -> c_char {
        let mut raw = 0;
        if self.shift {
            raw |= gamestream_sys::MODIFIER_SHIFT;
        }
        if self.ctrl {
            raw |= gamestream_sys::MODIFIER_CTRL;
        }
        if self.alt {
            raw |= gamestream_sys::MODIFIER_ALT;
        }
        if self.meta {
            raw |= gamestream_sys::MODIFIER_META;
        }
        raw
    }
}

impl ControllerType {
    fn as_raw(self) -> u8 {
        match self {
            Self::Unknown => gamestream_sys::LI_CTYPE_UNKNOWN,
            Self::Xbox => gamestream_sys::LI_CTYPE_XBOX,
            Self::PlayStation => gamestream_sys::LI_CTYPE_PS,
            Self::Nintendo => gamestream_sys::LI_CTYPE_NINTENDO,
        }
    }
}

impl ControllerCapabilities {
    fn as_raw(self) -> u16 {
        let mut raw = 0;
        if self.analog_triggers {
            raw |= gamestream_sys::LI_CCAP_ANALOG_TRIGGERS;
        }
        if self.rumble {
            raw |= gamestream_sys::LI_CCAP_RUMBLE;
        }
        if self.trigger_rumble {
            raw |= gamestream_sys::LI_CCAP_TRIGGER_RUMBLE;
        }
        if self.touchpad {
            raw |= gamestream_sys::LI_CCAP_TOUCHPAD;
        }
        if self.accelerometer {
            raw |= gamestream_sys::LI_CCAP_ACCEL;
        }
        if self.gyroscope {
            raw |= gamestream_sys::LI_CCAP_GYRO;
        }
        if self.battery_state {
            raw |= gamestream_sys::LI_CCAP_BATTERY_STATE;
        }
        if self.rgb_led {
            raw |= gamestream_sys::LI_CCAP_RGB_LED;
        }
        raw
    }
}

fn send_input(label: &str, send: impl FnOnce() -> i32) -> Result<(), CoreError> {
    if !cfg!(moonlight_common_c_linked) {
        return Err(CoreError::Backend(format!(
            "C GameStream library is not linked. Cannot send {label} input."
        )));
    }

    let result = send();
    if result == 0 {
        Ok(())
    } else {
        Err(CoreError::Backend(format!(
            "GameStream rejected {label} input with code {result}."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ButtonAction, ControllerCapabilities, ControllerState, ControllerType, KeyAction,
        KeyModifiers, MouseButton, StreamInputSender,
    };
    use crate::core::gamestream_sys;

    #[test]
    fn mouse_and_keyboard_values_match_limelight() {
        assert_eq!(gamestream_sys::BUTTON_LEFT, MouseButton::Left.as_raw());
        assert_eq!(
            gamestream_sys::BUTTON_ACTION_PRESS,
            ButtonAction::Press.as_raw()
        );
        assert_eq!(gamestream_sys::KEY_ACTION_DOWN, KeyAction::Down.as_raw());

        let modifiers = KeyModifiers {
            shift: true,
            ctrl: true,
            alt: false,
            meta: true,
        };

        assert_eq!(
            gamestream_sys::MODIFIER_SHIFT
                | gamestream_sys::MODIFIER_CTRL
                | gamestream_sys::MODIFIER_META,
            modifiers.as_raw()
        );
    }

    #[test]
    fn controller_capabilities_match_limelight_flags() {
        let capabilities = ControllerCapabilities {
            analog_triggers: true,
            rumble: true,
            trigger_rumble: false,
            touchpad: true,
            accelerometer: false,
            gyroscope: true,
            battery_state: true,
            rgb_led: false,
        };

        assert_eq!(
            gamestream_sys::LI_CCAP_ANALOG_TRIGGERS
                | gamestream_sys::LI_CCAP_RUMBLE
                | gamestream_sys::LI_CCAP_TOUCHPAD
                | gamestream_sys::LI_CCAP_GYRO
                | gamestream_sys::LI_CCAP_BATTERY_STATE,
            capabilities.as_raw()
        );
        assert_eq!(
            gamestream_sys::LI_CTYPE_PS,
            ControllerType::PlayStation.as_raw()
        );
    }

    #[test]
    fn controller_state_defaults_to_first_controller() {
        let state = ControllerState {
            active_gamepad_mask: 1,
            button_flags: gamestream_sys::A_FLAG | gamestream_sys::B_FLAG,
            left_trigger: 255,
            ..ControllerState::default()
        };

        assert_eq!(0, state.controller_number);
        assert_eq!(1, state.active_gamepad_mask);
        assert_eq!(255, state.left_trigger);
    }

    #[cfg(not(moonlight_common_c_linked))]
    #[test]
    fn sender_reports_unlinked_library() {
        let error = StreamInputSender
            .send_mouse_move(1, 2)
            .unwrap_err()
            .to_string();

        assert_eq!(
            "C GameStream library is not linked. Cannot send mouse move input.",
            error
        );
    }
}
