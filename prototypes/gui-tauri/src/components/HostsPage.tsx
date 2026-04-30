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
  return (
    <section className="panel" aria-labelledby="hosts-title" data-page-panel="hosts">
      <div className="panel-heading">
        <h2 id="hosts-title">Hosts</h2>
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
