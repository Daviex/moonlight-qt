import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

type Page = 'hosts' | 'apps' | 'settings';

interface HostEntry {
  id: string;
  name: string;
  status: string;
  paired: boolean;
  running: boolean;
}

interface AppEntry {
  id: string;
  name: string;
  hidden: boolean;
  directLaunch: boolean;
  running: boolean;
}

interface StreamingSettings {
  width: number;
  height: number;
  fps: number;
  bitrateKbps: number;
  enableHdr: boolean;
  gamepadMouse: boolean;
}

const fallbackHosts: HostEntry[] = [
  { id: 'gaming-pc', name: 'Gaming PC', status: 'Online', paired: true, running: false },
  { id: 'living-room', name: 'Living Room PC', status: 'Offline', paired: true, running: false },
  { id: 'new-host', name: 'New Host', status: 'Pairing required', paired: false, running: false },
];

const fallbackApps: AppEntry[] = [
  { id: 'steam', name: 'Steam Big Picture', hidden: false, directLaunch: true, running: false },
  { id: 'desktop', name: 'Desktop', hidden: false, directLaunch: false, running: false },
  { id: 'game', name: 'Example Game', hidden: false, directLaunch: false, running: false },
];

const fallbackSettings: StreamingSettings = {
  width: 1920,
  height: 1080,
  fps: 60,
  bitrateKbps: 20000,
  enableHdr: false,
  gamepadMouse: true,
};

async function callBackend<T>(command: string, args?: Record<string, unknown>, fallback?: T): Promise<T> {
  try {
    return await invoke<T>(command, args);
  }
  catch (error) {
    if (fallback !== undefined) {
      return fallback;
    }

    throw error;
  }
}

export default function App() {
  const [page, setPage] = useState<Page>('hosts');
  const [selectedHostId, setSelectedHostId] = useState<string>(fallbackHosts[0].id);
  const [hosts, setHosts] = useState<HostEntry[]>(fallbackHosts);
  const [apps, setApps] = useState<AppEntry[]>(fallbackApps);
  const [settings, setSettings] = useState<StreamingSettings>(fallbackSettings);
  const [status, setStatus] = useState('Tauri shell ready.');

  const selectedHost = useMemo(
    () => hosts.find((host) => host.id === selectedHostId) ?? hosts[0],
    [hosts, selectedHostId],
  );

  const refreshHosts = useCallback(async () => {
    const nextHosts = await callBackend<HostEntry[]>('list_hosts', undefined, fallbackHosts);
    setHosts(nextHosts);
    if (!nextHosts.some((host) => host.id === selectedHostId)) {
      setSelectedHostId(nextHosts[0]?.id ?? '');
    }
    setStatus('Host list refreshed.');
  }, [selectedHostId]);

  const openApps = useCallback(async (hostId: string) => {
    const nextApps = await callBackend<AppEntry[]>('list_apps', { hostId }, fallbackApps);
    setSelectedHostId(hostId);
    setApps(nextApps);
    setPage('apps');
    setStatus('App list loaded.');
  }, []);

  const loadSettings = useCallback(async () => {
    const nextSettings = await callBackend<StreamingSettings>('load_settings', undefined, fallbackSettings);
    setSettings(nextSettings);
    setPage('settings');
    setStatus('Settings loaded.');
  }, []);

  useEffect(() => {
    void refreshHosts();
  }, [refreshHosts]);

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

      {page === 'hosts' && (
        <section className="panel" aria-labelledby="hosts-title">
          <div className="panel-heading">
            <h2 id="hosts-title">Hosts</h2>
            <button type="button" onClick={refreshHosts}>Refresh</button>
          </div>
          <div className="card-grid">
            {hosts.map((host) => (
              <button
                key={host.id}
                type="button"
                className={`host-card ${host.id === selectedHostId ? 'selected' : ''}`}
                onClick={() => openApps(host.id)}
              >
                <span className="title">{host.name}</span>
                <span>{host.status}</span>
                <span>{host.paired ? 'Paired' : 'Pairing required'}</span>
                {host.running && <span className="tag">In Game</span>}
              </button>
            ))}
          </div>
        </section>
      )}

      {page === 'apps' && (
        <section className="panel" aria-labelledby="apps-title">
          <div className="panel-heading">
            <div>
              <h2 id="apps-title">{selectedHost?.name ?? 'Apps'}</h2>
              <p>Launch, quit, hide, and direct-launch actions will map to native commands.</p>
            </div>
            <button type="button" onClick={() => setPage('hosts')}>Back</button>
          </div>
          <div className="card-grid">
            {apps.map((app) => (
              <button
                key={app.id}
                type="button"
                className="app-card"
                onClick={() => setStatus(`Launch requested for ${app.name}.`)}
              >
                <span className="title">{app.name}</span>
                <span>{app.running ? 'Running' : 'Ready'}</span>
                {app.directLaunch && <span className="tag">Direct Launch</span>}
                {app.hidden && <span className="tag muted">Hidden</span>}
              </button>
            ))}
          </div>
        </section>
      )}

      {page === 'settings' && (
        <section className="panel settings" aria-labelledby="settings-title">
          <div className="panel-heading">
            <h2 id="settings-title">Settings</h2>
            <button type="button" onClick={() => setPage('hosts')}>Done</button>
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

      <footer className="status" role="status">{status}</footer>
    </main>
  );
}
