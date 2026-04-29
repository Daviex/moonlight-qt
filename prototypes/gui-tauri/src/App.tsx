import { useCallback, useEffect, useMemo, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  AppEntry,
  BackendInfo,
  BridgeEvent,
  ControllerAction,
  HostDetails,
  HostEntry,
  PairingChallenge,
  StreamingSettings,
  SystemInfo,
  bridge,
} from './bridge';

type Page = 'hosts' | 'apps' | 'settings';
type StreamPhase = 'idle' | 'launching' | 'active' | 'quitting' | 'finished' | 'error';

interface StreamUiState {
  phase: StreamPhase;
  hostId: string;
  hostName: string;
  appName: string;
  message: string;
  warnings: string[];
  errors: string[];
  uiHiddenRequested: boolean;
}

interface UpdateInfo {
  version: string;
  url: string;
  message: string;
}

type DialogState =
  | { kind: 'none' }
  | { kind: 'addHost'; address: string; error: string; submitting: boolean }
  | { kind: 'renameHost'; host: HostEntry; name: string; error: string; submitting: boolean }
  | { kind: 'deleteHost'; host: HostEntry; error: string; submitting: boolean }
  | { kind: 'pairing'; host: HostEntry; challenge: PairingChallenge }
  | { kind: 'hostDetails'; details: HostDetails }
  | { kind: 'help' };

const fallbackSettings: StreamingSettings = {
  width: 1920,
  height: 1080,
  fps: 60,
  bitrateKbps: 20000,
  enableHdr: false,
  gamepadMouse: true,
};

const controllerTestActions: ControllerAction[] = [
  'previousControl',
  'nextControl',
  'accept',
  'back',
  'settings',
  'contextMenu',
];

const idleStreamState: StreamUiState = {
  phase: 'idle',
  hostId: '',
  hostName: '',
  appName: '',
  message: '',
  warnings: [],
  errors: [],
  uiHiddenRequested: false,
};

const appWindow = getCurrentWindow();
const setupGuideUrl = 'https://github.com/moonlight-stream/moonlight-docs/wiki/Setup-Guide';
const discordUrl = 'https://moonlight-stream.org/discord';
const hardwareDecodingHelpUrl = 'https://github.com/moonlight-stream/moonlight-docs/wiki/Fixing-Hardware-Decoding-Problems';
const gamepadMappingHelpUrl = 'https://github.com/moonlight-stream/moonlight-docs/wiki/Gamepad-Mapping';

function writeDebugLog(message: string) {
  void bridge.debugLog(message).catch(() => undefined);
}

function activeDialogRoot(): HTMLElement | null {
  return document.getElementById('active-dialog');
}

function focusableElements(root: ParentNode = document): HTMLElement[] {
  return Array.from(
    root.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'),
  ).filter((element) =>
    !element.hasAttribute('disabled') &&
    element.tabIndex !== -1 &&
    element.offsetParent !== null,
  );
}

function moveFocus(delta: number) {
  const elements = focusableElements(activeDialogRoot() ?? document);
  if (elements.length === 0) {
    return;
  }

  const currentIndex = Math.max(0, elements.indexOf(document.activeElement as HTMLElement));
  const nextIndex = (currentIndex + delta + elements.length) % elements.length;
  elements[nextIndex].focus();
}

