use crate::backend::{
    AppEntry, BackendInfo, BridgeEvent, CommandStatus, HostDetails, HostEntry, MoonlightBackend,
    NetworkTestResult, PairingChallenge, StreamingSettings, SystemInfo,
};
use crate::logger;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

const BACKEND_MODE_ENV: &str = "MOONLIGHT_TAURI_BACKEND";
const HELPER_PATH_ENV: &str = "MOONLIGHT_TAURI_HELPER";
const IPC_TIMEOUT_ENV: &str = "MOONLIGHT_TAURI_IPC_TIMEOUT_SECS";
const BRIDGE_EVENT: &str = "moonlight-bridge-event";

type PendingResponses = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<serde_json::Value, String>>>>>;

pub fn ipc_backend_requested() -> bool {
    env::var(BACKEND_MODE_ENV)
        .map(|value| value.eq_ignore_ascii_case("ipc"))
        .unwrap_or(false)
}

pub fn mock_backend_requested() -> bool {
    env::var(BACKEND_MODE_ENV)
        .map(|value| value.eq_ignore_ascii_case("mock"))
        .unwrap_or(false)
}

pub struct IpcBackend {
    process: Child,
    helper_path: String,
    stdin: Arc<Mutex<ChildStdin>>,
    pending_responses: PendingResponses,
    next_request_id: u64,
}

impl IpcBackend {
    pub fn from_environment(app_handle: tauri::AppHandle) -> Result<Self, String> {
        let helper_path = env::var(HELPER_PATH_ENV).map_err(|_| {
            format!("{HELPER_PATH_ENV} must point to the native helper executable.")
        })?;
        logger::log(format!("ipc backend requested; helper_path={helper_path}"));
        Self::from_helper_path(helper_path, app_handle)
    }

    pub fn staged_helper_path() -> Option<String> {
        let helper_name = if cfg!(target_os = "windows") {
            "Moonlight.exe"
        } else {
            "Moonlight"
        };
        let helper_path: PathBuf = env::current_exe()
            .ok()?
            .parent()?
            .join("native")
            .join(helper_name);

        helper_path
            .is_file()
            .then(|| helper_path.to_string_lossy().into_owned())
    }

    pub fn from_helper_path(
        helper_path: String,
        app_handle: tauri::AppHandle,
    ) -> Result<Self, String> {
        logger::log(format!(
            "creating IPC backend with helper_path={helper_path}"
        ));
        Self::spawn(helper_path, app_handle)
    }

