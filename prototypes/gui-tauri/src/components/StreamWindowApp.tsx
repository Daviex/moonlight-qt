import { useEffect, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { bridge, StreamWindowDescriptor } from '../bridge';
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
  const [message, setMessage] = useState('Preparing Rust stream window...');

  useEffect(() => {
    let cancelled = false;
    const loadDescriptor = async () => {
      try {
        const activeWindow = await bridge.activeStreamWindow();
        if (cancelled) {
          return;
        }
        setDescriptor(activeWindow);
        if (activeWindow) {
          setMessage(`${activeWindow.title} is ready for stream input capture.`);
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
    return () => {
      cancelled = true;
    };
  }, []);

  const streamState = streamWindowState(descriptor, message);

  return (
    <main className="stream-window-root">
      <section className="stream-window-status" aria-label="Rust stream window status">
        <h1>{descriptor?.title ?? 'Moonlight Stream'}</h1>
        <p>{message}</p>
        <small>{streamPhaseHelp(streamState.phase)}</small>
      </section>
      <StreamInputSurface streamState={streamState} />
    </main>
  );
}
