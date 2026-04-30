import { AppEntry } from '../bridge';
import { appInitials, boxArtSrc } from '../ui/artwork';

interface AppCardProps {
  app: AppEntry;
  preferredFocus: boolean;
  onLaunch: (app: AppEntry) => void;
  onToggleDirectLaunch: (app: AppEntry) => void;
  onToggleHidden: (app: AppEntry) => void;
}

export function AppCard({
  app,
  preferredFocus,
  onLaunch,
  onToggleDirectLaunch,
  onToggleHidden,
}: AppCardProps) {
  const imageSrc = boxArtSrc(app);

  return (
    <article className="app-card" data-controller-card="true">
      <button
        type="button"
        className="card-primary"
        data-controller-focus={preferredFocus ? 'true' : undefined}
        onClick={() => onLaunch(app)}
      >
        <span className="app-art" aria-hidden="true">
          {imageSrc ? (
            <img src={imageSrc} alt="" />
          ) : (
            <span>{appInitials(app.name)}</span>
          )}
        </span>
        <span className="card-copy">
          <span className="eyebrow">{app.running ? 'Now running' : 'Ready to launch'}</span>
          <span className="title">{app.name}</span>
        </span>
        <span className="tag-row">
          {app.directLaunch && <span className="tag">Direct Launch</span>}
          {app.appCollectorGame && <span className="tag">App Collector</span>}
          {app.hidden && <span className="tag muted">Hidden</span>}
        </span>
      </button>
      <div className="card-actions">
        <button type="button" onClick={() => onLaunch(app)}>Launch</button>
        <button type="button" onClick={() => onToggleDirectLaunch(app)}>
          {app.directLaunch ? 'Clear Direct Launch' : 'Direct Launch'}
        </button>
        <button type="button" onClick={() => onToggleHidden(app)}>
          {app.hidden ? 'Unhide' : 'Hide'}
        </button>
      </div>
    </article>
  );
}