    fn spawn(helper_path: String, app_handle: tauri::AppHandle) -> Result<Self, String> {
        let mut command = Command::new(&helper_path);
        command
            .arg("--tauri-bridge-helper")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if logger::enabled() {
            command.stderr(Stdio::piped());
        } else {
            command.stderr(Stdio::inherit());
        }

        let mut process = command
            .spawn()
            .map_err(|error| format!("Failed to start native helper '{helper_path}': {error}"))?;
        logger::log(format!("native helper spawned; pid={}", process.id()));

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| "Failed to open native helper stdin.".to_string())?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| "Failed to open native helper stdout.".to_string())?;
        if let Some(stderr) = process.stderr.take() {
            start_stderr_thread(stderr);
        }
        let pending_responses = Arc::new(Mutex::new(HashMap::new()));
        start_reader_thread(stdout, pending_responses.clone(), app_handle);

        Ok(Self {
            process,
            helper_path,
            stdin: Arc::new(Mutex::new(stdin)),
            pending_responses,
            next_request_id: 1,
        })
    }

    fn request<R>(&mut self, command: IpcCommand) -> Result<R, String>
    where
        R: DeserializeOwned,
    {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        let command_name = command.name();
        logger::log(format!(
            "ipc request {request_id} begin; command={command_name}"
        ));

        let request = serde_json::to_vec(&IpcRequest {
            id: request_id,
            command,
        })
        .map_err(|error| format!("Failed to serialize native helper request: {error}"))?;

        let (sender, receiver) = mpsc::channel();
        self.pending_responses
            .lock()
            .map_err(|error| error.to_string())?
            .insert(request_id, sender);

        let write_result = self
            .stdin
            .lock()
            .map_err(|error| error.to_string())
            .and_then(|mut stdin| {
                stdin
                    .write_all(&request)
                    .and_then(|_| stdin.write_all(b"\n"))
                    .and_then(|_| stdin.flush())
                    .map_err(|error| error.to_string())
            });

        if let Err(error) = write_result {
            self.pending_responses
                .lock()
                .map_err(|lock_error| lock_error.to_string())?
                .remove(&request_id);
            logger::log(format!(
                "ipc request {request_id} send failed; command={command_name}; error={error}"
            ));
            return Err(format!("Failed to send native helper request: {error}"));
        }

        let frame = receiver.recv_timeout(ipc_timeout()).map_err(|error| {
            self.pending_responses
                .lock()
                .map(|mut pending| pending.remove(&request_id))
                .ok();
            format!("Native helper response timed out or channel closed: {error}")
        })??;
        logger::log(format!(
            "ipc request {request_id} received response; command={command_name}"
        ));
        let response: IpcResponse<R> = serde_json::from_value(frame)
            .map_err(|error| format!("Failed to parse native helper response: {error}"))?;
        if response.id != request_id {
            return Err(format!(
                "Native helper response ID mismatch: expected {request_id}, got {}.",
                response.id
            ));
        }

        let result = response.into_result();
        match &result {
            Ok(_) => logger::log(format!(
                "ipc request {request_id} completed; command={command_name}"
            )),
            Err(error) => logger::log(format!(
                "ipc request {request_id} failed; command={command_name}; error={error}"
            )),
        }
        result
    }
}

fn ipc_timeout() -> Duration {
    env::var(IPC_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(15))
}

fn start_reader_thread(
    stdout: ChildStdout,
    pending_responses: PendingResponses,
    app_handle: tauri::AppHandle,
) {
    thread::spawn(move || {
        logger::log("native helper stdout reader started");
        let mut stdout = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match stdout.read_line(&mut line) {
                Ok(0) => {
                    logger::log("native helper stdout closed");
                    fail_pending_responses(&pending_responses, "Native helper stdout closed.");
                    break;
                }
                Ok(_) => handle_helper_frame(&line, &pending_responses, &app_handle),
                Err(error) => {
                    logger::log(format!("native helper stdout read failed; error={error}"));
                    fail_pending_responses(
                        &pending_responses,
                        &format!("Failed to read native helper response: {error}"),
                    );
                    break;
                }
            }
        }
    });
}

fn start_stderr_thread(stderr: ChildStderr) {
    thread::spawn(move || {
        logger::log("native helper stderr reader started");
        let stderr = BufReader::new(stderr);
        for line in stderr.lines() {
            match line {
                Ok(line) => logger::log(format!("native helper stderr: {line}")),
                Err(error) => {
                    logger::log(format!("native helper stderr read failed; error={error}"));
                    break;
                }
            }
        }
    });
}

fn handle_helper_frame(
    line: &str,
    pending_responses: &PendingResponses,
    app_handle: &tauri::AppHandle,
) {
    let frame: serde_json::Value = match serde_json::from_str(line) {
        Ok(frame) => frame,
        Err(error) => {
            logger::log(format!(
                "failed to parse native helper frame; error={error}; line={line:?}"
            ));
            eprintln!("Failed to parse native helper frame: {error}");
            return;
        }
    };

    if let Some(event) = frame.get("event") {
        match serde_json::from_value::<BridgeEvent>(event.clone()) {
            Ok(event) => {
                logger::log(format!(
                    "native helper event; kind={:?}; message={}",
                    event.kind, event.message
                ));
                if let Err(error) = app_handle.emit(BRIDGE_EVENT, event) {
                    logger::log(format!("failed to emit native helper event; error={error}"));
                    eprintln!("Failed to emit native helper event: {error}");
                }
            }
            Err(error) => {
                logger::log(format!(
                    "failed to parse native helper event; error={error}"
                ));
                eprintln!("Failed to parse native helper event: {error}");
            }
        }
        return;
    }

    let Some(response_id) = frame.get("id").and_then(|value| value.as_u64()) else {
        logger::log("native helper response missing numeric ID");
        eprintln!("Native helper response was missing a numeric ID.");
        return;
    };

    let sender = pending_responses
        .lock()
        .map(|mut pending| pending.remove(&response_id))
        .unwrap_or(None);
    if let Some(sender) = sender {
        let _ = sender.send(Ok(frame));
    } else {
        logger::log(format!(
            "native helper response had no pending request; id={response_id}"
        ));
        eprintln!("Native helper response had no pending request: {response_id}");
    }
}

