import { StreamPhase, StreamUiState } from './types';

export const idleStreamState: StreamUiState = {
  phase: 'idle',
  hostId: '',
  hostName: '',
  appName: '',
  message: '',
  warnings: [],
  errors: [],
  uiHiddenRequested: false,
};

export function streamPhaseLabel(phase: StreamPhase) {
  switch (phase) {
  case 'launching':
    return 'Starting';
  case 'active':
    return 'Streaming';
  case 'quitting':
    return 'Quitting';
  case 'cleanup':
    return 'Cleaning up';
  case 'finished':
    return 'Finished';
  case 'error':
    return 'Needs attention';
  case 'idle':
    return 'Idle';
  }
}

export function streamPhaseHelp(phase: StreamPhase) {
  switch (phase) {
  case 'launching':
    return 'Moonlight is creating the streaming session through the Rust backend. Keep this shell open until the native stream window takes over.';
  case 'active':
    return 'The native stream owns video, audio, and input. The Tauri shell can stay hidden until the Rust session asks for it again.';
  case 'quitting':
    return 'Moonlight is asking the host app and native session to stop. Wait for cleanup before launching another app.';
  case 'cleanup':
    return 'The stream has ended and Moonlight is releasing native session resources.';
  case 'finished':
    return 'The Rust-owned session has finished and the Tauri shell is ready for the next action.';
  case 'error':
    return 'Moonlight reported a stream problem. Review the messages below, then dismiss the stream state after the native session is no longer active.';
  case 'idle':
    return '';
  }
}
