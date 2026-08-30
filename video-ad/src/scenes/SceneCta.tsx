import React from 'react';
import {
  AbsoluteFill,
  Easing,
  Img,
  interpolate,
  staticFile,
  useCurrentFrame,
} from 'remotion';
import {T, FONT_DISPLAY, FONT_MONO} from '../theme';
import {Sweep} from '../primitives';

// Beat 5 (frames 375–450): brand lockup, verified fact chips, repo CTA.

const CHIPS = ['~15 MB', 'No Electron', 'No telemetry', 'MIT', 'macOS'];
const CTA_IN = 10;

// Soft radial darkening at edges for cinematic focus.
const Vignette: React.FC = () => (
  <div
    style={{
      position: 'absolute',
      inset: 0,
      background:
        'radial-gradient(ellipse at center, transparent 55%, rgba(0,0,0,0.5) 100%)',
      pointerEvents: 'none',
    }}
  />
);

export const SceneCta: React.FC = () => {
  const frame = useCurrentFrame();
  const icon = interpolate(frame, [0, 14], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const name = interpolate(frame, [6, 22], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const chips = interpolate(frame, [CTA_IN + 8, CTA_IN + 24], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const cta = interpolate(frame, [CTA_IN + 20, CTA_IN + 36], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });

  return (
    <AbsoluteFill style={{background: T.bg}}>
      <div
        style={{
          position: 'absolute',
          inset: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 30,
        }}
      >
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            gap: 18,
            opacity: icon,
            transform: `translateY(${(1 - icon) * -18}px)`,
          }}
        >
          <Img
            src={staticFile('icon.png')}
            width={132}
            height={132}
            style={{borderRadius: 28, boxShadow: '0 12px 40px rgba(0,0,0,0.5)'}}
            alt="Athena's Core icon"
          />
          <div
            style={{
              fontFamily: FONT_DISPLAY,
              fontWeight: 800,
              fontSize: 64,
              letterSpacing: -1,
              color: T.text,
              opacity: name,
              transform: `translateY(${(1 - name) * 12}px)`,
            }}
          >
            Athena’s Core
          </div>
        </div>

        {/* fact chips */}
        <div
          style={{
            display: 'flex',
            gap: 12,
            opacity: chips,
            transform: `translateY(${(1 - chips) * 12}px)`,
          }}
        >
          {CHIPS.map((chip, i) => (
            <span
              key={chip}
              style={{
                fontFamily: FONT_MONO,
                fontSize: 17,
                color: T.textMuted,
                border: `1px solid ${T.border}`,
                background: T.bgSecondary,
                borderRadius: 999,
                padding: '7px 16px',
                transform: `translateY(${
                  (1 -
                    interpolate(frame, [CTA_IN + 8 + i * 3, CTA_IN + 22 + i * 3], [0, 1], {
                      extrapolateLeft: 'clamp',
                      extrapolateRight: 'clamp',
                      easing: Easing.out(Easing.cubic),
                    })) *
                  8
                }px)`,
              }}
            >
              {chip}
            </span>
          ))}
        </div>

        {/* CTA line */}
        <div
          style={{
            position: 'relative',
            fontFamily: FONT_MONO,
            fontSize: 27,
            color: T.text,
            opacity: cta,
            padding: '16px 34px',
            borderRadius: 14,
            border: `1.5px solid ${T.accent}`,
            background: 'rgba(201,162,75,0.08)',
            overflow: 'hidden',
          }}
        >
          <span style={{color: T.textMuted}}>Free &amp; open source — </span>
          <span style={{color: T.accent}}>github.com/TOX9C/athenas-core</span>
          <Sweep delay={CTA_IN + 38} duration={26} />
        </div>
      </div>
      <Vignette />
    </AbsoluteFill>
  );
};
