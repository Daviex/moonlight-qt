import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type HostStatus = 'Online' | 'Offline' | 'Pairing required';

export interface HostEntry {
  id: string;
  name: string;
  address: string;
  status: HostStatus;
  paired: boolean;
  running: boolean;
}

export interface HostDetails {
  name: string;
  address: string;
  status: HostStatus;
  paired: boolean;
  running: boolean;
  serverVersion: string;
}

export interface AppEntry {
  id: string;
  name: string;
  hidden: boolean;
  directLaunch: boolean;
  running: boolean;
}

export interface StreamingSettings {
  width: number;
  height: number;
  fps: number;
  bitrateKbps: number;
  enableHdr: boolean;
  gamepadMouse: boolean;
}

export interface CommandStatus {
  message: string;
}

export interface NetworkTestResult {
  result: 'ok' | 'blocked' | 'unavailable';
  blockedPorts: string[];
  message: string;
}

export interface PairingChallenge {
  pin: string;
  message: string;
}

export type BridgeEventKind = 'hostChanged' | 'appChanged' | 'sessionChanged' | 'settingsChanged' | 'status';

export interface BridgeEvent {
  kind: BridgeEventKind;
  message: string;
  hostId?: string;
  appId?: string;
}

export const BRIDGE_EVENT = 'moonlight-bridge-event';

export const bridge = {
  listHosts: () => invoke<HostEntry[]>('list_hosts'),
  addHost: (address: string) => invoke<CommandStatus>('add_host', { address }),
  pairHost: (hostId: string) => invoke<PairingChallenge>('pair_host', { hostId }),
  wakeHost: (hostId: string) => invoke<CommandStatus>('wake_host', { hostId }),
  renameHost: (hostId: string, name: string) => invoke<CommandStatus>('rename_host', { hostId, name }),
  deleteHost: (hostId: string) => invoke<CommandStatus>('delete_host', { hostId }),
  hostDetails: (hostId: string) => invoke<HostDetails>('host_details', { hostId }),
  testNetwork: (hostId: string) => invoke<NetworkTestResult>('test_network', { hostId }),
  listApps: (hostId: string, showHidden: boolean) => invoke<AppEntry[]>('list_apps', { hostId, showHidden }),
  launchApp: (hostId: string, appId: string) => invoke<CommandStatus>('launch_app', { hostId, appId }),
  quitRunningApp: (hostId: string) => invoke<CommandStatus>('quit_running_app', { hostId }),
  setAppHidden: (hostId: string, appId: string, hidden: boolean) =>
    invoke<CommandStatus>('set_app_hidden', { hostId, appId, hidden }),
  setAppDirectLaunch: (hostId: string, appId: string, directLaunch: boolean) =>
    invoke<CommandStatus>('set_app_direct_launch', { hostId, appId, directLaunch }),
  loadSettings: () => invoke<StreamingSettings>('load_settings'),
  saveSettings: (settings: StreamingSettings) => invoke<CommandStatus>('save_settings', { settings }),
  listen: (handler: (event: BridgeEvent) => void) =>
    listen<BridgeEvent>(BRIDGE_EVENT, (event) => handler(event.payload)),
};
