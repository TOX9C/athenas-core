import React from 'react';
import {AbsoluteFill, Easing, interpolate, useCurrentFrame} from 'remotion';
import {T, W, H, FONT_DISPLAY, FONT_MONO} from '../theme';
const DISPLAY = 'Hanken Grotesk';

// Beat 1 (frames 0–75): scattered single-purpose app windows converge into
// one gold-hairlined window. Headline lands the claim.

type Rect = {x: number; y: number; w: number; h: number};

const SCATTER: Rect[] = [
  {x: 150, y: 130, w: 420, h: 260},
  {x: 1350, y: 170, w: 420, h: 260},
  {x: 210, y: 640, w: 420, h: 260},
  {x: 1290, y: 680, w: 420, h: 260},
];

// Quadrants of the converged 960×540 window centered on screen.
const MERGED: Rect[] = [
  {x: 480, y: 293, w: 478, h: 245},
  {x: 962, y: 293, w: 478, h: 245},
  {x: 480, y: 542, w: 478, h: 245},
  {x: 962, y: 542, w: 478, h: 245},
];

const LABELS = ['Terminal', 'Athena AI', 'Task Board', 'Agent Team'];

const CONVERGE_START = 16;
const CONVERGE_END = 46;
const CHROME_DRAW = 46;
const HEADLINE_IN = 56;

const GhostWindow: React.FC<{
  index: number;
}> = ({index}) => {
  const frame = useCurrentFrame();
  const appear = interpolate(frame, [index * 2, index * 2 + 10], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  const p = interpolate(frame, [CONVERGE_START, CONVERGE_END], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.inOut(Easing.cubic),
  });
  const s = SCATTER[index];
  const m = MERGED[index];
  const x = s.x + (m.x - s.x) * p;
  const y = s.y + (m.y - s.y) * p;
  const w = s.w + (m.w - s.w) * p;
  const h = s.h + (m.h - s.h) * p;
  // Once merged, the pane's own border dissolves into the unified frame.
  const ownBorder = 1 - interpolate(frame, [CHROME_DRAW, CHROME_DRAW + 10], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  return (
    <div
      style={{
        position: 'absolute',
        left: x,
        top: y,
        width: w,
        height: h,
        borderRadius: 12,
        background: T.bgSecondary,
        border: `1px solid ${T.border}`,
        boxShadow: '0 24px 64px rgba(0,0,0,0.42)',
        opacity: appear,
        overflow: 'hidden',
      }}
    >
      <div
        style={{
          height: 34,
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '0 12px',
          borderBottom: `1px solid ${T.border}`,
          background: T.bgTertiary,
        }}
      >
        <div
          style={{
            width: 9,
            height: 9,
            borderRadius: '50%',
            background: T.accent,
            opacity: 0.85,
          }}
        />
        <span
          style={{
            fontFamily: DISPLAY,
            fontSize: 15,
            fontWeight: 600,
            color: T.textMuted,
          }}
        >
          {LABELS[index]}
        </span>
      </div>
      {/* hint of content */}
      <div style={{padding: '12px 14px', display: 'grid', gap: 8}}>
        {[0.9, 0.65, 0.4].map((wd, i) => (
          <div
            key={i}
            style={{
              height: 8,
              width: `${wd * 100 * ownBorder}%`,
              borderRadius: 4,
              background: T.bgHover,
            }}
          />
        ))}
      </div>
    </div>
  );
};

export const SceneHook: React.FC = () => {
  const frame = useCurrentFrame();
  const chromeOpacity = interpolate(frame, [CHROME_DRAW, CHROME_DRAW + 12], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  const headline = interpolate(frame, [HEADLINE_IN, HEADLINE_IN + 16], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const headlineY = interpolate(frame, [HEADLINE_IN, HEADLINE_IN + 16], [24, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const kicker = interpolate(frame, [6, 18], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  return (
    <AbsoluteFill>
      {/* kicker */}
      <div
        style={{
          position: 'absolute',
          top: 96,
          width: W,
          textAlign: 'center',
          fontFamily: FONT_MONO,
          fontSize: 22,
          letterSpacing: 10,
          color: T.textDim,
          opacity: kicker,
        }}
      >
        ATHENA’S CORE
      </div>

      {SCATTER.map((_, i) => (
        <GhostWindow key={i} index={i} />
      ))}

      {/* unified gold-hairline frame over merged quadrants */}
      <div
        style={{
          position: 'absolute',
          left: 480,
          top: 270,
          width: 960,
          height: 540,
          borderRadius: 14,
          border: `1.5px solid ${T.accent}`,
          boxShadow: `0 0 40px rgba(201,162,75,${0.22 * chromeOpacity}), 0 24px 64px rgba(0,0,0,0.42)`,
          opacity: chromeOpacity,
        }}
      >
        <div
          style={{
            height: 44,
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            padding: '0 16px',
            borderBottom: `1px solid ${T.border}`,
            opacity: chromeOpacity,
          }}
        >
          {LABELS.map((label) => (
            <span
              key={label}
              style={{
                fontFamily: DISPLAY,
                fontSize: 16,
                fontWeight: 600,
                color: T.textMuted,
                padding: '4px 12px',
                borderRadius: 999,
                background: T.bgTertiary,
                border: `1px solid ${T.border}`,
              }}
            >
              {label}
            </span>
          ))}
        </div>
      </div>

      {/* headline */}
      <div
        style={{
          position: 'absolute',
          top: 866 + headlineY,
          width: W,
          textAlign: 'center',
          fontFamily: DISPLAY,
          fontWeight: 800,
          fontSize: 74,
          letterSpacing: -1.5,
          color: T.text,
          opacity: headline,
        }}
      >
        Five apps.{' '}
        <span style={{color: T.accent}}>One window.</span>
      </div>
    </AbsoluteFill>
  );
};