export default function App() {
  const [page, setPage] = useState<Page>('hosts');
  const [hosts, setHosts] = useState<HostEntry[]>([]);
  const [apps, setApps] = useState<AppEntry[]>([]);
  const [selectedHostId, setSelectedHostId] = useState('');
  const [settings, setSettings] = useState<StreamingSettings>(fallbackSettings);
  const [showHiddenApps, setShowHiddenApps] = useState(false);
  const [status, setStatus] = useState('Tauri shell ready.');
  const [eventLog, setEventLog] = useState<BridgeEvent[]>([]);
  const [streamState, setStreamState] = useState<StreamUiState>(idleStreamState);
  const [backendInfo, setBackendInfo] = useState<BackendInfo | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [dialog, setDialog] = useState<DialogState>({ kind: 'none' });
  const [hostRefreshDiagnostics, setHostRefreshDiagnostics] = useState({
    attempts: 0,
    lastCount: 0,
    lastError: '',
  });

  const selectedHost = useMemo(
    () => hosts.find((host) => host.id === selectedHostId),
    [hosts, selectedHostId],
  );

  const refreshHosts = useCallback(async () => {
    writeDebugLog(`refreshHosts begin; selectedHostId=${selectedHostId}`);
    try {
      const nextHosts = await bridge.listHosts();
      writeDebugLog(`refreshHosts success; count=${nextHosts.length}`);
      setHosts(nextHosts);
      if (!nextHosts.some((host) => host.id === selectedHostId)) {
        setSelectedHostId(nextHosts[0]?.id ?? '');
      }
      setHostRefreshDiagnostics((previousDiagnostics) => ({
        attempts: previousDiagnostics.attempts + 1,
        lastCount: nextHosts.length,
        lastError: '',
      }));
      setStatus(nextHosts.length > 0 ? 'Host list refreshed.' : 'No hosts found yet. Discovery is still running.');
      return nextHosts;
    }
    catch (error) {
      const message = String(error);
      writeDebugLog(`refreshHosts failed; error=${message}`);
      setHostRefreshDiagnostics((previousDiagnostics) => ({
        ...previousDiagnostics,
        attempts: previousDiagnostics.attempts + 1,
        lastError: message,
      }));
      setStatus(`Failed to refresh hosts: ${message}`);
      return [];
    }
  }, [selectedHostId]);

  const refreshBackendInfo = useCallback(async () => {
    writeDebugLog('refreshBackendInfo begin');
    try {
      const info = await bridge.backendInfo();
      writeDebugLog(`refreshBackendInfo success; mode=${info.mode}; helperPath=${info.helperPath ?? ''}`);
      setBackendInfo(info);
    }
    catch (error) {
      writeDebugLog(`refreshBackendInfo failed; error=${String(error)}`);
      setStatus(`Failed to load backend info: ${String(error)}`);
    }
  }, []);

  const refreshApps = useCallback(async (hostId = selectedHostId, includeHidden = showHiddenApps) => {
    writeDebugLog(`refreshApps begin; hostId=${hostId}; includeHidden=${includeHidden}`);
    if (!hostId) {
      setApps([]);
      writeDebugLog('refreshApps skipped; no hostId');
      return;
    }

    try {
      const nextApps = await bridge.listApps(hostId, includeHidden);
      writeDebugLog(`refreshApps success; count=${nextApps.length}`);
      setApps(nextApps);
      setStatus('App list loaded.');
    }
    catch (error) {
      writeDebugLog(`refreshApps failed; error=${String(error)}`);
      setStatus(`Failed to load apps: ${String(error)}`);
    }
  }, [selectedHostId, showHiddenApps]);

  const openApps = useCallback(async (hostId: string) => {
    setSelectedHostId(hostId);
    await refreshApps(hostId, showHiddenApps);
    setPage('apps');
  }, [refreshApps, showHiddenApps]);

  const runHostCommand = useCallback(async (action: () => Promise<{ message: string }>) => {
    try {
      const result = await action();
      setStatus(result.message);
      await refreshHosts();
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [refreshHosts]);

  const closeDialog = useCallback(() => {
    setDialog({ kind: 'none' });
  }, []);

  const openAddHostDialog = useCallback(() => {
    setDialog({ kind: 'addHost', address: '', error: '', submitting: false });
  }, []);

  const submitAddHost = useCallback(async () => {
    if (dialog.kind !== 'addHost') {
      return;
    }

    const address = dialog.address.trim();
    if (!address) {
      setDialog({ ...dialog, error: 'Enter a host address.' });
      return;
    }

    setDialog({ ...dialog, submitting: true });
    try {
      const result = await bridge.addHost(address);
      setStatus(result.message);
      setDialog({ kind: 'none' });
      await refreshHosts();
    }
    catch (error) {
      const message = String(error);
      setStatus(message);
      setDialog({ ...dialog, error: message, submitting: false });
    }
  }, [dialog, refreshHosts]);

  const openRenameHostDialog = useCallback((host: HostEntry) => {
    setDialog({ kind: 'renameHost', host, name: host.name, error: '', submitting: false });
  }, []);

  const submitRenameHost = useCallback(async () => {
    if (dialog.kind !== 'renameHost') {
      return;
    }

    const name = dialog.name.trim();
    if (!name) {
      setDialog({ ...dialog, error: 'Enter a host name.' });
      return;
    }

    setDialog({ ...dialog, submitting: true });
    try {
      const result = await bridge.renameHost(dialog.host.id, name);
      setStatus(result.message);
      setDialog({ kind: 'none' });
      await refreshHosts();
    }
    catch (error) {
      const message = String(error);
      setStatus(message);
      setDialog({ ...dialog, error: message, submitting: false });
    }
  }, [dialog, refreshHosts]);

  const openDeleteHostDialog = useCallback((host: HostEntry) => {
    setDialog({ kind: 'deleteHost', host, error: '', submitting: false });
  }, []);

  const confirmDeleteHost = useCallback(async () => {
    if (dialog.kind !== 'deleteHost') {
      return;
    }

    setDialog({ ...dialog, submitting: true });
    try {
      const result = await bridge.deleteHost(dialog.host.id);
      setStatus(result.message);
      setDialog({ kind: 'none' });
      await refreshHosts();
    }
    catch (error) {
      const message = String(error);
      setStatus(message);
      setDialog({ ...dialog, error: message, submitting: false });
    }
  }, [dialog, refreshHosts]);

  const showDetails = useCallback(async (host: HostEntry) => {
    try {
      const details = await bridge.hostDetails(host.id);
      setStatus(`${details.name}: ${details.status}, ${details.address}, ${details.serverVersion}`);
      setDialog({ kind: 'hostDetails', details });
    }
    catch (error) {
      setStatus(String(error));
    }
  }, []);

  const testNetwork = useCallback(async (host: HostEntry) => {
    try {
      const result = await bridge.testNetwork(host.id);
      const blockedPorts = result.blockedPorts.length > 0 ? ` Blocked ports: ${result.blockedPorts.join(', ')}` : '';
      setStatus(`${result.message}${blockedPorts}`);
    }
    catch (error) {
      setStatus(String(error));
    }
  }, []);

  const pairHost = useCallback(async (host: HostEntry) => {
    try {
      const challenge = await bridge.pairHost(host.id);
      setStatus(challenge.message);
      setDialog({ kind: 'pairing', host, challenge });
      await refreshHosts();
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [refreshHosts]);

  const loadSystemInfo = useCallback(async () => {
    writeDebugLog('loadSystemInfo begin');
    try {
      const info = await bridge.systemInfo();
      writeDebugLog(`loadSystemInfo success; version=${info.version}; displays=${info.displays.length}`);
      setSystemInfo(info);
      return info;
    }
    catch (error) {
      const message = String(error);
      writeDebugLog(`loadSystemInfo failed; error=${message}`);
      setStatus(message);
      return null;
    }
  }, []);

  const openHelpDialog = useCallback(() => {
    setDialog({ kind: 'help' });
    void loadSystemInfo();
  }, [loadSystemInfo]);

  const openExternalUrl = useCallback(async (url: string, label: string) => {
    try {
      const result = await bridge.openUrl(url);
      setStatus(result.message || `Opened ${label}.`);
    }
    catch (error) {
      setStatus(`Failed to open ${label}: ${String(error)}`);
    }
  }, []);

  const loadSettings = useCallback(async () => {
    try {
      setSettings(await bridge.loadSettings());
      setPage('settings');
      setStatus('Settings loaded.');
    }
    catch (error) {
      setStatus(String(error));
    }
  }, []);

  const refreshSettingsSnapshot = useCallback(async () => {
    try {
      setSettings(await bridge.loadSettings());
    }
    catch (error) {
      setStatus(String(error));
    }
  }, []);

  const saveSettings = useCallback(async () => {
    try {
      const result = await bridge.saveSettings(settings);
      setStatus(result.message);
      setPage('hosts');
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [settings]);

  const launchApp = useCallback(async (app: AppEntry) => {
    if (!selectedHostId) {
      return;
    }

    setStreamState({
      phase: 'launching',
      hostId: selectedHostId,
      hostName: selectedHost?.name ?? selectedHostId,
      appName: app.name,
      message: `Launching ${app.name}...`,
      warnings: [],
      errors: [],
      uiHiddenRequested: false,
    });

    try {
      const result = await bridge.launchApp(selectedHostId, app.id);
      setStatus(result.message);
      await refreshApps(selectedHostId, showHiddenApps);
      await refreshHosts();
    }
    catch (error) {
      const message = String(error);
      setStatus(message);
      setStreamState((previousState) => ({
        ...previousState,
        phase: 'error',
        message,
        errors: [...previousState.errors, message],
      }));
    }
  }, [refreshApps, refreshHosts, selectedHost?.name, selectedHostId, showHiddenApps]);

  const resumeSession = useCallback(async (host: HostEntry) => {
    setSelectedHostId(host.id);
    setStreamState({
      phase: 'launching',
      hostId: host.id,
      hostName: host.name,
      appName: 'Running session',
      message: `Resuming the running session on ${host.name}...`,
      warnings: [],
      errors: [],
      uiHiddenRequested: false,
    });

    try {
      const result = await bridge.resumeSession(host.id);
      setStatus(result.message);
      await refreshHosts();
    }
    catch (error) {
      const message = String(error);
      setStatus(message);
      setStreamState((previousState) => ({
        ...previousState,
        phase: 'error',
        message,
        errors: [...previousState.errors, message],
      }));
    }
  }, [refreshHosts]);

  const quitRunningApp = useCallback(async () => {
    const hostId = selectedHostId || streamState.hostId;
    if (!hostId) {
      setStatus('No active host is selected for quit.');
      return;
    }

    setStreamState((previousState) => ({
      ...previousState,
      phase: 'quitting',
      message: 'Quit requested...',
    }));

    try {
      const result = await bridge.quitRunningApp(hostId);
      setStatus(result.message);
      await refreshApps(hostId, showHiddenApps);
      await refreshHosts();
    }
    catch (error) {
      const message = String(error);
      setStatus(message);
      setStreamState((previousState) => ({
        ...previousState,
        phase: 'error',
        message,
        errors: [...previousState.errors, message],
      }));
    }
  }, [refreshApps, refreshHosts, selectedHostId, showHiddenApps, streamState.hostId]);

  const handleSessionEvent = useCallback((event: BridgeEvent) => {
    setStreamState((previousState) => {
      const message = event.message;
      const nextState = previousState.phase === 'idle'
        ? { ...idleStreamState, phase: 'launching' as StreamPhase }
        : previousState;

      if (message.includes('hide UI requested')) {
        return {
          ...nextState,
          phase: 'active',
          message,
          uiHiddenRequested: true,
        };
      }

      if (message.includes('UI can be shown')) {
        return {
          ...nextState,
          phase: 'active',
          message,
          uiHiddenRequested: false,
        };
      }

      if (message.startsWith('Quitting ')) {
        return {
          ...nextState,
          phase: 'quitting',
          message,
        };
      }

      if (message.includes('finished') || message.includes('cleanup completed')) {
        return {
          ...nextState,
          phase: 'finished',
          message,
          uiHiddenRequested: false,
        };
      }

      return {
        ...nextState,
        phase: nextState.phase === 'idle' ? 'launching' : nextState.phase,
        message,
      };
    });
  }, []);

  const handleStatusEvent = useCallback((event: BridgeEvent) => {
    setStreamState((previousState) => {
      if (previousState.phase === 'idle') {
        return previousState;
      }

      const message = event.message;
      const isError = /failed|error|terminated|unable|cannot/i.test(message);
      return {
        ...previousState,
        phase: isError ? 'error' : previousState.phase,
        message,
        warnings: isError ? previousState.warnings : [...previousState.warnings, message].slice(-4),
        errors: isError ? [...previousState.errors, message].slice(-4) : previousState.errors,
      };
    });
  }, []);

  const showTauriShell = useCallback(async () => {
    await appWindow.show();
    await appWindow.setFocus();
  }, []);

  const syncWindowForSessionEvent = useCallback(async (event: BridgeEvent) => {
    if (event.kind === 'sessionChanged' && event.message.includes('hide UI requested')) {
      await appWindow.hide();
      return;
    }

    if (event.kind === 'sessionChanged' && (
      event.message.includes('UI can be shown') ||
      event.message.includes('finished') ||
      event.message.includes('cleanup completed')
    )) {
      await showTauriShell();
      return;
    }

    if (event.kind === 'status' && /failed|error|terminated|unable|cannot/i.test(event.message)) {
      await showTauriShell();
    }
  }, [showTauriShell]);

  const toggleHidden = useCallback(async (app: AppEntry) => {
    if (!selectedHostId) {
      return;
    }

    try {
      const result = await bridge.setAppHidden(selectedHostId, app.id, !app.hidden);
      setStatus(result.message);
      await refreshApps(selectedHostId, showHiddenApps);
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [refreshApps, selectedHostId, showHiddenApps]);

  const toggleDirectLaunch = useCallback(async (app: AppEntry) => {
    if (!selectedHostId) {
      return;
    }

    try {
      const result = await bridge.setAppDirectLaunch(selectedHostId, app.id, !app.directLaunch);
      setStatus(result.message);
      await refreshApps(selectedHostId, showHiddenApps);
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [refreshApps, selectedHostId, showHiddenApps]);

  const handleControllerAction = useCallback((action: ControllerAction) => {
    switch (action) {
    case 'up':
    case 'left':
    case 'previousControl':
      moveFocus(-1);
      break;
    case 'down':
    case 'right':
    case 'nextControl':
      moveFocus(1);
      break;
    case 'accept':
    case 'activateControl': {
      const root = activeDialogRoot() ?? document;
      const activeElement = document.activeElement;
      if (activeElement instanceof HTMLElement && root.contains(activeElement)) {
        activeElement.click();
      }
      else {
        focusableElements(root)[0]?.focus();
      }
      break;
    }
    case 'back':
      if (dialog.kind !== 'none') {
        closeDialog();
        break;
      }
      if (page === 'apps' || page === 'settings') {
        setPage('hosts');
      }
      else {
        setStatus('Back requested from the host page.');
      }
      break;
    case 'settings':
      if (dialog.kind !== 'none') {
        closeDialog();
      }
      void loadSettings();
      break;
    case 'contextMenu':
      setStatus('Context menu requested by controller navigation.');
      break;
    }
  }, [closeDialog, dialog.kind, loadSettings, page]);

  const handleBridgeEvent = useCallback((event: BridgeEvent) => {
    writeDebugLog(`bridge event received; kind=${event.kind}; message=${event.message}`);
    setEventLog((previousEvents) => [event, ...previousEvents].slice(0, 6));
    void (async () => {
      if (event.kind === 'controllerAction' && event.controllerAction) {
        handleControllerAction(event.controllerAction);
      }

      if (event.kind === 'sessionChanged') {
        handleSessionEvent(event);
      }

      if (event.kind === 'status') {
        handleStatusEvent(event);
      }

      if (event.kind === 'updateAvailable') {
        setUpdateInfo({
          version: event.updateVersion ?? '',
          url: event.updateUrl ?? '',
          message: event.message,
        });
      }

      try {
        await syncWindowForSessionEvent(event);
      }
      catch (error) {
        setStatus(`Failed to update Tauri window visibility: ${String(error)}`);
      }

      if (event.kind === 'hostChanged' || event.kind === 'sessionChanged') {
        await refreshHosts();
      }

      if ((event.kind === 'appChanged' || event.kind === 'sessionChanged') && event.hostId === selectedHostId) {
        await refreshApps(event.hostId, showHiddenApps);
      }

      if (event.kind === 'settingsChanged') {
        await refreshSettingsSnapshot();
      }

      setStatus(event.message);
    })();
  }, [handleControllerAction, handleSessionEvent, handleStatusEvent, refreshApps, refreshHosts, refreshSettingsSnapshot, selectedHostId, showHiddenApps, syncWindowForSessionEvent]);

  useEffect(() => {
    writeDebugLog('frontend mounted; loading backend info and hosts');
    void refreshBackendInfo();
    void refreshHosts();
  }, [refreshBackendInfo, refreshHosts]);

  useEffect(() => {
    if (dialog.kind === 'none') {
      return undefined;
    }

    const focusTimer = window.setTimeout(() => {
      focusableElements(activeDialogRoot() ?? document)[0]?.focus();
    }, 0);

    return () => window.clearTimeout(focusTimer);
  }, [dialog.kind]);

  useEffect(() => {
    if (hosts.length > 0) {
      return undefined;
    }

    let attempts = 0;
    const intervalId = window.setInterval(() => {
      attempts += 1;
      void refreshHosts().then((nextHosts) => {
        if (nextHosts.length > 0 || attempts >= 15) {
          window.clearInterval(intervalId);
        }
      });
    }, 2000);

    return () => window.clearInterval(intervalId);
  }, [hosts.length, refreshHosts]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    bridge.listen(handleBridgeEvent)
      .then((nextUnlisten) => {
        writeDebugLog('bridge event subscription ready');
        if (disposed) {
          nextUnlisten();
        }
        else {
          unlisten = nextUnlisten;
        }
      })
      .catch((error) => {
        writeDebugLog(`bridge event subscription failed; error=${String(error)}`);
        setStatus(`Failed to subscribe to native events: ${String(error)}`);
      });

    return () => {
      writeDebugLog('frontend unmounting; removing bridge event subscription');
      disposed = true;
      unlisten?.();
    };
  }, [handleBridgeEvent]);

  const renderDialog = () => {
    if (dialog.kind === 'none') {
      return null;
    }

    return (
      <div className="dialog-backdrop" role="presentation">
        <section
          id="active-dialog"
          className="dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="dialog-title"
        >
          {dialog.kind === 'addHost' && (
            <>
              <h2 id="dialog-title">Add host</h2>
              <p>Enter the Sunshine host address or hostname. Moonlight will add it through the native bridge.</p>
              <label className="form-field">
                Host address
                <input
                  value={dialog.address}
                  disabled={dialog.submitting}
                  onChange={(event) => setDialog({ ...dialog, address: event.target.value, error: '' })}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      void submitAddHost();
                    }
                  }}
                />
              </label>
              {dialog.error && <p className="dialog-error">{dialog.error}</p>}
              <div className="button-row">
                <button type="button" onClick={closeDialog} disabled={dialog.submitting}>Cancel</button>
                <button type="button" onClick={submitAddHost} disabled={dialog.submitting}>Add Host</button>
              </div>
            </>
          )}

          {dialog.kind === 'renameHost' && (
            <>
              <h2 id="dialog-title">Rename host</h2>
              <p>Choose the display name for {dialog.host.name}.</p>
              <label className="form-field">
                Host name
                <input
                  value={dialog.name}
                  disabled={dialog.submitting}
                  onChange={(event) => setDialog({ ...dialog, name: event.target.value, error: '' })}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      void submitRenameHost();
                    }
                  }}
                />
              </label>
              {dialog.error && <p className="dialog-error">{dialog.error}</p>}
              <div className="button-row">
                <button type="button" onClick={closeDialog} disabled={dialog.submitting}>Cancel</button>
                <button type="button" onClick={submitRenameHost} disabled={dialog.submitting}>Save Name</button>
              </div>
            </>
          )}

          {dialog.kind === 'deleteHost' && (
            <>
              <h2 id="dialog-title">Delete host</h2>
              <p>Remove {dialog.host.name} from Moonlight?</p>
              {dialog.error && <p className="dialog-error">{dialog.error}</p>}
              <div className="button-row">
                <button type="button" onClick={closeDialog} disabled={dialog.submitting}>Cancel</button>
                <button type="button" className="danger" onClick={confirmDeleteHost} disabled={dialog.submitting}>
                  Delete Host
                </button>
              </div>
            </>
          )}

          {dialog.kind === 'pairing' && (
            <>
              <h2 id="dialog-title">Pair with {dialog.host.name}</h2>
              <p>{dialog.challenge.message}</p>
              <div className="pin-code" aria-label="Pairing PIN">{dialog.challenge.pin}</div>
              <p>Enter this PIN on the host to complete pairing.</p>
              <div className="button-row">
                <button type="button" onClick={closeDialog}>Done</button>
              </div>
            </>
          )}

          {dialog.kind === 'hostDetails' && (
            <>
              <h2 id="dialog-title">{dialog.details.name}</h2>
              <dl className="details-grid">
                <dt>Address</dt>
                <dd>{dialog.details.address || 'Unknown'}</dd>
                <dt>Status</dt>
                <dd>{dialog.details.status}</dd>
                <dt>Paired</dt>
                <dd>{dialog.details.paired ? 'Yes' : 'No'}</dd>
                <dt>Running</dt>
                <dd>{dialog.details.running ? 'Yes' : 'No'}</dd>
                <dt>Server version</dt>
                <dd>{dialog.details.serverVersion || 'Unknown'}</dd>
              </dl>
              <div className="button-row">
                <button type="button" onClick={closeDialog}>Close</button>
              </div>
            </>
          )}

          {dialog.kind === 'help' && (
            <>
              <h2 id="dialog-title">Moonlight help</h2>
              <p>This Tauri shell currently exercises the native bridge for host/app/session/settings flows.</p>
              <p>
                Use Hosts to refresh discovery, add/pair/wake/test machines, and open Apps. Use Settings to edit the
                streaming snapshot exposed by the native helper. Controller actions move focus and activate the selected
                control.
              </p>
              <h3>System</h3>
              {systemInfo ? (
                <>
                  <dl className="details-grid">
                    <dt>Version</dt>
                    <dd>{systemInfo.version || 'Unknown'}</dd>
                    <dt>Architecture</dt>
                    <dd>{systemInfo.friendlyNativeArchName || 'Unknown'}</dd>
                    <dt>Hardware acceleration</dt>
                    <dd>{systemInfo.hasHardwareAcceleration ? 'Available' : 'Not detected'}</dd>
                    <dt>HDR support</dt>
                    <dd>{systemInfo.supportsHdr ? 'Available' : 'Not detected'}</dd>
                    <dt>Maximum resolution</dt>
                    <dd>
                      {systemInfo.maximumResolutionWidth > 0 && systemInfo.maximumResolutionHeight > 0
                        ? `${systemInfo.maximumResolutionWidth}x${systemInfo.maximumResolutionHeight}`
                        : 'Unknown'}
                    </dd>
                    <dt>Desktop session</dt>
                    <dd>
                      {systemInfo.isRunningWayland ? 'Wayland' : systemInfo.isRunningXWayland ? 'XWayland' : 'Native/X11/Windows'}
                    </dd>
                    <dt>Browser integration</dt>
                    <dd>{systemInfo.hasBrowser ? 'Available' : 'Not detected'}</dd>
                    <dt>Unmapped gamepads</dt>
                    <dd>{systemInfo.unmappedGamepads || 'None detected'}</dd>
                  </dl>
                  {systemInfo.displays.length > 0 && (
                    <div className="display-list">
                      {systemInfo.displays.map((display, index) => (
                        <span key={`${display.nativeWidth}-${display.nativeHeight}-${index}`}>
                          Display {index + 1}: {display.nativeWidth}x{display.nativeHeight}
                          {display.refreshRate > 0 && ` @ ${display.refreshRate} Hz`}
                        </span>
                      ))}
                    </div>
                  )}
                </>
              ) : (
                <p>Loading native system information...</p>
              )}
              <div className="button-row">
                <button type="button" onClick={() => {
                  void openExternalUrl(setupGuideUrl, 'setup guide');
                }}>
                  Setup Guide
                </button>
                <button type="button" onClick={() => {
                  void openExternalUrl(discordUrl, 'Discord');
                }}>
                  Discord
                </button>
                <button type="button" onClick={() => {
                  void openExternalUrl(hardwareDecodingHelpUrl, 'hardware decoding help');
                }}>
                  Hardware Help
                </button>
                <button type="button" onClick={() => {
                  void openExternalUrl(gamepadMappingHelpUrl, 'gamepad mapping help');
                }}>
                  Gamepad Help
                </button>
                <button type="button" onClick={() => {
                  void loadSystemInfo();
                }}>
                  Refresh System Info
                </button>
                <button type="button" onClick={closeDialog}>Close</button>
              </div>
            </>
          )}
        </section>
      </div>
    );
  };

  return (
    <main className="shell">
      <header className="toolbar">
        <div>
          <h1>Moonlight</h1>
          <p>Tauri + React migration shell</p>
        </div>
        <nav aria-label="Primary">
          <button type="button" onClick={() => setPage('hosts')}>Hosts</button>
          <button type="button" onClick={loadSettings}>Settings</button>
          <button type="button" onClick={openHelpDialog}>Help</button>
        </nav>
      </header>

      <section className="controller-test" aria-label="Controller navigation test">
        <span>Controller event test</span>
        {controllerTestActions.map((action) => (
          <button key={action} type="button" onClick={() => {
            void bridge.emitControllerAction(action);
          }}>
            {action}
          </button>
        ))}
      </section>

      {updateInfo && (
        <section className="update-banner" aria-label="Moonlight update available">
          <div>
            <h2>Update available</h2>
            <p>{updateInfo.message}</p>
          </div>
          <div className="button-row">
            {updateInfo.url && (
              <button type="button" onClick={() => {
                void openExternalUrl(updateInfo.url, 'update download');
              }}>
                Download {updateInfo.version || 'update'}
              </button>
            )}
            <button type="button" onClick={() => setUpdateInfo(null)}>Dismiss</button>
          </div>
        </section>
      )}

      {streamState.phase !== 'idle' && (
        <section className={`stream-panel ${streamState.phase}`} aria-label="Native stream state">
          <div>
            <h2>Native stream</h2>
            <p>
              {streamState.appName || 'Session'}
              {streamState.hostName && ` on ${streamState.hostName}`}
            </p>
          </div>
          <div className="stream-status">
            <span className="tag">{streamState.phase}</span>
            <span>{streamState.message}</span>
            {streamState.uiHiddenRequested && <span>Native requested the Tauri shell to hide while streaming.</span>}
          </div>
          {(streamState.warnings.length > 0 || streamState.errors.length > 0) && (
            <div className="stream-messages">
              {streamState.warnings.map((warning, index) => (
                <span key={`warning-${index}`}>{warning}</span>
              ))}
              {streamState.errors.map((error, index) => (
                <span key={`error-${index}`} className="error">{error}</span>
              ))}
            </div>
          )}
          <div className="button-row">
            <button type="button" onClick={showTauriShell}>Show Shell</button>
            <button type="button" onClick={quitRunningApp}>Quit Stream</button>
            {(streamState.phase === 'finished' || streamState.phase === 'error') && (
              <button type="button" onClick={() => setStreamState(idleStreamState)}>Dismiss</button>
            )}
          </div>
        </section>
      )}

      {page === 'hosts' && (
        <section className="panel" aria-labelledby="hosts-title">
          <div className="panel-heading">
            <h2 id="hosts-title">Hosts</h2>
            <div className="button-row">
              <button type="button" onClick={refreshHosts}>Refresh</button>
              <button type="button" onClick={openAddHostDialog}>Add Host</button>
            </div>
          </div>
          <div className="backend-diagnostics" aria-label="Backend diagnostics">
            <span>Backend: {backendInfo?.mode ?? 'unknown'}</span>
            {backendInfo?.helperPath && <span>Helper: {backendInfo.helperPath}</span>}
            <span>Last host count: {hostRefreshDiagnostics.lastCount}</span>
            <span>Refresh attempts: {hostRefreshDiagnostics.attempts}</span>
            {hostRefreshDiagnostics.lastError && <span>Error: {hostRefreshDiagnostics.lastError}</span>}
          </div>
          {hosts.length === 0 && (
            <div className="empty-state">
              <h3>No hosts found yet</h3>
              <p>
                Moonlight is polling saved hosts and listening for Sunshine via mDNS. If this stays empty, confirm the
                Tauri shell was started with the IPC helper environment variables and that the helper path points at the
                latest built Moonlight.exe.
              </p>
              <p>
                Refresh attempts: {hostRefreshDiagnostics.attempts}; last host count: {hostRefreshDiagnostics.lastCount}
                {hostRefreshDiagnostics.lastError && `; last error: ${hostRefreshDiagnostics.lastError}`}
              </p>
            </div>
          )}
          <div className="card-grid">
            {hosts.map((host) => (
              <article key={host.id} className={`host-card ${host.id === selectedHostId ? 'selected' : ''}`}>
                <button type="button" className="card-primary" onClick={() => openApps(host.id)}>
                  <span className="title">{host.name}</span>
                  <span>{host.status}</span>
                  <span>{host.paired ? 'Paired' : 'Pairing required'}</span>
                  {host.running && <span className="tag">In Game</span>}
                </button>
                <div className="card-actions">
                  <button type="button" onClick={() => pairHost(host)}>Pair</button>
                  {host.running && <button type="button" onClick={() => resumeSession(host)}>Resume</button>}
                  <button type="button" onClick={() => runHostCommand(() => bridge.wakeHost(host.id))}>Wake</button>
                  <button type="button" onClick={() => showDetails(host)}>Details</button>
                  <button type="button" onClick={() => testNetwork(host)}>Test</button>
                  <button type="button" onClick={() => openRenameHostDialog(host)}>Rename</button>
                  <button type="button" onClick={() => openDeleteHostDialog(host)}>Delete</button>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}

      {page === 'apps' && (
        <section className="panel" aria-labelledby="apps-title">
          <div className="panel-heading">
            <div>
              <h2 id="apps-title">{selectedHost?.name ?? 'Apps'}</h2>
              <p>Launch, quit, hide, and direct-launch actions map to native commands.</p>
            </div>
            <div className="button-row">
              <button type="button" onClick={() => {
                const nextShowHidden = !showHiddenApps;
                setShowHiddenApps(nextShowHidden);
                void refreshApps(selectedHostId, nextShowHidden);
              }}>
                {showHiddenApps ? 'Hide Hidden Apps' : 'View All Apps'}
              </button>
              <button type="button" onClick={quitRunningApp}>Quit Running App</button>
              <button type="button" onClick={() => setPage('hosts')}>Back</button>
            </div>
          </div>
          <div className="card-grid">
            {apps.map((app) => (
              <article key={app.id} className="app-card">
                <button type="button" className="card-primary" onClick={() => launchApp(app)}>
                  <span className="title">{app.name}</span>
                  <span>{app.running ? 'Running' : 'Ready'}</span>
                  {app.directLaunch && <span className="tag">Direct Launch</span>}
                  {app.hidden && <span className="tag muted">Hidden</span>}
                </button>
                <div className="card-actions">
                  <button type="button" onClick={() => launchApp(app)}>Launch</button>
                  <button type="button" onClick={() => toggleDirectLaunch(app)}>
                    {app.directLaunch ? 'Clear Direct Launch' : 'Direct Launch'}
                  </button>
                  <button type="button" onClick={() => toggleHidden(app)}>
                    {app.hidden ? 'Unhide' : 'Hide'}
                  </button>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}

      {page === 'settings' && (
        <section className="panel settings" aria-labelledby="settings-title">
          <div className="panel-heading">
            <h2 id="settings-title">Settings</h2>
            <div className="button-row">
              <button type="button" onClick={() => setPage('hosts')}>Cancel</button>
              <button type="button" onClick={saveSettings}>Save</button>
            </div>
          </div>
          <label>
            Width
            <input value={settings.width} type="number" onChange={(event) => setSettings({ ...settings, width: Number(event.target.value) })} />
          </label>
          <label>
            Height
            <input value={settings.height} type="number" onChange={(event) => setSettings({ ...settings, height: Number(event.target.value) })} />
          </label>
          <label>
            FPS
            <input value={settings.fps} type="number" onChange={(event) => setSettings({ ...settings, fps: Number(event.target.value) })} />
          </label>
          <label>
            Bitrate
            <input value={settings.bitrateKbps} type="number" onChange={(event) => setSettings({ ...settings, bitrateKbps: Number(event.target.value) })} />
          </label>
          <label className="checkbox">
            <input checked={settings.enableHdr} type="checkbox" onChange={(event) => setSettings({ ...settings, enableHdr: event.target.checked })} />
            HDR
          </label>
          <label className="checkbox">
            <input checked={settings.gamepadMouse} type="checkbox" onChange={(event) => setSettings({ ...settings, gamepadMouse: event.target.checked })} />
            Gamepad mouse
          </label>
        </section>
      )}

      {renderDialog()}

      {eventLog.length > 0 && (
        <aside className="event-log" aria-label="Native event log">
          {eventLog.map((event, index) => (
            <p key={`${event.kind}-${index}`}>
              <strong>{event.kind}</strong>
              {': '}
              {event.controllerAction ?? event.message}
            </p>
          ))}
        </aside>
      )}

      <footer className="status" role="status">{status}</footer>
    </main>
  );
}
