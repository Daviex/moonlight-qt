import { useEffect, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { ActiveStreamSession, bridge, StreamMediaStats, StreamWindowDescriptor } from '../bridge';
import { idleStreamState, streamPhaseHelp } from '../ui/stream';
import { StreamUiState } from '../ui/types';
import { StreamInputSurface } from './StreamInputSurface';

function streamWindowState(descriptor: StreamWindowDescriptor | null, message: string): StreamUiState {
  if (!descriptor) {
    return {
      ...idleStreamState,
      phase: 'launching',
      message,
    };
  }

  return {
    ...idleStreamState,
    phase: 'active',
    message,
  };
}

export function StreamWindowApp() {
  const [descriptor, setDescriptor] = useState<StreamWindowDescriptor | null>(null);
  const [session, setSession] = useState<ActiveStreamSession | null>(null);
  const [mediaStats, setMediaStats] = useState<StreamMediaStats | null>(null);
  const [message, setMessage] = useState('Preparing Rust stream window...');

  useEffect(() => {
    let cancelled = false;
    const loadDescriptor = async () => {
      try {
        const activeSession = await bridge.activeStreamSession();
        const activeWindow = activeSession?.window ?? await bridge.activeStreamWindow();
        if (cancelled) {
          return;
        }
        setSession(activeSession);
        setDescriptor(activeWindow);
        if (activeWindow) {
          setMessage(`${activeSession?.appName ?? activeWindow.title} is ready for stream input capture.`);
          await getCurrentWebviewWindow().setTitle(activeWindow.title);
        } else {
          setMessage('Waiting for an active Rust stream session.');
        }
      } catch (error) {
        if (!cancelled) {
          setMessage(`Unable to load stream window descriptor: ${String(error)}`);
        }
      }
    };

    void loadDescriptor();
    const unlistenPromise = bridge.listen((event) => {
      if (event.kind === 'sessionChanged' || event.kind === 'status') {
        setMessage(event.message);
      }
      if (
        event.kind === 'sessionChanged' &&
        (event.message.includes('cleanup completed') ||
          event.message.includes('finished') ||
          event.message.includes('UI can be shown'))
      ) {
        void getCurrentWebviewWindow().close();
      }
    });

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && session?.hostId) {
        event.preventDefault();
        void bridge.quitRunningApp(session.hostId).catch((error) => {
          setMessage(`Unable to stop stream: ${String(error)}`);
        });
      }
    };
    window.addEventListener('keydown', onKeyDown);
    const statsInterval = window.setInterval(() => {
      void bridge.streamMediaStats()
        .then(setMediaStats)
        .catch(() => undefined);
    }, 1000);

    return () => {
      cancelled = true;
      window.removeEventListener('keydown', onKeyDown);
      window.clearInterval(statsInterval);
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [session?.hostId]);

  const streamState = streamWindowState(descriptor, message);

  return (
    <main className="stream-window-root">
      <section className="stream-window-status" aria-label="Rust stream window status">
        <h1>{descriptor?.title ?? 'Moonlight Stream'}</h1>
        <p>{message}</p>
        <small>{streamPhaseHelp(streamState.phase)}</small>
        {mediaStats && (
          <small>
            Video: {mediaStats.videoFrames} frames / {mediaStats.videoBytes} bytes
            {' - '}
            Audio: {mediaStats.audioPackets} packets / {mediaStats.audioBytes} bytes
          </small>
        )}
        {session && (
          <button
            className="danger"
            type="button"
            onClick={() => {
              void bridge.quitRunningApp(session.hostId).catch((error) => {
                setMessage(`Unable to stop stream: ${String(error)}`);
              });
            }}
          >
            Stop Stream
          </button>
        )}
      </section>
      <StreamInputSurface streamState={streamState} />
    </main>
  );
}
