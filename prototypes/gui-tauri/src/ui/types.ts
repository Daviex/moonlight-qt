import { HostDetails, HostEntry, PairingChallenge } from '../bridge';

export type Page = 'hosts' | 'apps' | 'settings';
export type StreamPhase = 'idle' | 'launching' | 'active' | 'quitting' | 'cleanup' | 'finished' | 'error';

export interface StreamUiState {
  phase: StreamPhase;
  hostId: string;
  hostName: string;
  appName: string;
  message: string;
  warnings: string[];
  errors: string[];
  uiHiddenRequested: boolean;
}

export interface UpdateInfo {
  version: string;
  url: string;
  message: string;
}

export type DialogState =
  | { kind: 'none' }
  | { kind: 'addHost'; address: string; error: string; submitting: boolean }
  | { kind: 'renameHost'; host: HostEntry; name: string; error: string; submitting: boolean }
  | { kind: 'deleteHost'; host: HostEntry; error: string; submitting: boolean }
  | { kind: 'pairing'; host: HostEntry; challenge: PairingChallenge }
  | { kind: 'hostDetails'; details: HostDetails }
  | { kind: 'help' };
