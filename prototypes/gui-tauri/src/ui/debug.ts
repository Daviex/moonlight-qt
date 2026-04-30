export const debugUiStorageKey = 'moonlight-tauri-debug-ui';

export function readStoredDebugUi() {
  return window.localStorage.getItem(debugUiStorageKey) === '1';
}

export function applyStoredDebugUi(enabled: boolean) {
  window.localStorage.setItem(debugUiStorageKey, enabled ? '1' : '0');
}