fn fail_pending_responses(pending_responses: &PendingResponses, message: &str) {
    let senders = pending_responses
        .lock()
        .map(|mut pending| {
            pending
                .drain()
                .map(|(_, sender)| sender)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let count = senders.len();
    for sender in senders {
        let _ = sender.send(Err(message.to_string()));
    }
    logger::log(format!(
        "failed {count} pending IPC responses; message={message}"
    ));
}

impl Drop for IpcBackend {
    fn drop(&mut self) {
        logger::log(format!("stopping native helper; pid={}", self.process.id()));
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl MoonlightBackend for IpcBackend {
    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            mode: "ipc".into(),
            helper_path: Some(self.helper_path.clone()),
        }
    }

    fn emits_native_events(&self) -> bool {
        true
    }

    fn list_hosts(&mut self) -> Result<Vec<HostEntry>, String> {
        self.request(IpcCommand::ListHosts)
    }

    fn add_host(&mut self, address: String) -> Result<(CommandStatus, String), String> {
        let result: AddHostResult = self.request(IpcCommand::AddHost { address })?;
        Ok((result.status, result.host_id))
    }

    fn pair_host(&mut self, host_id: &str) -> Result<PairingChallenge, String> {
        self.request(IpcCommand::PairHost {
            host_id: host_id.into(),
        })
    }

    fn wake_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.request(IpcCommand::WakeHost {
            host_id: host_id.into(),
        })
    }

    fn rename_host(&mut self, host_id: &str, name: String) -> Result<CommandStatus, String> {
        self.request(IpcCommand::RenameHost {
            host_id: host_id.into(),
            name,
        })
    }

    fn delete_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.request(IpcCommand::DeleteHost {
            host_id: host_id.into(),
        })
    }

    fn host_details(&mut self, host_id: &str) -> Result<HostDetails, String> {
        self.request(IpcCommand::HostDetails {
            host_id: host_id.into(),
        })
    }

    fn test_network(&mut self, host_id: &str) -> Result<NetworkTestResult, String> {
        self.request(IpcCommand::TestNetwork {
            host_id: host_id.into(),
        })
    }

    fn list_apps(&mut self, host_id: &str, show_hidden: bool) -> Result<Vec<AppEntry>, String> {
        self.request(IpcCommand::ListApps {
            host_id: host_id.into(),
            show_hidden,
        })
    }

    fn launch_app(&mut self, host_id: &str, app_id: &str) -> Result<CommandStatus, String> {
        self.request(IpcCommand::LaunchApp {
            host_id: host_id.into(),
            app_id: app_id.into(),
        })
    }

    fn resume_session(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.request(IpcCommand::ResumeSession {
            host_id: host_id.into(),
        })
    }

    fn quit_running_app(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.request(IpcCommand::QuitRunningApp {
            host_id: host_id.into(),
        })
    }

    fn set_app_hidden(
        &mut self,
        host_id: &str,
        app_id: &str,
        hidden: bool,
    ) -> Result<CommandStatus, String> {
        self.request(IpcCommand::SetAppHidden {
            host_id: host_id.into(),
            app_id: app_id.into(),
            hidden,
        })
    }

    fn set_app_direct_launch(
        &mut self,
        host_id: &str,
        app_id: &str,
        direct_launch: bool,
    ) -> Result<CommandStatus, String> {
        self.request(IpcCommand::SetAppDirectLaunch {
            host_id: host_id.into(),
            app_id: app_id.into(),
            direct_launch,
        })
    }

    fn load_settings(&mut self) -> Result<StreamingSettings, String> {
        self.request(IpcCommand::LoadSettings)
    }

    fn save_settings(&mut self, settings: StreamingSettings) -> Result<CommandStatus, String> {
        self.request(IpcCommand::SaveSettings { settings })
    }

    fn default_bitrate(
        &mut self,
        width: u32,
        height: u32,
        fps: u32,
        yuv444: bool,
    ) -> Result<u32, String> {
        self.request(IpcCommand::DefaultBitrate {
            width,
            height,
            fps,
            yuv444,
        })
    }

    fn system_info(&mut self) -> Result<SystemInfo, String> {
        self.request(IpcCommand::SystemInfo)
    }

    fn open_url(&mut self, url: &str) -> Result<CommandStatus, String> {
        self.request(IpcCommand::OpenUrl { url: url.into() })
    }
}

