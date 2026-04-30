import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  AppEntry,
  BackendInfo,
  BridgeEvent,
  ControllerAction,
  HostEntry,
  StreamingSettings,
  SystemInfo,
  bridge,
} from './bridge';
import {
  audioConfigOptions,
  captureSysKeysOptions,
  controllerTestActions,
  discordUrl,
  fallbackSettings,
  gamepadMappingHelpUrl,
  hardwareDecodingHelpUrl,
  languageOptions,
  numericSettingRules,
  NumericSettingKey,
  setupGuideUrl,
  uiDisplayModeOptions,
  videoCodecOptions,
  videoDecoderOptions,
  windowModeOptions,
} from './ui/constants';
import { DialogState, Page, StreamPhase, StreamUiState, UpdateInfo } from './ui/types';
import { normalizeSettings, validateSettings } from './ui/settings';
import { idleStreamState } from './ui/stream';
import {
  activeDialogRoot,
  focusCardActions,
  focusPage,
  focusPreferredElement,
  focusableElements,
  moveFocus,
} from './ui/navigation';
import { AppsPage } from './components/AppsPage';
import { HostsPage } from './components/HostsPage';
import { StreamPanel } from './components/StreamPanel';
import { applyStoredTheme, readStoredTheme, themeOptions, UiTheme } from './ui/theme';
import { canPairHost } from './ui/hosts';
import { applyStoredDebugUi, readStoredDebugUi } from './ui/debug';

const appWindow = getCurrentWindow();

