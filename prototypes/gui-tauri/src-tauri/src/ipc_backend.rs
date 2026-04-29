use crate::backend::{
    AppEntry, CommandStatus, HostDetails, HostEntry, MoonlightBackend, NetworkTestResult,
    PairingChallenge, StreamingSettings,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const BACKEND_MODE_ENV: &str = "MOONLIGHT_TAURI_BACKEND";
const HELPER_PATH_ENV: &str = "MOONLIGHT_TAURI_HELPER";

pub fn ipc_backend_requested() -> bool {
    env::var(BACKEND_MODE_ENV)
        .map(|value| value.eq_ignore_ascii_case("ipc"))
        .unwrap_or(false)
}

pub struct IpcBackend {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
}

impl IpcBackend {
    pub fn from_environment() -> Result<Self, String> {
        let helper_path = env::var(HELPER_PATH_ENV).map_err(|_| {
            format!("{HELPER_PATH_ENV} must point to the native helper executable.")
        })?;
        Self::spawn(helper_path)
    }

    fn spawn(helper_path: String) -> Result<Self, String> {
        let mut process = Command::new(&helper_path)
            .arg("--tauri-bridge-helper")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("Failed to start native helper '{helper_path}': {error}"))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| "Failed to open native helper stdin.".to_string())?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| "Failed to open native helper stdout.".to_string())?;

        Ok(Self {
            process,
            stdin,
            stdout: BufReader::new(stdout),
            next_request_id: 1,
        })
    }

    fn request<R>(&mut self, command: IpcCommand) -> Result<R, String>
    where
        R: DeserializeOwned,
    {
        let request_id = self.next_request_id;
        self.next_request_id += 1;

        serde_json::to_writer(
            &mut self.stdin,
            &IpcRequest {
                id: request_id,
                command,
            },
        )
        .map_err(|error| format!("Failed to serialize native helper request: {error}"))?;
        self.stdin
            .write_all(b"\n")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("Failed to send native helper request: {error}"))?;

        let mut line = String::new();
        let bytes_read = self
            .stdout
            .read_line(&mut line)
            .map_err(|error| format!("Failed to read native helper response: {error}"))?;
        if bytes_read == 0 {
            return Err("Native helper exited before sending a response.".into());
        }

        let response: IpcResponse<R> = serde_json::from_str(&line)
            .map_err(|error| format!("Failed to parse native helper response: {error}"))?;
        if response.id != request_id {
            return Err(format!(
                "Native helper response ID mismatch: expected {request_id}, got {}.",
                response.id
            ));
        }

        response.into_result()
    }
}

impl Drop for IpcBackend {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl MoonlightBackend for IpcBackend {
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
}

#[derive(Serialize)]
struct IpcRequest {
    id: u64,
    command: IpcCommand,
}

#[derive(Serialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
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