#[derive(Serialize)]
struct IpcRequest {
    id: u64,
    command: IpcCommand,
}

#[derive(Serialize)]
#[serde(tag = "name", content = "payload", rename_all = "snake_case")]
enum IpcCommand {
    ListHosts,
    AddHost {
        address: String,
    },
    PairHost {
        host_id: String,
    },
    WakeHost {
        host_id: String,
    },
    RenameHost {
        host_id: String,
        name: String,
    },
    DeleteHost {
        host_id: String,
    },
    HostDetails {
        host_id: String,
    },
    TestNetwork {
        host_id: String,
    },
    ListApps {
        host_id: String,
        show_hidden: bool,
    },
    LaunchApp {
        host_id: String,
        app_id: String,
    },
    ResumeSession {
        host_id: String,
    },
    QuitRunningApp {
        host_id: String,
    },
    SetAppHidden {
        host_id: String,
        app_id: String,
        hidden: bool,
    },
    SetAppDirectLaunch {
        host_id: String,
        app_id: String,
        direct_launch: bool,
    },
    LoadSettings,
    SaveSettings {
        settings: StreamingSettings,
    },
    DefaultBitrate {
        width: u32,
        height: u32,
        fps: u32,
        yuv444: bool,
    },
    SystemInfo,
    OpenUrl {
        url: String,
    },
}

impl IpcCommand {
    fn name(&self) -> &'static str {
        match self {
            Self::ListHosts => "list_hosts",
            Self::AddHost { .. } => "add_host",
            Self::PairHost { .. } => "pair_host",
            Self::WakeHost { .. } => "wake_host",
            Self::RenameHost { .. } => "rename_host",
            Self::DeleteHost { .. } => "delete_host",
            Self::HostDetails { .. } => "host_details",
            Self::TestNetwork { .. } => "test_network",
            Self::ListApps { .. } => "list_apps",
            Self::LaunchApp { .. } => "launch_app",
            Self::ResumeSession { .. } => "resume_session",
            Self::QuitRunningApp { .. } => "quit_running_app",
            Self::SetAppHidden { .. } => "set_app_hidden",
            Self::SetAppDirectLaunch { .. } => "set_app_direct_launch",
            Self::LoadSettings => "load_settings",
            Self::SaveSettings { .. } => "save_settings",
            Self::DefaultBitrate { .. } => "default_bitrate",
            Self::SystemInfo => "system_info",
            Self::OpenUrl { .. } => "open_url",
        }
    }
}

#[derive(Deserialize)]
struct IpcResponse<T> {
    id: u64,
    result: Option<T>,
    error: Option<String>,
}

impl<T> IpcResponse<T> {
    fn into_result(self) -> Result<T, String> {
        match (self.result, self.error) {
            (Some(result), None) => Ok(result),
            (_, Some(error)) => Err(error),
            (None, None) => Err("Native helper response had no result or error.".into()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddHostResult {
    status: CommandStatus,
    host_id: String,
}
