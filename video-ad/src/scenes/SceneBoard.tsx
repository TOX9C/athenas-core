import React from 'react';
import {AbsoluteFill, Easing, interpolate, useCurrentFrame} from 'remotion';
import {T, FONT_DISPLAY, FONT_MONO} from '../theme';
import {Chrome, WindowBody, TITLEBAR_H} from '../Chrome';
import {StatusDot} from '../primitives';

// Beat 4 (frames 285–375): Kanban board with a card sweeping across the
// columns, plus the agent roster cycling idle → working → done.

const COLUMNS = ['To Do', 'In Progress', 'In Review', 'Complete'] as const;
const COLUMN_ACCENT = [T.textDim, T.accent, T.blue, T.success];

const CARDS: Array<{col: number; title: string; tag: string}> = [
  {col: 0, title: 'Wire notification hooks', tag: 'core'},
  {col: 1, title: 'Pane status dots', tag: 'ui'},
  {col: 2, title: 'Swarm handoff spec', tag: 'agents'},
  {col: 3, title: 'Log redaction audit', tag: 'core'},
];

// The hero card travels col 0 → 1 → 2 → 3.
const HERO_KEYFRAMES = [0, 26, 52, 78];
const HERO_TITLE = 'Ship v0.3.0';

type AgentState = 'idle' | 'working' | 'done';
const AGENTS: Array<{name: string; role: string; doneAt: number; startAt: number}> = [
  {name: 'Coordinator', role: 'orchestrating', startAt: 6, doneAt: 84},
  {name: 'Builder', role: 'implementing', startAt: 14, doneAt: 70},
  {name: 'Scout', role: 'exploring repo', startAt: 22, doneAt: 54},
  {name: 'Reviewer', role: 'reviewing diff', startAt: 38, doneAt: 88},
];

export const SceneBoard: React.FC = () => {
  const frame = useCurrentFrame();
  const heroP = interpolate(frame, HERO_KEYFRAMES, [0, 1, 2, 3], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.inOut(Easing.cubic),
  });

  return (
    <AbsoluteFill>
      <Chrome title="athenas-core — board" />
      <WindowBody pad={20}>
        <div
          style={{
            position: 'absolute',
            inset: TITLEBAR_H + 21,
            display: 'flex',
            gap: 22,
          }}
        >
          {/* Board */}
          <div style={{flex: 1.6, display: 'flex', gap: 12, position: 'relative'}}>
            {COLUMNS.map((col, i) => {
              const drawIn = interpolate(frame, [i * 4, i * 4 + 10], [0, 1], {
                extrapolateLeft: 'clamp',
                extrapolateRight: 'clamp',
                easing: Easing.out(Easing.cubic),
              });
              return (
                <div
                  key={col}
                  style={{
                    flex: 1,
                    borderRadius: 10,
                    border: `1px solid ${T.border}`,
                    background: T.bgSecondary,
                    display: 'flex',
                    flexDirection: 'column',
                    opacity: drawIn,
                    transform: `translateY(${(1 - drawIn) * 12}px)`,
                    overflow: 'hidden',
                  }}
                >
                  <div
                    style={{
                      padding: '10px 12px',
                      borderBottom: `1px solid ${T.border}`,
                      display: 'flex',
                      alignItems: 'center',
                      gap: 8,
                    }}
                  >
                    <StatusDot size={8} color={COLUMN_ACCENT[i]} />
                    <span
                      style={{
                        fontFamily: FONT_DISPLAY,
                        fontSize: 15,
                        fontWeight: 700,
                        color: T.textMuted,
                      }}
                    >
                      {col}
                    </span>
                  </div>
                  <div style={{padding: 10, display: 'flex', flexDirection: 'column', gap: 8}}>
                    {CARDS.filter((c) => c.col === i).map((c) => (
                      <Card key={c.title} title={c.title} tag={c.tag} />
                    ))}
                  </div>
                </div>
              );
            })}

            {/* hero card sweeping across columns */}
            <div
              style={{
                position: 'absolute',
                top: 46,
                left: `${heroP * 100}%`,
                width: 'calc(25% - 9px)',
                transform: 'translateX(0px)',
                marginLeft: 0,
                // ride the columns: left edge tracks heroP across the row
                x: 0,
              }}
            >
              <div
                style={{
                  borderRadius: 10,
                  background: T.bgTertiary,
                  border: `1.5px solid ${T.accent}`,
                  boxShadow: `0 0 24px rgba(201,162,75,0.25), 0 12px 32px rgba(0,0,0,0.35)`,
                  padding: '12px 14px',
                }}
              >
                <div
                  style={{
                    fontFamily: FONT_DISPLAY,
                    fontSize: 16,
                    fontWeight: 700,
                    color: T.text,
                    marginBottom: 6,
                  }}
                >
                  {HERO_TITLE}
                </div>
                <div style={{display: 'flex', alignItems: 'center', gap: 7}}>
                  <StatusDot size={8} color={T.accent} glow />
                  <span style={{fontFamily: FONT_MONO, fontSize: 12, color: T.textDim}}>
                    agent-assigned
                  </span>
                </div>
              </div>
            </div>
          </div>

          {/* Agent roster */}
          <div
            style={{
              flex: 1,
              borderRadius: 12,
              border: `1px solid ${T.border}`,
              background: T.bgSecondary,
              padding: '16px 18px',
              display: 'flex',
              flexDirection: 'column',
              gap: 12,
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
              AGENT TEAM
            </div>
            {AGENTS.map((a) => {
              const working = frame >= a.startAt && frame < a.doneAt;
              const done = frame >= a.doneAt;
              const dotColor = done ? T.success : working ? T.accent : T.textDim;
              const pulse = working ? 0.5 + 0.5 * Math.abs(Math.sin(frame * 0.15)) : 1;
              return (
                <div key={a.name} style={{display: 'flex', alignItems: 'center', gap: 12}}>
                  <StatusDot size={11} color={dotColor} opacity={pulse} glow={working} />
                  <div style={{display: 'flex', flexDirection: 'column'}}>
                    <span style={{fontFamily: FONT_DISPLAY, fontSize: 17, fontWeight: 700, color: T.text}}>
                      {a.name}
                    </span>
                    <span style={{fontFamily: FONT_MONO, fontSize: 13, color: done ? T.success : T.textMuted}}>
                      {done ? 'done ✓' : a.role}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </WindowBody>
    </AbsoluteFill>
  );
};

const Card: React.FC<{title: string; tag: string}> = ({title, tag}) => (
  <div
    style={{
      borderRadius: 8,
      background: T.bgTertiary,
      border: `1px solid ${T.border}`,
      padding: '9px 11px',
    }}
  >
    <div style={{fontFamily: FONT_DISPLAY, fontSize: 14, fontWeight: 600, color: T.text, marginBottom: 5}}>
      {title}
    </div>
    <span
      style={{
        fontFamily: FONT_MONO,
        fontSize: 11,
        color: T.textMuted,
        background: T.bgHover,
        borderRadius: 999,
        padding: '2px 8px',
      }}
    >
      {tag}
    </span>
  </div>
);
