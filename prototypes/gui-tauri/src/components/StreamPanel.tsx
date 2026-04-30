import { StreamUiState } from '../ui/types';
import { streamPhaseHelp, streamPhaseLabel } from '../ui/stream';

interface StreamPanelProps {
  streamState: StreamUiState;
  canQuitStream: boolean;
  onShowShell: () => void;
  onQuitStream: () => void;
  onReturnToHosts: () => void;
  onDismiss: () => void;
}

export function StreamPanel({
  streamState,
  canQuitStream,
  onShowShell,
  onQuitStream,
  onReturnToHosts,
  onDismiss,
}: StreamPanelProps) {
  if (streamState.phase === 'idle') {
    return null;
  }

  return (
    <section className={`stream-panel ${streamState.phase}`} aria-label="Native stream state">
      <div>
        <h2>Native stream - {streamPhaseLabel(streamState.phase)}</h2>
        <p>
          {streamState.appName || 'Session'}
          {streamState.hostName && ` on ${streamState.hostName}`}
        </p>
        <p>{streamPhaseHelp(streamState.phase)}</p>
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
        <button type="button" onClick={onShowShell}>Show Shell</button>
        <button type="button" onClick={onQuitStream} disabled={!canQuitStream}>Quit Stream</button>
        <button type="button" onClick={onReturnToHosts}>Return to Hosts</button>
        {(streamState.phase === 'finished' || streamState.phase === 'error') && (
          <button type="button" onClick={onDismiss}>Dismiss</button>
        )}
      </div>
    </section>
  );
}
