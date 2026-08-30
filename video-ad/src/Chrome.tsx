import React from 'react';
import {interpolate, useCurrentFrame} from 'remotion';
import {T} from './theme';

export const TITLEBAR_H = 46;
// macOS-style app window shell shared by every UI scene so the ad reads as
// one continuous app surface.
export const Chrome: React.FC<{title?: string}> = ({
  title = 'Athena’s Core — ~/athena',
}) => {
  const frame = useCurrentFrame();
  const draw = interpolate(frame, [0, 12], [0, 1], {
    extrapolateRight: 'clamp',
  });
  return (
    <div
      style={{
        position: 'absolute',
        inset: '56px 96px',
        borderRadius: 14,
        background: T.bgSecondary,
        border: `1px solid ${T.border}`,
        boxShadow:
          '0 2px 8px rgba(0,0,0,0.24), 0 8px 24px rgba(0,0,0,0.18), 0 24px 64px rgba(0,0,0,0.42)',
        overflow: 'hidden',
        opacity: draw,
      }}
    >
      {/* Titlebar */}
      <div
        style={{
          height: TITLEBAR_H,
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '0 16px',
          borderBottom: `1px solid ${T.border}`,
          background: T.bgTertiary,
        }}
      >
        <TrafficLight color="#5a5a60" />
        <TrafficLight color="#4a4741" />
        <TrafficLight color="#44503c" />
        <div
          style={{
            marginLeft: 14,
            fontFamily: "'JetBrains Mono', ui-monospace, Menlo, monospace",
            fontSize: 17,
            color: T.textMuted,
            letterSpacing: 0.2,
          }}
        >
          {title}
        </div>
        <div
          style={{
            marginLeft: 'auto',
            width: 11,
            height: 11,
            borderRadius: '50%',
            background: T.accent,
            opacity: 0.55 + 0.45 * Math.abs(Math.sin(frame * 0.09)),
          }}
        />
      </div>
    </div>
  );
};

// Absolutely-positioned slot matching the window's client area, so scenes
// can lay out content over Chrome without threading children through it.
export const WindowBody: React.FC<{
  children: React.ReactNode;
  pad?: number;
}> = ({children, pad = 20}) => (
  <div
    style={{
      position: 'absolute',
      top: 56 + TITLEBAR_H + 1,
      left: 96 + 1,
      right: 96 + 1,
      bottom: 56 + 1,
      padding: pad,
    }}
  >
    {children}
  </div>
);

const TrafficLight: React.FC<{color: string}> = ({color}) => (
  <div
    style={{
      width: 13,
      height: 13,
      borderRadius: '50%',
      background: color,
      opacity: 0.9,
      flexShrink: 0,
    }}
  />
);
