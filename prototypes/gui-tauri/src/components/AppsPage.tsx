import { AppEntry, HostEntry } from '../bridge';
import { Page } from '../ui/types';
import { canPairHost } from '../ui/hosts';
import { AppCard } from './AppCard';

interface AppsPageProps {
  apps: AppEntry[];
  selectedHost: HostEntry | undefined;
  selectedHostId: string;
  showHiddenApps: boolean;
  onSetShowHiddenApps: (showHiddenApps: boolean) => void;
  onRefreshApps: (hostId: string, includeHidden: boolean) => void;
  onQuitRunningApp: () => void;
  onSetPage: (page: Page) => void;
  onPair: (host: HostEntry) => void;
  onLaunch: (app: AppEntry) => void;
  onToggleDirectLaunch: (app: AppEntry) => void;
  onToggleHidden: (app: AppEntry) => void;
}

export function AppsPage({
  apps,
  selectedHost,
  selectedHostId,
  showHiddenApps,
  onSetShowHiddenApps,
  onRefreshApps,
  onQuitRunningApp,
  onSetPage,
  onPair,
  onLaunch,
  onToggleDirectLaunch,
  onToggleHidden,
}: AppsPageProps) {
  const runningApp = apps.find((app) => app.running);

  return (
    <section className="panel library-panel" aria-labelledby="apps-title" data-page-panel="apps">
      <div className="dashboard-hero library-hero">
        <div>
          <span className="eyebrow">Game library</span>
          <h2 id="apps-title">{selectedHost?.name ?? 'Apps'}</h2>
          <p>
            {selectedHost
              ? `${apps.length} visible apps${runningApp ? ` - ${runningApp.name} is running` : ''}`
              : 'Select a host before opening the library.'}
          </p>
        </div>
        <div className="hero-metrics" aria-label="Library summary">
          <span><strong>{apps.length}</strong> Apps</span>
          <span><strong>{showHiddenApps ? 'All' : 'Visible'}</strong> Filter</span>
          <span><strong>{runningApp ? 'Live' : 'Idle'}</strong> Stream</span>
        </div>
      </div>
      <div className="panel-heading compact-heading">
        <div>
          <span className="eyebrow">Launch shelf</span>
          <h3>Pick a game</h3>
        </div>
        <div className="button-row">
          <button type="button" onClick={() => {
            const nextShowHidden = !showHiddenApps;
            onSetShowHiddenApps(nextShowHidden);
            onRefreshApps(selectedHostId, nextShowHidden);
          }}>
            {showHiddenApps ? 'Hide Hidden Apps' : 'View All Apps'}
          </button>
          <button type="button" onClick={onQuitRunningApp}>Quit Running App</button>
          <button type="button" onClick={() => onSetPage('hosts')}>Back</button>
        </div>
      </div>
      {apps.length === 0 && (
        <div className="empty-state">
          <h3>
            {selectedHost
              ? selectedHost.paired ? 'No apps returned' : 'Pair this host to load apps'
              : 'No host selected'}
          </h3>
          {selectedHost ? (
            selectedHost.paired ? (
              <p>
                The native helper did not return any visible apps for this host. Try refreshing, show hidden apps,
                or confirm the host is online and Sunshine is returning an app list.
              </p>
            ) : (
              <p>
                {selectedHost.name} is not paired yet. Pair it from here or return to Hosts before trying to load
                apps.
              </p>
            )
          ) : (
            <p>Select a host before loading apps.</p>
          )}
          {selectedHost && (
            <p>
              Host status: {selectedHost.status}; paired: {selectedHost.paired ? 'yes' : 'no'};
              server supported: {selectedHost.serverSupported ? 'yes' : 'no'}.
            </p>
          )}
          <div className="button-row">
            {selectedHost && !selectedHost.paired && (
              <button type="button" disabled={!canPairHost(selectedHost)} onClick={() => onPair(selectedHost)}>
                Pair Host
              </button>
            )}
            {selectedHost?.paired && (
              <button type="button" onClick={() => onRefreshApps(selectedHost.id, showHiddenApps)}>
                Refresh Apps
              </button>
            )}
            <button type="button" onClick={() => onSetPage('hosts')}>Back to Hosts</button>
          </div>
        </div>
      )}
      <div className="card-grid">
        {apps.map((app, index) => (
          <AppCard
            key={app.id}
            app={app}
            preferredFocus={index === 0}
            onLaunch={onLaunch}
            onToggleDirectLaunch={onToggleDirectLaunch}
            onToggleHidden={onToggleHidden}
          />
        ))}
      </div>
    </section>
  );
}
