import { ControllerAction, StreamingSettings } from '../bridge';

export const fallbackSettings: StreamingSettings = {
  width: 1920,
  height: 1080,
  fps: 60,
  bitrateKbps: 20000,
  packetSize: 0,
  audioConfig: 0,
  videoCodecConfig: 0,
  videoDecoderSelection: 0,
  windowMode: 1,
  uiDisplayMode: 0,
  language: 0,
  captureSysKeysMode: 1,
  unlockBitrate: false,
  autoAdjustBitrate: false,
  enableVsync: true,
  gameOptimizations: false,
  playAudioOnHost: false,
  multiController: true,
  enableMdns: true,
  quitAppAfter: false,
  absoluteMouseMode: false,
  absoluteTouchMode: true,
  framePacing: false,
  connectionWarnings: true,
  configurationWarnings: true,
  richPresence: true,
  enableHdr: false,
  gamepadMouse: true,
  detectNetworkBlocking: true,
  showPerformanceOverlay: false,
  swapMouseButtons: false,
  muteOnFocusLoss: false,
  backgroundGamepad: false,
  reverseScrollDirection: false,
  swapFaceButtons: false,
  keepAwake: true,
  enableYUV444: false,
};

export const controllerTestActions: ControllerAction[] = [
  'previousControl',
  'nextControl',
  'accept',
  'back',
  'settings',
  'contextMenu',
];

export const setupGuideUrl = 'https://github.com/moonlight-stream/moonlight-docs/wiki/Setup-Guide';
export const discordUrl = 'https://moonlight-stream.org/discord';
export const hardwareDecodingHelpUrl = 'https://github.com/moonlight-stream/moonlight-docs/wiki/Fixing-Hardware-Decoding-Problems';
export const gamepadMappingHelpUrl = 'https://github.com/moonlight-stream/moonlight-docs/wiki/Gamepad-Mapping';

export const audioConfigOptions = [
  { value: 0, label: 'Stereo' },
  { value: 1, label: '5.1 surround' },
  { value: 2, label: '7.1 surround' },
];

export const videoCodecOptions = [
  { value: 0, label: 'Automatic' },
  { value: 1, label: 'Force H.264' },
  { value: 2, label: 'Force HEVC' },
  { value: 4, label: 'Force AV1' },
];

export const videoDecoderOptions = [
  { value: 0, label: 'Automatic' },
  { value: 1, label: 'Force hardware' },
  { value: 2, label: 'Force software' },
];

export const windowModeOptions = [
  { value: 0, label: 'Fullscreen' },
  { value: 1, label: 'Borderless fullscreen' },
  { value: 2, label: 'Windowed' },
];

export const uiDisplayModeOptions = [
  { value: 0, label: 'Windowed' },
  { value: 1, label: 'Maximized' },
  { value: 2, label: 'Fullscreen' },
];

export const languageOptions = [
  { value: 0, label: 'Automatic' },
  { value: 1, label: 'English' },
  { value: 2, label: 'French' },
  { value: 3, label: 'Simplified Chinese' },
  { value: 4, label: 'German' },
  { value: 5, label: 'Norwegian Bokmal' },
  { value: 6, label: 'Russian' },
  { value: 7, label: 'Spanish' },
  { value: 8, label: 'Japanese' },
  { value: 9, label: 'Vietnamese' },
  { value: 10, label: 'Thai' },
  { value: 11, label: 'Korean' },
  { value: 12, label: 'Hungarian' },
  { value: 13, label: 'Dutch' },
  { value: 14, label: 'Swedish' },
  { value: 15, label: 'Turkish' },
  { value: 16, label: 'Ukrainian' },
  { value: 17, label: 'Traditional Chinese' },
  { value: 18, label: 'Portuguese' },
  { value: 19, label: 'Brazilian Portuguese' },
  { value: 20, label: 'Greek' },
  { value: 21, label: 'Italian' },
  { value: 22, label: 'Hindi' },
  { value: 23, label: 'Polish' },
  { value: 24, label: 'Czech' },
  { value: 25, label: 'Hebrew' },
  { value: 26, label: 'Central Kurdish' },
  { value: 27, label: 'Lithuanian' },
  { value: 28, label: 'Estonian' },
  { value: 29, label: 'Bulgarian' },
  { value: 30, label: 'Esperanto' },
  { value: 31, label: 'Tamil' },
];

export const captureSysKeysOptions = [
  { value: 0, label: 'Off' },
  { value: 1, label: 'Fullscreen only' },
  { value: 2, label: 'Always' },
];

export const numericSettingRules = {
  width: { label: 'Width', min: 256, max: 8192 },
  height: { label: 'Height', min: 256, max: 8192 },
  fps: { label: 'FPS', min: 10, max: 9999 },
  bitrateKbps: { label: 'Bitrate', min: 500, max: 500000 },
  packetSize: { label: 'Packet size', min: 0, max: 9000 },
} as const;

export type NumericSettingKey = keyof typeof numericSettingRules;
