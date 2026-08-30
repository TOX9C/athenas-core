import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';
import {T, FPS} from './theme';

// ---- StatusDot: per-pane activity dot (signature app motif) -------------
export const StatusDot: React.FC<{
  size?: number;
  color?: string;
  opacity?: number;
  glow?: boolean;
}> = ({size = 12, color = T.accent, opacity = 1, glow = false}) => (
  <div
    style={{
      width: size,
      height: size,
      borderRadius: '50%',
      background: color,
      opacity,
      flexShrink: 0,
      boxShadow: glow ? `0 0 ${size * 1.5}px ${color}` : 'none',
    }}
  />
);

// ---- TypedText: typewriter with block cursor ---------------------------
export const TypedText: React.FC<{
  text: string;
  startFrame: number;
  cps?: number; // chars per second
  style?: React.CSSProperties;
  cursorColor?: string;
  hideCursorWhenDone?: boolean;
}> = ({
  text,
  startFrame,
  cps = 18,
  style,
  cursorColor = T.accent,
  hideCursorWhenDone = true,
}) => {
  const frame = useCurrentFrame();
  const elapsed = frame - startFrame;
  const chars =
    elapsed <= 0 ? 0 : Math.min(text.length, Math.floor(elapsed * (cps / FPS)));
  const shown = text.slice(0, chars);
  const done = chars >= text.length;
  return (
    <span style={{...style, whiteSpace: 'pre-wrap'}}>
      {shown}
      <span
        style={{
          display: 'inline-block',
          width: '0.55em',
          height: '1.05em',
          background: cursorColor,
          verticalAlign: 'text-bottom',
          marginLeft: 2,
          opacity: done && hideCursorWhenDone ? 0 : 1,
        }}
      />
    </span>
  );
};

// ---- Sweep: the app's signature lit-sweep pass --------------------------
export const Sweep: React.FC<{delay?: number; duration?: number}> = ({
  delay = 0,
  duration = 30,
}) => {
  const frame = useCurrentFrame();
  const p = interpolate(frame - delay, [0, duration], [0, 100], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  return (
    <div
      style={{
        position: 'absolute',
        inset: 0,
        overflow: 'hidden',
        pointerEvents: 'none',
      }}
    >
      <div
        style={{
          position: 'absolute',
          top: 0,
          bottom: 0,
          width: 280,
          left: `${p * 120 - 20}%`,
          background:
            'linear-gradient(105deg, transparent, rgba(201,162,75,0.14) 45%, rgba(226,194,108,0.26) 50%, rgba(201,162,75,0.14) 55%, transparent)',
        }}
      />
    </div>
  );
};

// ---- Fade wrapper -------------------------------------------------------
export const Fade: React.FC<{
  inFrames?: number;
  outFrames?: number;
  durationInFrames: number;
  children: React.ReactNode;
}> = ({inFrames = 0, outFrames = 0, durationInFrames, children}) => {
  const frame = useCurrentFrame();
  const opacity = interpolate(
    frame,
    [0, inFrames, durationInFrames - outFrames, durationInFrames],
    [0, 1, 1, 0],
    {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'},
  );
  return <AbsoluteFill style={{opacity}}>{children}</AbsoluteFill>;
};

export const frameOf = (seconds: number) => Math.round(seconds * FPS);
