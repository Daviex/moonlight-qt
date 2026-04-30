import { BackendInfo, HostEntry } from '../bridge';
import { canPairHost, canWakeHost } from '../ui/hosts';
import { HostCard } from './HostCard';

interface HostRefreshDiagnostics {
  attempts: number;
  lastCount: number;
  lastError: string;
}

interface HostsPageProps {
  backendInfo: BackendInfo | null;
  diagnostics: HostRefreshDiagnostics;
  hosts: HostEntry[];
  selectedHostId: string;
  showDebugInfo: boolean;
  onRefreshHosts: () => void;
  onAddHost: () => void;
  onOpenApps: (host: HostEntry) => void;
  onPair: (host: HostEntry) => void;
  onResume: (host: HostEntry) => void;
  onWake: (host: HostEntry) => void;
  onDetails: (host: HostEntry) => void;
  onTestNetwork: (host: HostEntry) => void;
  onRename: (host: HostEntry) => void;
  onDelete: (host: HostEntry) => void;
}

export function HostsPage({
  backendInfo,
  diagnostics,
  hosts,
  selectedHostId,
  showDebugInfo,
  onRefreshHosts,
  onAddHost,
  onOpenApps,
  onPair,
  onResume,
  onWake,
  onDetails,
  onTestNetwork,
  onRename,
  onDelete,
}: HostsPageProps) {
  const selectedHost = hosts.find((host) => host.id === selectedHostId) ?? hosts[0];
  const readyHosts = hosts.filter((host) => host.paired && host.serverSupported).length;
  const heroTitle = selectedHost ? 'Ready to stream' : 'Connect a gaming PC';

  return (
    <section className="panel dashboard-panel" aria-labelledby="hosts-title" data-page-panel="hosts">
      <div className="dashboard-hero">
        <div className="hero-content">
          <span className="eyebrow">{selectedHost ? selectedHost.name : 'Moonlight setup'}</span>
          <h2 id="hosts-title">{heroTitle}</h2>
          <p>
            {selectedHost
              ? `${selectedHost.status}. ${selectedHost.paired ? 'Open the library to launch a game, or manage this host below.' : 'Pair this host before loading its library.'}`
              : 'Moonlight is looking for Sunshine hosts. Add an IP manually if discovery does not find your PC.'}
          </p>
          <div className="hero-actions">
            {selectedHost ? (
              <>
                <button type="button" onClick={() => onOpenApps(selectedHost)} data-controller-focus="true">
                  Open Library
                </button>
                {!selectedHost.paired && (
                  <button type="button" disabled={!canPairHost(selectedHost)} onClick={() => onPair(selectedHost)}>
                    Pair Host
                  </button>
                )}
                {selectedHost.running && <button type="button" onClick={() => onResume(selectedHost)}>Resume</button>}
                <button type="button" disabled={!canWakeHost(selectedHost)} onClick={() => onWake(selectedHost)}>Wake</button>
                <button type="button" onClick={() => onDetails(selectedHost)}>Details</button>
              </>
            ) : (
              <>
                <button type="button" onClick={onRefreshHosts} data-controller-focus="true">Scan Again</button>
                <button type="button" onClick={onAddHost}>Add by IP</button>
              </>
            )}
          </div>
        </div>
        <div className="hero-metrics" aria-label="Host summary">
          <span><strong>{hosts.length}</strong> Found</span>
          <span><strong>{readyHosts}</strong> Ready</span>
          <span><strong>{selectedHost?.paired ? 'Paired' : 'Setup'}</strong> State</span>
        </div>
      </div>

      <div className="panel-heading compact-heading">
        <div>
          <span className="eyebrow">Available PCs</span>
          <h3>{hosts.length === 1 ? 'Your host' : 'Choose a host'}</h3>
        </div>
        <div className="button-row">
          <button type="button" onClick={onRefreshHosts} data-controller-focus={hosts.length === 0 ? 'true' : undefined}>Refresh</button>
          <button type="button" onClick={onAddHost}>Add Host</button>
        </div>
      </div>
      {showDebugInfo && (
        <div className="backend-diagnostics" aria-label="Backend diagnostics">
          <span>Backend: {backendInfo?.mode ?? 'unknown'}</span>
          {backendInfo?.helperPath && <span>Helper: {backendInfo.helperPath}</span>}
          <span>Last host count: {diagnostics.lastCount}</span>
          <span>Refresh attempts: {diagnostics.attempts}</span>
          {diagnostics.lastError && <span>Error: {diagnostics.lastError}</span>}
        </div>
      )}
      {hosts.length === 0 && (
        <div className="empty-state">
          <h3>No hosts found yet</h3>
          <p>
            Moonlight is polling saved hosts and listening for Sunshine via mDNS. If this stays empty, confirm the
            Tauri shell was started with the IPC helper environment variables and that the helper path points at the
            latest built Moonlight.exe.
          </p>
          {showDebugInfo && (
            <p>
              Refresh attempts: {diagnostics.attempts}; last host count: {diagnostics.lastCount}
              {diagnostics.lastError && `; last error: ${diagnostics.lastError}`}
            </p>
          )}
        </div>
      )}
      <div className="card-grid">
        {hosts.map((host) => (
          <HostCard
            key={host.id}
            host={host}
            selected={host.id === selectedHostId}
            pairEnabled={canPairHost(host)}
            wakeEnabled={canWakeHost(host)}
            onOpenApps={onOpenApps}
            onPair={onPair}
            onResume={onResume}
            onWake={onWake}
            onDetails={onDetails}
            onTestNetwork={onTestNetwork}
            onRename={onRename}
            onDelete={onDelete}
          />
        ))}
      </div>
    </section>
  );
}
