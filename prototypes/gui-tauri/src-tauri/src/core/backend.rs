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
}
