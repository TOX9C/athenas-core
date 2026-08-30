import React from 'react';
import {AbsoluteFill, Easing, interpolate, useCurrentFrame} from 'remotion';
import {T, FONT_DISPLAY, FONT_MONO} from '../theme';
import {Chrome, WindowBody, TITLEBAR_H} from '../Chrome';
import {StatusDot, TypedText, Sweep} from '../primitives';

// Beat 3 (frames 180–285): Athena chat panel. Question types in, thinking
// dots pulse, streamed answer + plan checklist ticks to done.

const QUESTION = 'why is the build failing?';

const ANSWER: Array<{t: string; c: string}> = [
  {t: 'Two stale build artifacts are shadowing the new', c: T.text},
  {t: 'frontend assets. Rebuild with a clean target:', c: T.text},
  {t: '$ cargo clean -p frontend && cargo build', c: T.accent},
];

const PLAN_STEPS = [
  'Reproduce failing build',
  'Trace artifact paths',
  'Patch build script',
  'Verify green build',
] as const;

const PANEL_IN = 8;

export const SceneAthena: React.FC = () => {
  const frame = useCurrentFrame();
  const slide = interpolate(frame, [PANEL_IN, PANEL_IN + 14], [36, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const opacity = interpolate(frame, [PANEL_IN, PANEL_IN + 12], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  // Answer starts streaming at frame 40 after question completes (~f26).
  const answerStart = 44;
  const totalChars = ANSWER.reduce((n, l) => n + l.t.length + 1, 0);
  const typedChars = Math.max(
    0,
    Math.min(totalChars, Math.floor((frame - answerStart) * (30 / 30))),
  );
  let budget = typedChars;
  const rendered: React.ReactNode[] = [];
  for (let i = 0; i < ANSWER.length && budget > 0; i++) {
    const take = Math.min(ANSWER[i].t.length, budget);
    rendered.push(
      <div key={i} style={{color: ANSWER[i].c}}>
        {ANSWER[i].t.slice(0, take)}
      </div>,
    );
    budget -= ANSWER[i].t.length + 1;
  }

  return (
    <AbsoluteFill>
      <Chrome title="athenas-core — athena" />
      <WindowBody pad={24}>
        <div
          style={{
            position: 'absolute',
            inset: TITLEBAR_H + 25,
            display: 'flex',
            gap: 20,
            transform: `translateX(${slide}px)`,
            opacity,
          }}
        >
          {/* Chat column */}
          <div
            style={{
              flex: 1.35,
              display: 'flex',
              flexDirection: 'column',
              gap: 16,
              minWidth: 0,
            }}
          >
            {/* user bubble */}
            <div style={{display: 'flex', justifyContent: 'flex-end'}}>
              <div
                style={{
                  background: T.bgHover,
                  border: `1px solid ${T.border}`,
                  borderRadius: '14px 14px 4px 14px',
                  padding: '12px 18px',
                  fontFamily: FONT_DISPLAY,
                  fontSize: 21,
                  color: T.text,
                }}
              >
                <TypedText text={QUESTION} startFrame={PANEL_IN + 6} cps={22} />
              </div>
            </div>

            {/* athena reply card */}
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 10,
                background: T.bgTertiary,
                border: `1px solid ${T.border}`,
                borderLeft: `3px solid ${T.accent}`,
                borderRadius: '4px 14px 14px 4px',
                padding: '16px 20px',
                fontFamily: FONT_MONO,
                fontSize: 19,
                lineHeight: 1.7,
                minHeight: 150,
              }}
            >
              {/* thinking dots while waiting for stream */}
              {frame < answerStart ? (
                <div style={{display: 'flex', gap: 9, padding: '10px 2px'}}>
                  {[0, 1, 2].map((i) => {
                    const o =
                      0.25 +
                      0.75 *
                        Math.abs(Math.sin((frame - i * 7) * 0.18));
                    return <StatusDot key={i} size={11} opacity={Math.max(0.15, o)} />;
                  })}
                </div>
              ) : null}
              {rendered}
            </div>

            {/* input line */}
            <div
              style={{
                marginTop: 'auto',
                height: 52,
                borderRadius: 12,
                border: `1px solid ${T.border}`,
                background: T.bgSecondary,
                display: 'flex',
                alignItems: 'center',
                padding: '0 18px',
                color: T.textDim,
                fontFamily: FONT_MONO,
                fontSize: 17,
              }}
            >
              Ask Athena anything about this workspace…
            </div>
          </div>

          {/* Plan checklist column */}
          <div
            style={{
              flex: 1,
              borderRadius: 12,
              border: `1px solid ${T.border}`,
              background: T.bgSecondary,
              padding: '18px 20px',
              display: 'flex',
              flexDirection: 'column',
              gap: 14,
              position: 'relative',
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                fontFamily: FONT_DISPLAY,
                fontWeight: 700,
                fontSize: 18,
                letterSpacing: 2,
                color: T.textMuted,
              }}
            >
              ATHENA’S PLAN
            </div>
            {PLAN_STEPS.map((step, i) => {
              const doneAt = 58 + i * 9;
              const done = frame >= doneAt;
              const check = interpolate(frame, [doneAt, doneAt + 8], [0, 1], {
                extrapolateLeft: 'clamp',
                extrapolateRight: 'clamp',
              });
              return (
                <div key={step} style={{display: 'flex', alignItems: 'center', gap: 12}}>
                  <div
                    style={{
                      width: 24,
                      height: 24,
                      borderRadius: 7,
                      border: `1.5px solid ${done ? T.success : T.border}`,
                      background: done ? T.success : 'transparent',
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      color: T.bg,
                      fontSize: 16,
                      fontWeight: 800,
                      opacity: done ? check : 1,
                    }}
                  >
                    ✓
                  </div>
                  <span
                    style={{
                      fontFamily: FONT_DISPLAY,
                      fontSize: 19,
                      color: done ? T.text : T.textMuted,
                      textDecoration: done ? 'line-through' : 'none',
                      textDecorationColor: `${T.textDim}`,
                    }}
                  >
                    {step}
                  </span>
                </div>
              );
            })}
            {/* progress bar */}
            <div
              style={{
                marginTop: 'auto',
                height: 6,
                borderRadius: 3,
                background: T.bgHover,
                overflow: 'hidden',
              }}
            >
              <div
                style={{
                  height: '100%',
                  width: `${interpolate(frame, [58, 86], [0, 100], {
                    extrapolateLeft: 'clamp',
                    extrapolateRight: 'clamp',
                  })}%`,
                  background: `linear-gradient(90deg, ${T.accent}, ${T.accentHover})`,
                }}
              />
            </div>
            <Sweep delay={90} duration={24} />
          </div>
        </div>
      </WindowBody>
    </AbsoluteFill>
  );
};
