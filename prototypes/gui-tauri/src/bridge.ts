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
  wakeable: boolean;
  serverSupported: boolean;
}

export interface HostDetails {
  name: string;
  address: string;
  status: HostStatus;
  paired: boolean;
  running: boolean;
  wakeable: boolean;
  serverSupported: boolean;
  uuid: string;
  localAddress: string;
  remoteAddress: string;
  ipv6Address: string;
  manualAddress: string;
  macAddress: string;
  pairState: string;
  runningGameId: number;
  httpsPort: number;
  appVersion: string;
  gfeVersion: string;
  serverVersion: string;
  gpuModel: string;
  details: string;
}

export interface AppEntry {
  id: string;
  name: string;
  boxArtUrl: string;
  hidden: boolean;
  directLaunch: boolean;
  running: boolean;
  appCollectorGame: boolean;
}

export interface StreamingSettings {
  width: number;
  height: number;
  fps: number;
  bitrateKbps: number;
  packetSize: number;
  audioConfig: number;
  videoCodecConfig: number;
  videoDecoderSelection: number;
  windowMode: number;
  uiDisplayMode: number;
  language: number;
  captureSysKeysMode: number;
  unlockBitrate: boolean;
  autoAdjustBitrate: boolean;
  enableVsync: boolean;
  gameOptimizations: boolean;
  playAudioOnHost: boolean;
  multiController: boolean;
  enableMdns: boolean;
  quitAppAfter: boolean;
  absoluteMouseMode: boolean;
  absoluteTouchMode: boolean;
  framePacing: boolean;
  connectionWarnings: boolean;
  configurationWarnings: boolean;
  richPresence: boolean;
  enableHdr: boolean;
  gamepadMouse: boolean;
  detectNetworkBlocking: boolean;
  showPerformanceOverlay: boolean;
  swapMouseButtons: boolean;
  muteOnFocusLoss: boolean;
  backgroundGamepad: boolean;
  reverseScrollDirection: boolean;
  swapFaceButtons: boolean;
  keepAwake: boolean;
  enableYUV444: boolean;
}

export interface StreamControllerInput {
  controllerNumber: number;
  activeGamepadMask: number;
  buttonFlags: number;
  leftTrigger: number;
  rightTrigger: number;
  leftStickX: number;
  leftStickY: number;
  rightStickX: number;
  rightStickY: number;
}

export type StreamWindowMode = 'fullscreen' | 'borderlessFullscreen' | 'windowed';
export type InputCapturePolicy = 'firstPointerEnter' | 'afterRendererCreated';

export interface StreamWindowDescriptor {
  title: string;
  width: number;
  height: number;
  mode: StreamWindowMode;
  resizable: boolean;
  highDpi: boolean;
  inputCapturePolicy: InputCapturePolicy;
}

export interface ActiveStreamSession {
  hostId: string;
  appId: string;
  appName: string;
  window: StreamWindowDescriptor;
}

export interface DisplayInfo {
  nativeWidth: number;
  nativeHeight: number;
  safeAreaWidth: number;
  safeAreaHeight: number;
  refreshRate: number;
}

export interface SystemInfo {
  version: string;
  friendlyNativeArchName: string;
  isRunningWayland: boolean;
  isRunningXWayland: boolean;
  isWow64: boolean;
  hasDesktopEnvironment: boolean;
  hasBrowser: boolean;
  hasDiscordIntegration: boolean;
  usesMaterial3Theme: boolean;
  hasHardwareAcceleration: boolean;
  rendererAlwaysFullScreen: boolean;
  maximumResolutionWidth: number;
  maximumResolutionHeight: number;
  supportsHdr: boolean;
  unmappedGamepads: string;
  displays: DisplayInfo[];
}

export interface CommandStatus {
  message: string;
}

export interface BackendInfo {
  mode: string;
  helperPath?: string;
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

export type ControllerAction =
  | 'up'
  | 'down'
  | 'left'
  | 'right'
  | 'accept'
  | 'back'
  | 'contextMenu'
  | 'settings'
  | 'nextControl'
  | 'previousControl'
  | 'activateControl';

export type BridgeEventKind =
  | 'hostChanged'
  | 'appChanged'
  | 'sessionChanged'
  | 'settingsChanged'
  | 'status'
  | 'controllerAction'
  | 'updateAvailable';

export interface BridgeEvent {
  kind: BridgeEventKind;
  message: string;
  hostId?: string;
  appId?: string;
  controllerAction?: ControllerAction;
  updateVersion?: string;
  updateUrl?: string;
}

export const BRIDGE_EVENT = 'moonlight-bridge-event';

export const bridge = {
  debugLog: (message: string) => invoke<CommandStatus>('debug_log', { message }),
  backendInfo: () => invoke<BackendInfo>('backend_info'),
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
  resumeSession: (hostId: string) => invoke<CommandStatus>('resume_session', { hostId }),
  quitRunningApp: (hostId: string) => invoke<CommandStatus>('quit_running_app', { hostId }),
  setAppHidden: (hostId: string, appId: string, hidden: boolean) =>
    invoke<CommandStatus>('set_app_hidden', { hostId, appId, hidden }),
  setAppDirectLaunch: (hostId: string, appId: string, directLaunch: boolean) =>
    invoke<CommandStatus>('set_app_direct_launch', { hostId, appId, directLaunch }),
  loadSettings: () => invoke<StreamingSettings>('load_settings'),
  saveSettings: (settings: StreamingSettings) => invoke<CommandStatus>('save_settings', { settings }),
  defaultBitrate: (width: number, height: number, fps: number, yuv444: boolean) =>
    invoke<number>('default_bitrate', { width, height, fps, yuv444 }),
  systemInfo: () => invoke<SystemInfo>('system_info'),
  openUrl: (url: string) => invoke<CommandStatus>('open_url', { url }),
  activeStreamWindow: () => invoke<StreamWindowDescriptor | null>('active_stream_window'),
  activeStreamSession: () => invoke<ActiveStreamSession | null>('active_stream_session'),
  emitControllerAction: (action: ControllerAction) =>
    invoke<CommandStatus>('emit_controller_action', { action }),
  streamMouseMove: (deltaX: number, deltaY: number) =>
    invoke<CommandStatus>('stream_mouse_move', { deltaX, deltaY }),
  streamMousePosition: (x: number, y: number, referenceWidth: number, referenceHeight: number) =>
    invoke<CommandStatus>('stream_mouse_position', { x, y, referenceWidth, referenceHeight }),
  streamMouseButton: (button: 'left' | 'middle' | 'right' | 'x1' | 'x2', pressed: boolean) =>
    invoke<CommandStatus>('stream_mouse_button', { button, pressed }),
  streamKeyboard: (
    keyCode: number,
    pressed: boolean,
    shift: boolean,
    ctrl: boolean,
    alt: boolean,
    meta: boolean,
    nonNormalized: boolean,
  ) =>
    invoke<CommandStatus>('stream_keyboard', { keyCode, pressed, shift, ctrl, alt, meta, nonNormalized }),
  streamText: (text: string) => invoke<CommandStatus>('stream_text', { text }),
  streamScroll: (deltaX: number, deltaY: number) =>
    invoke<CommandStatus>('stream_scroll', { deltaX, deltaY }),
  streamController: (input: StreamControllerInput) =>
    invoke<CommandStatus>('stream_controller', { ...input }),
  listen: (handler: (event: BridgeEvent) => void) =>
    listen<BridgeEvent>(BRIDGE_EVENT, (event) => handler(event.payload)),
};
