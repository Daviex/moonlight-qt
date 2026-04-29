import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AppEntry,
  BridgeEvent,
  ControllerAction,
  HostEntry,
  StreamingSettings,
  bridge,
} from './bridge';

type Page = 'hosts' | 'apps' | 'settings';

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

function focusableElements(): HTMLElement[] {
  return Array.from(
    document.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'),
  ).filter((element) =>
    !element.hasAttribute('disabled') &&
    element.tabIndex !== -1 &&
    element.offsetParent !== null,
  );
}

function moveFocus(delta: number) {
  const elements = focusableElements();
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

  const selectedHost = useMemo(
    () => hosts.find((host) => host.id === selectedHostId),
    [hosts, selectedHostId],
  );

  const refreshHosts = useCallback(async () => {
    try {
      const nextHosts = await bridge.listHosts();
      setHosts(nextHosts);
      if (!nextHosts.some((host) => host.id === selectedHostId)) {
        setSelectedHostId(nextHosts[0]?.id ?? '');
      }
      setStatus('Host list refreshed.');
    }
    catch (error) {
      setStatus(`Failed to refresh hosts: ${String(error)}`);
    }
  }, [selectedHostId]);

  const refreshApps = useCallback(async (hostId = selectedHostId, includeHidden = showHiddenApps) => {
    if (!hostId) {
      setApps([]);
      return;
    }

    try {
      setApps(await bridge.listApps(hostId, includeHidden));
      setStatus('App list loaded.');
    }
    catch (error) {
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

  const addHost = useCallback(async () => {
    const address = window.prompt('Enter host address');
    if (!address) {
      return;
    }
    await runHostCommand(() => bridge.addHost(address));
  }, [runHostCommand]);

  const renameHost = useCallback(async (host: HostEntry) => {
    const name = window.prompt('Enter new host name', host.name);
    if (!name) {
      return;
    }
    await runHostCommand(() => bridge.renameHost(host.id, name));
  }, [runHostCommand]);

  const showDetails = useCallback(async (host: HostEntry) => {
    try {
      const details = await bridge.hostDetails(host.id);
      setStatus(`${details.name}: ${details.status}, ${details.address}, ${details.serverVersion}`);
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
      await refreshHosts();
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [refreshHosts]);

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

    try {
      const result = await bridge.launchApp(selectedHostId, app.id);
      setStatus(result.message);
      await refreshApps(selectedHostId, showHiddenApps);
      await refreshHosts();
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [refreshApps, refreshHosts, selectedHostId, showHiddenApps]);

  const quitRunningApp = useCallback(async () => {
    if (!selectedHostId) {
      return;
    }

    try {
      const result = await bridge.quitRunningApp(selectedHostId);
      setStatus(result.message);
      await refreshApps(selectedHostId, showHiddenApps);
      await refreshHosts();
    }
    catch (error) {
      setStatus(String(error));
    }
  }, [refreshApps, refreshHosts, selectedHostId, showHiddenApps]);

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
      const activeElement = document.activeElement;
      if (activeElement instanceof HTMLElement) {
        activeElement.click();
      }
      else {
        focusableElements()[0]?.focus();
      }
      break;
    }
    case 'back':
      if (page === 'apps' || page === 'settings') {
        setPage('hosts');
      }
      else {
        setStatus('Back requested from the host page.');
      }
      break;
    case 'settings':
      void loadSettings();
      break;
    case 'contextMenu':
      setStatus('Context menu requested by controller navigation.');
      break;
    }
  }, [loadSettings, page]);

  const handleBridgeEvent = useCallback((event: BridgeEvent) => {
    setEventLog((previousEvents) => [event, ...previousEvents].slice(0, 6));
    void (async () => {
      if (event.kind === 'controllerAction' && event.controllerAction) {
        handleControllerAction(event.controllerAction);
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
  }, [handleControllerAction, refreshApps, refreshHosts, refreshSettingsSnapshot, selectedHostId, showHiddenApps]);

  useEffect(() => {
    void refreshHosts();
  }, [refreshHosts]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    bridge.listen(handleBridgeEvent)
      .then((nextUnlisten) => {
        if (disposed) {
          nextUnlisten();
        }
        else {
          unlisten = nextUnlisten;
        }
      })
      .catch((error) => {
        setStatus(`Failed to subscribe to native events: ${String(error)}`);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [handleBridgeEvent]);

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
          <button type="button" onClick={() => setStatus('Help will open Moonlight docs from the native bridge.')}>Help</button>
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

      {page === 'hosts' && (
        <section className="panel" aria-labelledby="hosts-title">
          <div className="panel-heading">
            <h2 id="hosts-title">Hosts</h2>
            <div className="button-row">
              <button type="button" onClick={refreshHosts}>Refresh</button>
              <button type="button" onClick={addHost}>Add Host</button>
            </div>
          </div>
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
                  <button type="button" onClick={() => runHostCommand(() => bridge.wakeHost(host.id))}>Wake</button>
                  <button type="button" onClick={() => showDetails(host)}>Details</button>
                  <button type="button" onClick={() => testNetwork(host)}>Test</button>
                  <button type="button" onClick={() => renameHost(host)}>Rename</button>
                  <button type="button" onClick={() => runHostCommand(() => bridge.deleteHost(host.id))}>Delete</button>
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
