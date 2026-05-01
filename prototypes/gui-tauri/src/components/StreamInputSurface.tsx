import { useCallback, useEffect, useRef } from 'react';
import type { PointerEvent } from 'react';
import { bridge } from '../bridge';
import { StreamUiState } from '../ui/types';

interface StreamInputSurfaceProps {
  streamState: StreamUiState;
}

type StreamMouseButton = 'left' | 'middle' | 'right' | 'x1' | 'x2';

const int16Min = -32768;
const int16Max = 32767;
const buttonFlags = [
  0x1000,
  0x2000,
  0x4000,
  0x8000,
  0x0100,
  0x0200,
  0,
  0,
  0x0020,
  0x0010,
  0x0040,
  0x0080,
  0x0001,
  0x0002,
  0x0004,
  0x0008,
  0x0400,
];

function clampInt16(value: number) {
  return Math.max(int16Min, Math.min(int16Max, Math.round(value)));
}

function mouseButton(button: number): StreamMouseButton | null {
  switch (button) {
  case 0:
    return 'left';
  case 1:
    return 'middle';
  case 2:
    return 'right';
  case 3:
    return 'x1';
  case 4:
    return 'x2';
  default:
    return null;
  }
}

function shouldCapture(streamState: StreamUiState) {
  return streamState.phase === 'launching' || streamState.phase === 'active';
}

function axisToInt16(value: number | undefined) {
  return clampInt16((value ?? 0) * int16Max);
}

function triggerToByte(button: GamepadButton | undefined) {
  return Math.max(0, Math.min(255, Math.round((button?.value ?? 0) * 255)));
}

function gamepadFlags(gamepad: Gamepad) {
  return gamepad.buttons.reduce((flags, button, index) => {
    const flag = buttonFlags[index] ?? 0;
    return button.pressed ? flags | flag : flags;
  }, 0);
}

export function StreamInputSurface({ streamState }: StreamInputSurfaceProps) {
  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const active = shouldCapture(streamState);

  useEffect(() => {
    if (!active) {
      return;
    }
    surfaceRef.current?.focus();
  }, [active]);

  useEffect(() => {
    if (!active) {
      return;
    }

    let animationFrame = 0;
    const pollGamepads = () => {
      for (const gamepad of navigator.getGamepads?.() ?? []) {
        if (!gamepad) {
          continue;
        }

        const controllerNumber = Math.min(gamepad.index, 3);
        void bridge.streamController({
          controllerNumber,
          activeGamepadMask: 1 << controllerNumber,
          buttonFlags: gamepadFlags(gamepad),
          leftTrigger: triggerToByte(gamepad.buttons[6]),
          rightTrigger: triggerToByte(gamepad.buttons[7]),
          leftStickX: axisToInt16(gamepad.axes[0]),
          leftStickY: axisToInt16(gamepad.axes[1]),
          rightStickX: axisToInt16(gamepad.axes[2]),
          rightStickY: axisToInt16(gamepad.axes[3]),
        }).catch(() => undefined);
      }
      animationFrame = window.requestAnimationFrame(pollGamepads);
    };

    animationFrame = window.requestAnimationFrame(pollGamepads);
    return () => window.cancelAnimationFrame(animationFrame);
  }, [active]);

  const sendPointerInput = useCallback((event: PointerEvent<HTMLDivElement>) => {
    const target = event.currentTarget;
    const rect = target.getBoundingClientRect();
    if (event.pointerType !== 'touch' && event.pointerType !== 'pen') {
      void bridge.streamMouseMove(clampInt16(event.movementX), clampInt16(event.movementY)).catch(() => undefined);
      return;
    }

    const x = clampInt16(event.clientX - rect.left);
    const y = clampInt16(event.clientY - rect.top);
    const referenceWidth = clampInt16(rect.width);
    const referenceHeight = clampInt16(rect.height);
    void bridge.streamMousePosition(x, y, referenceWidth, referenceHeight).catch(() => undefined);
  }, []);

  if (!active) {
    return null;
  }

  return (
    <div
      ref={surfaceRef}
      className="stream-input-surface"
      tabIndex={0}
      role="application"
      aria-label="Stream input capture surface"
      onContextMenu={(event) => event.preventDefault()}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        event.currentTarget.focus();
        sendPointerInput(event);
        const button = mouseButton(event.button);
        if (button) {
          void bridge.streamMouseButton(button, true).catch(() => undefined);
        }
      }}
      onPointerMove={sendPointerInput}
      onPointerUp={(event) => {
        sendPointerInput(event);
        const button = mouseButton(event.button);
        if (button) {
          void bridge.streamMouseButton(button, false).catch(() => undefined);
        }
      }}
      onWheel={(event) => {
        event.preventDefault();
        void bridge.streamScroll(clampInt16(event.deltaX), clampInt16(event.deltaY)).catch(() => undefined);
      }}
      onKeyDown={(event) => {
        event.preventDefault();
        void bridge.streamKeyboard(
          clampInt16(event.keyCode),
          true,
          event.shiftKey,
          event.ctrlKey,
          event.altKey,
          event.metaKey,
          true,
        ).catch(() => undefined);
        if (event.key.length === 1 && !event.ctrlKey && !event.altKey && !event.metaKey) {
          void bridge.streamText(event.key).catch(() => undefined);
        }
      }}
      onKeyUp={(event) => {
        event.preventDefault();
        void bridge.streamKeyboard(
          clampInt16(event.keyCode),
          false,
          event.shiftKey,
          event.ctrlKey,
          event.altKey,
          event.metaKey,
          true,
        ).catch(() => undefined);
      }}
    >
      <span>Stream input capture active</span>
      <small>Keyboard, mouse, and wheel events are forwarded to the Rust GameStream input bridge.</small>
    </div>
  );
}
