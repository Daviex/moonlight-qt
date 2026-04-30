import { HostEntry } from '../bridge';

interface HostCardProps {
  host: HostEntry;
  selected: boolean;
  pairEnabled: boolean;
  wakeEnabled: boolean;
  onOpenApps: (host: HostEntry) => void;
  onPair: (host: HostEntry) => void;
  onResume: (host: HostEntry) => void;
  onWake: (host: HostEntry) => void;
  onDetails: (host: HostEntry) => void;
  onTestNetwork: (host: HostEntry) => void;
  onRename: (host: HostEntry) => void;
  onDelete: (host: HostEntry) => void;
}

export function HostCard({
  host,
  selected,
  pairEnabled,
  wakeEnabled,
  onOpenApps,
  onPair,
  onResume,
  onWake,
  onDetails,
  onTestNetwork,
  onRename,
  onDelete,
}: HostCardProps) {
  const stateLabel = host.running ? 'Streaming now' : host.paired ? 'Ready to play' : 'Needs pairing';

  return (
    <article
      className={`host-card ${selected ? 'selected' : ''}`}
      data-controller-card="true"
    >
      <button
        type="button"
        className="card-primary"
        data-controller-focus={selected ? 'true' : undefined}
        onClick={() => onOpenApps(host)}
      >
        <span className="host-orb" aria-hidden="true">{host.name.slice(0, 1).toUpperCase()}</span>
        <span className="card-copy">
          <span className="eyebrow">{stateLabel}</span>
          <span className="title">{host.name}</span>
          <span className="card-subtitle">{host.status}</span>
        </span>
        <span className="tag-row">
          <span className={host.paired ? 'tag' : 'tag warning'}>{host.paired ? 'Paired' : 'Pairing required'}</span>
          {host.running && <span className="tag">In Game</span>}
          {host.wakeable && <span className="tag">Wakeable</span>}
          {!host.serverSupported && <span className="tag muted">Unsupported Server</span>}
        </span>
      </button>
      <div className="card-actions">
        <button type="button" disabled={!pairEnabled} onClick={() => onPair(host)}>Pair</button>
        {host.running && <button type="button" onClick={() => onResume(host)}>Resume</button>}
        <button type="button" disabled={!wakeEnabled} onClick={() => onWake(host)}>Wake</button>
        <button type="button" onClick={() => onDetails(host)}>Details</button>
        <button type="button" onClick={() => onTestNetwork(host)}>Test</button>
        <button type="button" onClick={() => onRename(host)}>Rename</button>
        <button type="button" onClick={() => onDelete(host)}>Delete</button>
      </div>
    </article>
  );
}