function writeDebugLog(message: string) {
  void bridge.debugLog(message).catch(() => undefined);
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
  const [theme, setTheme] = useState<UiTheme>(() => readStoredTheme());
  const [showDebugInfo, setShowDebugInfo] = useState(() => readStoredDebugUi());
  const dialogOpenerRef = useRef<HTMLElement | null>(null);
  const previousDialogKindRef = useRef<DialogState['kind']>('none');
  const [hostRefreshDiagnostics, setHostRefreshDiagnostics] = useState({
    attempts: 0,
    lastCount: 0,
    lastError: '',
  });

  const selectedHost = useMemo(
    () => hosts.find((host) => host.id === selectedHostId),
    [hosts, selectedHostId],
  );
  const settingsErrors = useMemo(() => validateSettings(settings), [settings]);
  const unmappedGamepads = systemInfo?.unmappedGamepads.trim() ?? '';
  const showControllerTest = showDebugInfo && backendInfo?.mode === 'mock';
  const canQuitStream = streamState.phase === 'launching' ||
    streamState.phase === 'active' ||
    streamState.phase === 'quitting' ||
    streamState.phase === 'cleanup';

  const updateSetting = useCallback(<K extends keyof StreamingSettings,>(key: K, value: StreamingSettings[K]) => {
    setSettings((currentSettings) => ({ ...currentSettings, [key]: value }));
  }, []);

  const updateNumericSetting = useCallback((key: NumericSettingKey, value: number) => {
    setSettings((currentSettings) => ({ ...currentSettings, [key]: Number.isFinite(value) ? value : NaN }));
  }, []);

  const updateNumericSettingFromInput = useCallback((key: NumericSettingKey, value: string) => {
    updateNumericSetting(key, value === '' ? NaN : Number(value));
  }, [updateNumericSetting]);

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

  const openApps = useCallback(async (host: HostEntry) => {
    setSelectedHostId(host.id);
    setPage('apps');

    if (!host.paired) {
      setApps([]);
      setStatus(`${host.name} must be paired before Moonlight can load apps.`);
      return;
    }

    await refreshApps(host.id, showHiddenApps);
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
      writeDebugLog(`submitAddHost begin; address=${address}`);
      const result = await bridge.addHost(address);
      writeDebugLog(`submitAddHost success; message=${result.message}`);
      setStatus(result.message);
      setDialog({ kind: 'none' });
      await refreshHosts();
    }
    catch (error) {
      const message = String(error);
      writeDebugLog(`submitAddHost failed; error=${message}`);
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
    if (!canPairHost(host)) {
      if (host.paired) {
        setStatus(`${host.name} is already paired.`);
      }
      else if (host.status !== 'Online') {
        setStatus(`${host.name} must be online before pairing.`);
      }
      else {
        setStatus(`${host.name} reports an unsupported server version.`);
      }
      return;
    }

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

  const applyDefaultBitrate = useCallback(async () => {
    try {
      const normalizedSettings = normalizeSettings(settings);
      if (settings.width !== normalizedSettings.width ||
          settings.height !== normalizedSettings.height ||
          settings.fps !== normalizedSettings.fps) {
        setSettings(normalizedSettings);
        setStatus('Resolution and FPS were adjusted to valid ranges before calculating the default bitrate.');
      }

      const bitrateKbps = await bridge.defaultBitrate(
        normalizedSettings.width,
        normalizedSettings.height,
        normalizedSettings.fps,
        normalizedSettings.enableYUV444,
      );
      setSettings((currentSettings) => ({
        ...currentSettings,
        width: normalizedSettings.width,
        height: normalizedSettings.height,
        fps: normalizedSettings.fps,
        bitrateKbps,
        autoAdjustBitrate: true,
      }));
      setStatus(`Default bitrate set to ${(bitrateKbps / 1000).toFixed(1)} Mbps.`);
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [settings]);

  const saveSettings = useCallback(async () => {
    const validationErrors = validateSettings(settings);
    if (validationErrors.length > 0) {
      setStatus(`Fix settings before saving: ${validationErrors[0]}`);
      return;
    }

    try {
      const normalizedSettings = normalizeSettings(settings);
      const result = await bridge.saveSettings(normalizedSettings);
      setSettings(normalizedSettings);
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

  const dismissStreamState = useCallback(() => {
    setStreamState(idleStreamState);
    void refreshHosts();
    if (selectedHostId) {
      void refreshApps(selectedHostId, showHiddenApps);
    }
  }, [refreshApps, refreshHosts, selectedHostId, showHiddenApps]);

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

      if (message.includes('cleanup completed')) {
        return {
          ...nextState,
          phase: 'finished',
          message,
          uiHiddenRequested: false,
        };
      }

      if (message.includes('finished')) {
        return {
          ...nextState,
          phase: 'cleanup',
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
      if (!focusCardActions()) {
        setStatus('Focus a host or app card before opening card actions.');
      }
      break;
    }
  }, [closeDialog, dialog.kind, loadSettings, page]);

  const handleRefreshShortcut = useCallback(() => {
    if (page === 'apps' && selectedHostId) {
      void refreshApps(selectedHostId, showHiddenApps);
      return;
    }

    if (page === 'settings') {
      void loadSettings();
      return;
    }

    void refreshHosts();
  }, [loadSettings, page, refreshApps, refreshHosts, selectedHostId, showHiddenApps]);

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

      if (event.kind === 'hostChanged' && /pairing completed/i.test(event.message)) {
        setDialog((currentDialog) => currentDialog.kind === 'pairing' ? { kind: 'none' } : currentDialog);
      }

      if (event.kind === 'status' && /pair/i.test(event.message)) {
        setDialog((currentDialog) => currentDialog.kind === 'pairing'
          ? {
            ...currentDialog,
            challenge: {
              ...currentDialog.challenge,
              message: event.message,
            },
          }
          : currentDialog);
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
    applyStoredTheme(theme);
  }, [theme]);

  useEffect(() => {
    applyStoredDebugUi(showDebugInfo);
  }, [showDebugInfo]);

  useEffect(() => {
    if (dialog.kind !== 'none') {
      return undefined;
    }

    const focusTimer = window.setTimeout(() => {
      focusPage(page);
    }, 0);

    return () => window.clearTimeout(focusTimer);
  }, [page]);

  useEffect(() => {
    const previousDialogKind = previousDialogKindRef.current;
    previousDialogKindRef.current = dialog.kind;

    if (previousDialogKind === 'none' && dialog.kind !== 'none') {
      dialogOpenerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      const focusTimer = window.setTimeout(() => {
        focusPreferredElement(activeDialogRoot() ?? document);
      }, 0);

      return () => window.clearTimeout(focusTimer);
    }

    if (previousDialogKind !== 'none' && dialog.kind === 'none') {
      const focusTimer = window.setTimeout(() => {
        const opener = dialogOpenerRef.current;
        if (opener?.isConnected) {
          opener.focus();
        }
        else {
          focusPage(page);
        }
      }, 0);

      return () => window.clearTimeout(focusTimer);
    }

    return undefined;
  }, [dialog.kind, page]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      const editingText = target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement;

      if (event.key === 'Escape') {
        event.preventDefault();
        handleControllerAction('back');
        return;
      }

      if (editingText) {
        return;
      }

      if (event.key === 'F5') {
        event.preventDefault();
        handleRefreshShortcut();
        return;
      }

      if ((event.ctrlKey || event.metaKey) && event.key === ',') {
        event.preventDefault();
        void loadSettings();
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [handleControllerAction, handleRefreshShortcut, loadSettings]);

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
                <dt>Wakeable</dt>
                <dd>{dialog.details.wakeable ? 'Yes' : 'No'}</dd>
                <dt>Server supported</dt>
                <dd>{dialog.details.serverSupported ? 'Yes' : 'No'}</dd>
                <dt>Server version</dt>
                <dd>{dialog.details.serverVersion || 'Unknown'}</dd>
                <dt>App version</dt>
                <dd>{dialog.details.appVersion || 'Unknown'}</dd>
                <dt>GFE version</dt>
                <dd>{dialog.details.gfeVersion || 'Unknown'}</dd>
                <dt>GPU</dt>
                <dd>{dialog.details.gpuModel || 'Unknown'}</dd>
                <dt>UUID</dt>
                <dd>{dialog.details.uuid || 'Unknown'}</dd>
                <dt>Local address</dt>
                <dd>{dialog.details.localAddress || 'Unknown'}</dd>
                <dt>Remote address</dt>
                <dd>{dialog.details.remoteAddress || 'Unknown'}</dd>
                <dt>IPv6 address</dt>
                <dd>{dialog.details.ipv6Address || 'Unknown'}</dd>
                <dt>Manual address</dt>
                <dd>{dialog.details.manualAddress || 'Unknown'}</dd>
                <dt>MAC address</dt>
                <dd>{dialog.details.macAddress || 'Unknown'}</dd>
                <dt>Pair state</dt>
                <dd>{dialog.details.pairState || 'Unknown'}</dd>
                <dt>Running game ID</dt>
                <dd>{dialog.details.runningGameId || 'None'}</dd>
                <dt>HTTPS port</dt>
                <dd>{dialog.details.httpsPort || 'Unknown'}</dd>
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
              <p>
                Keyboard shortcuts: Escape goes back or closes dialogs, F5 refreshes the current page, and Ctrl+Comma
                opens Settings.
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
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true">M</div>
          <div>
            <span className="eyebrow">Moonlight Tauri</span>
            <h1>Moonlight</h1>
            <p>Pick a PC, open its library, and start streaming.</p>
          </div>
        </div>
        <nav aria-label="Primary">
          <button type="button" className={page === 'hosts' ? 'nav-active' : undefined} onClick={() => setPage('hosts')}>Hosts</button>
          <button type="button" className={page === 'settings' ? 'nav-active' : undefined} onClick={loadSettings}>Settings</button>
          <button type="button" onClick={openHelpDialog}>Help</button>
          <label className="theme-picker">
            Theme
            <select value={theme} onChange={(event) => setTheme(event.target.value as UiTheme)}>
              {themeOptions.map((option) => (
                <option key={option.value} value={option.value}>{option.label}</option>
              ))}
            </select>
          </label>
        </nav>
      </header>

      {showControllerTest && (
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
      )}

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

      {unmappedGamepads && (
        <section className="update-banner warning" aria-label="Unmapped gamepads detected">
          <div>
            <h2>Gamepad mapping needed</h2>
            <p>Moonlight detected unmapped gamepads: {unmappedGamepads}</p>
          </div>
          <div className="button-row">
            <button type="button" onClick={() => {
              void openExternalUrl(gamepadMappingHelpUrl, 'gamepad mapping help');
            }}>
              Gamepad Help
            </button>
            <button type="button" onClick={() => {
              void loadSystemInfo();
            }}>
              Refresh
            </button>
          </div>
        </section>
      )}

      <StreamPanel
        streamState={streamState}
        canQuitStream={canQuitStream}
        onShowShell={showTauriShell}
        onQuitStream={quitRunningApp}
        onReturnToHosts={() => setPage('hosts')}
        onDismiss={dismissStreamState}
      />

      {page === 'hosts' && (
        <HostsPage
          backendInfo={backendInfo}
          diagnostics={hostRefreshDiagnostics}
          hosts={hosts}
          selectedHostId={selectedHostId}
          showDebugInfo={showDebugInfo}
          onRefreshHosts={refreshHosts}
          onAddHost={openAddHostDialog}
          onOpenApps={openApps}
          onPair={pairHost}
          onResume={resumeSession}
          onWake={(hostToWake) => {
            void runHostCommand(() => bridge.wakeHost(hostToWake.id));
          }}
          onDetails={showDetails}
          onTestNetwork={testNetwork}
          onRename={openRenameHostDialog}
          onDelete={openDeleteHostDialog}
        />
      )}

      {page === 'apps' && (
        <AppsPage
          apps={apps}
          selectedHost={selectedHost}
          selectedHostId={selectedHostId}
          showHiddenApps={showHiddenApps}
          onSetShowHiddenApps={setShowHiddenApps}
          onRefreshApps={(hostId, includeHidden) => {
            void refreshApps(hostId, includeHidden);
          }}
          onQuitRunningApp={quitRunningApp}
          onSetPage={setPage}
          onPair={pairHost}
          onLaunch={launchApp}
          onToggleDirectLaunch={toggleDirectLaunch}
          onToggleHidden={toggleHidden}
        />
      )}

      {page === 'settings' && (
        <section className="panel settings-panel" aria-labelledby="settings-title" data-page-panel="settings">
          <div className="panel-heading">
            <div>
              <span className="eyebrow">Preferences</span>
              <h2 id="settings-title">Settings</h2>
              <p>Grouped for quick controller navigation. Advanced switches stay visible, Debug controls diagnostics.</p>
            </div>
            <div className="button-row">
              <button type="button" onClick={() => setPage('hosts')}>Cancel</button>
              <button type="button" onClick={saveSettings} disabled={settingsErrors.length > 0}>Save</button>
            </div>
          </div>
          {settingsErrors.length > 0 && (
            <div className="settings-errors" role="alert">
              {settingsErrors.map((error) => <span key={error}>{error}</span>)}
            </div>
          )}
          <div className="settings-layout">
            <section className="settings-group">
              <span className="eyebrow">Display</span>
              <h3>Stream canvas</h3>
          <label>
            Width
            <input value={Number.isNaN(settings.width) ? '' : settings.width} type="number" min={numericSettingRules.width.min} max={numericSettingRules.width.max} data-controller-focus="true" onChange={(event) => updateNumericSettingFromInput('width', event.target.value)} />
          </label>
          <label>
            Height
            <input value={Number.isNaN(settings.height) ? '' : settings.height} type="number" min={numericSettingRules.height.min} max={numericSettingRules.height.max} onChange={(event) => updateNumericSettingFromInput('height', event.target.value)} />
          </label>
          <label>
            FPS
            <input value={Number.isNaN(settings.fps) ? '' : settings.fps} type="number" min={numericSettingRules.fps.min} max={numericSettingRules.fps.max} onChange={(event) => updateNumericSettingFromInput('fps', event.target.value)} />
          </label>
          <label>
            Bitrate (Kbps)
            <input value={Number.isNaN(settings.bitrateKbps) ? '' : settings.bitrateKbps} type="number" min={numericSettingRules.bitrateKbps.min} max={numericSettingRules.bitrateKbps.max} onChange={(event) => updateNumericSettingFromInput('bitrateKbps', event.target.value)} />
          </label>
          <div className="setting-action">
            <button type="button" onClick={applyDefaultBitrate}>Use Default Bitrate</button>
          </div>
          <label>
            Packet size
            <input value={Number.isNaN(settings.packetSize) ? '' : settings.packetSize} type="number" min={numericSettingRules.packetSize.min} max={numericSettingRules.packetSize.max} onChange={(event) => updateNumericSettingFromInput('packetSize', event.target.value)} />
          </label>
            </section>

            <section className="settings-group">
              <span className="eyebrow">Audio & video</span>
              <h3>Quality pipeline</h3>
          <label>
            Audio
            <select value={settings.audioConfig} onChange={(event) => updateSetting('audioConfig', Number(event.target.value))}>
              {audioConfigOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label>
            Video codec
            <select value={settings.videoCodecConfig} onChange={(event) => updateSetting('videoCodecConfig', Number(event.target.value))}>
              {videoCodecOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label>
            Video decoder
            <select value={settings.videoDecoderSelection} onChange={(event) => updateSetting('videoDecoderSelection', Number(event.target.value))}>
              {videoDecoderOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label className="checkbox">
            <input checked={settings.enableHdr} type="checkbox" onChange={(event) => updateSetting('enableHdr', event.target.checked)} />
            HDR
          </label>
          <label className="checkbox">
            <input checked={settings.enableYUV444} type="checkbox" onChange={(event) => updateSetting('enableYUV444', event.target.checked)} />
            YUV 4:4:4
          </label>
            </section>

            <section className="settings-group">
              <span className="eyebrow">Window & system</span>
              <h3>Shell behavior</h3>
          <label>
            Stream window mode
            <select value={settings.windowMode} onChange={(event) => updateSetting('windowMode', Number(event.target.value))}>
              {windowModeOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label>
            UI startup mode
            <select value={settings.uiDisplayMode} onChange={(event) => updateSetting('uiDisplayMode', Number(event.target.value))}>
              {uiDisplayModeOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label>
            Language
            <select value={settings.language} onChange={(event) => updateSetting('language', Number(event.target.value))}>
              {languageOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label>
            Capture system keys
            <select value={settings.captureSysKeysMode} onChange={(event) => updateSetting('captureSysKeysMode', Number(event.target.value))}>
              {captureSysKeysOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            </select>
          </label>
          <label className="checkbox">
            <input checked={showDebugInfo} type="checkbox" onChange={(event) => setShowDebugInfo(event.target.checked)} />
            Debug
          </label>
          <label className="checkbox">
            <input checked={settings.keepAwake} type="checkbox" onChange={(event) => updateSetting('keepAwake', event.target.checked)} />
            Keep display awake
          </label>
            </section>

            <section className="settings-group">
              <span className="eyebrow">Streaming</span>
              <h3>Session behavior</h3>
          <label className="checkbox">
            <input checked={settings.unlockBitrate} type="checkbox" onChange={(event) => updateSetting('unlockBitrate', event.target.checked)} />
            Unlock bitrate limit
          </label>
          <label className="checkbox">
            <input checked={settings.autoAdjustBitrate} type="checkbox" onChange={(event) => {
              updateSetting('autoAdjustBitrate', event.target.checked);
              if (event.target.checked) {
                void applyDefaultBitrate();
              }
            }} />
            Automatically adjust bitrate
          </label>
          <label className="checkbox">
            <input checked={settings.enableVsync} type="checkbox" onChange={(event) => updateSetting('enableVsync', event.target.checked)} />
            V-Sync
          </label>
          <label className="checkbox">
            <input checked={settings.gameOptimizations} type="checkbox" onChange={(event) => updateSetting('gameOptimizations', event.target.checked)} />
            Optimize game settings
          </label>
          <label className="checkbox">
            <input checked={settings.playAudioOnHost} type="checkbox" onChange={(event) => updateSetting('playAudioOnHost', event.target.checked)} />
            Play audio on host
          </label>
          <label className="checkbox">
            <input checked={settings.enableMdns} type="checkbox" onChange={(event) => updateSetting('enableMdns', event.target.checked)} />
            mDNS discovery
          </label>
          <label className="checkbox">
            <input checked={settings.quitAppAfter} type="checkbox" onChange={(event) => updateSetting('quitAppAfter', event.target.checked)} />
            Quit app after stream
          </label>
            </section>

            <section className="settings-group">
              <span className="eyebrow">Input</span>
              <h3>Controller & pointer</h3>
          <label className="checkbox">
            <input checked={settings.multiController} type="checkbox" onChange={(event) => updateSetting('multiController', event.target.checked)} />
            Multiple controllers
          </label>
          <label className="checkbox">
            <input checked={settings.absoluteMouseMode} type="checkbox" onChange={(event) => updateSetting('absoluteMouseMode', event.target.checked)} />
            Absolute mouse mode
          </label>
          <label className="checkbox">
            <input checked={settings.absoluteTouchMode} type="checkbox" onChange={(event) => updateSetting('absoluteTouchMode', event.target.checked)} />
            Absolute touch mode
          </label>
          <label className="checkbox">
            <input checked={settings.gamepadMouse} type="checkbox" onChange={(event) => updateSetting('gamepadMouse', event.target.checked)} />
            Gamepad mouse
          </label>
          <label className="checkbox">
            <input checked={settings.swapMouseButtons} type="checkbox" onChange={(event) => updateSetting('swapMouseButtons', event.target.checked)} />
            Swap mouse buttons
          </label>
          <label className="checkbox">
            <input checked={settings.backgroundGamepad} type="checkbox" onChange={(event) => updateSetting('backgroundGamepad', event.target.checked)} />
            Background gamepad input
          </label>
          <label className="checkbox">
            <input checked={settings.reverseScrollDirection} type="checkbox" onChange={(event) => updateSetting('reverseScrollDirection', event.target.checked)} />
            Reverse scroll direction
          </label>
          <label className="checkbox">
            <input checked={settings.swapFaceButtons} type="checkbox" onChange={(event) => updateSetting('swapFaceButtons', event.target.checked)} />
            Swap controller face buttons
          </label>
          <label className="checkbox">
            <input checked={settings.framePacing} type="checkbox" onChange={(event) => updateSetting('framePacing', event.target.checked)} />
            Frame pacing
          </label>
            </section>

            <section className="settings-group">
              <span className="eyebrow">Advanced</span>
              <h3>Warnings & integrations</h3>
          <label className="checkbox">
            <input checked={settings.connectionWarnings} type="checkbox" onChange={(event) => updateSetting('connectionWarnings', event.target.checked)} />
            Connection warnings
          </label>
          <label className="checkbox">
            <input checked={settings.configurationWarnings} type="checkbox" onChange={(event) => updateSetting('configurationWarnings', event.target.checked)} />
            Configuration warnings
          </label>
          <label className="checkbox">
            <input checked={settings.richPresence} type="checkbox" onChange={(event) => updateSetting('richPresence', event.target.checked)} />
            Discord rich presence
          </label>
          <label className="checkbox">
            <input checked={settings.detectNetworkBlocking} type="checkbox" onChange={(event) => updateSetting('detectNetworkBlocking', event.target.checked)} />
            Detect network blocking
          </label>
          <label className="checkbox">
            <input checked={settings.showPerformanceOverlay} type="checkbox" onChange={(event) => updateSetting('showPerformanceOverlay', event.target.checked)} />
            Performance overlay
          </label>
          <label className="checkbox">
            <input checked={settings.muteOnFocusLoss} type="checkbox" onChange={(event) => updateSetting('muteOnFocusLoss', event.target.checked)} />
            Mute on focus loss
          </label>
            </section>
          </div>
        </section>
      )}

      {renderDialog()}

      {showDebugInfo && eventLog.length > 0 && (
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
