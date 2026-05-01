use super::stream_window::StreamWindowDescriptor;
use super::types::{
    AppEntry, BackendInfo, CommandStatus, HostDetails, HostEntry, NetworkTestResult,
    PairingChallenge, StreamingSettings, SystemInfo,
};

pub trait MoonlightCore: Send {
    fn backend_info(&self) -> BackendInfo;

    fn emits_native_events(&self) -> bool {
        false
    }

    fn list_hosts(&mut self) -> Result<Vec<HostEntry>, String>;
    fn add_host(&mut self, address: String) -> Result<(CommandStatus, String), String>;
    fn pair_host(&mut self, host_id: &str) -> Result<PairingChallenge, String>;
    fn wake_host(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn rename_host(&mut self, host_id: &str, name: String) -> Result<CommandStatus, String>;
    fn delete_host(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn host_details(&mut self, host_id: &str) -> Result<HostDetails, String>;
    fn test_network(&mut self, host_id: &str) -> Result<NetworkTestResult, String>;
    fn list_apps(&mut self, host_id: &str, show_hidden: bool) -> Result<Vec<AppEntry>, String>;
    fn launch_app(&mut self, host_id: &str, app_id: &str) -> Result<CommandStatus, String>;
    fn resume_session(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn quit_running_app(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn set_app_hidden(
        &mut self,
        host_id: &str,
        app_id: &str,
        hidden: bool,
    ) -> Result<CommandStatus, String>;
    fn set_app_direct_launch(
        &mut self,
        host_id: &str,
        app_id: &str,
        direct_launch: bool,
    ) -> Result<CommandStatus, String>;
    fn load_settings(&mut self) -> Result<StreamingSettings, String>;
    fn save_settings(&mut self, settings: StreamingSettings) -> Result<CommandStatus, String>;
    fn default_bitrate(
        &mut self,
        width: u32,
        height: u32,
        fps: u32,
        yuv444: bool,
    ) -> Result<u32, String>;
    fn system_info(&mut self) -> Result<SystemInfo, String>;
    fn open_url(&mut self, url: &str) -> Result<CommandStatus, String>;

    fn active_stream_window(&mut self) -> Result<Option<StreamWindowDescriptor>, String> {
        Ok(None)
    }

    fn stream_mouse_move(&mut self, _delta_x: i16, _delta_y: i16) -> Result<CommandStatus, String> {
        Err("Stream input is not available for this backend.".into())
    }

    fn stream_mouse_position(
        &mut self,
        _x: i16,
        _y: i16,
        _reference_width: i16,
        _reference_height: i16,
    ) -> Result<CommandStatus, String> {
        Err("Stream input is not available for this backend.".into())
    }

    fn stream_mouse_button(
        &mut self,
        _button: String,
        _pressed: bool,
    ) -> Result<CommandStatus, String> {
        Err("Stream input is not available for this backend.".into())
    }

    #[allow(clippy::too_many_arguments)]
    fn stream_keyboard(
        &mut self,
        _key_code: i16,
        _pressed: bool,
        _shift: bool,
        _ctrl: bool,
        _alt: bool,
        _meta: bool,
        _non_normalized: bool,
    ) -> Result<CommandStatus, String> {
        Err("Stream input is not available for this backend.".into())
    }

    fn stream_text(&mut self, _text: String) -> Result<CommandStatus, String> {
        Err("Stream input is not available for this backend.".into())
    }

    fn stream_scroll(&mut self, _delta_x: i16, _delta_y: i16) -> Result<CommandStatus, String> {
        Err("Stream input is not available for this backend.".into())
    }

    #[allow(clippy::too_many_arguments)]
    fn stream_controller(
        &mut self,
        _controller_number: u8,
        _active_gamepad_mask: u16,
        _button_flags: i32,
        _left_trigger: u8,
        _right_trigger: u8,
        _left_stick_x: i16,
        _left_stick_y: i16,
        _right_stick_x: i16,
        _right_stick_y: i16,
    ) -> Result<CommandStatus, String> {
        Err("Stream input is not available for this backend.".into())
    }
}
