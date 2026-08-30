import React from 'react';
import {AbsoluteFill, Easing, interpolate, useCurrentFrame} from 'remotion';
import {T, FONT_MONO} from '../theme';
import {Chrome, WindowBody, TITLEBAR_H} from '../Chrome';
import {StatusDot} from '../primitives';

// Beat 2 (frames 75–180): 2×2 terminal grid draws in; parallel cargo builds
// type out; status dots pulse; final green OK line per pane.

type Line = {t: string; c: string};

const PANE_SCRIPTS: Line[][] = [
  [
    {t: '$ cargo build --release', c: T.text},
    {t: '   Compiling athenas-core v0.3.0', c: T.textDim},
    {t: '   Compiling frontend v0.3.0', c: T.textDim},
    {t: '    Building [ 12/38 ]', c: T.textMuted},
    {t: '✓ Finished release [optimized] in 3.2s', c: T.success},
  ],
  [
    {t: '$ cargo test', c: T.text},
    {t: '   Compiling core-lib v0.3.0', c: T.textDim},
    {t: '    Running unittests (86 tests)', c: T.textMuted},
    {t: 'test result: ok. 86 passed; 0 failed', c: T.success},
  ],
  [
    {t: '$ cargo clippy -- -D warnings', c: T.text},
    {t: '    Checking athenas-core v0.3.0', c: T.textDim},
    {t: '    Checking frontend v0.3.0', c: T.textMuted},
    {t: '✓ Zero warnings', c: T.success},
  ],
  [
    {t: '$ ./run-agent-swarm.sh', c: T.text},
    {t: '  → spawning coordinator…', c: T.blue},
    {t: '  → spawning builder ×2…', c: T.blue},
    {t: '  → swarm online (4 agents)', c: T.accent},
  ],
];

const GRID_GAP = 14;

const Pane: React.FC<{
  script: Line[];
  index: number;
  startDelay: number;
}> = ({script, index, startDelay}) => {
  const frame = useCurrentFrame();
  const drawIn = interpolate(frame, [startDelay, startDelay + 12], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });

  // Type the whole pane's lines sequentially at ~28 chars/sec.
  const totalChars = script.reduce((n, l) => n + l.t.length + 1, 0);
  const cps = 34;
  const typedChars = Math.max(
    0,
    Math.min(totalChars, Math.floor((frame - startDelay - 8) * (cps / 30))),
  );
  let budget = typedChars;
  const rendered: React.ReactNode[] = [];
  for (let i = 0; i < script.length && budget > 0; i++) {
    const line = script[i];
    const take = Math.min(line.t.length, budget);
    rendered.push(
      <div
        key={i}
        style={{
          color: line.c,
          opacity:
            take < line.t.length
              ? 1
              : interpolate(
                  frame,
                  [startDelay + 8 + ((budget / totalChars) | 0) * 2, startDelay + 10 + ((budget / totalChars) | 0) * 2],
                  [0.7, 1],
                  {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'},
                ),
        }}
      >
        {line.t.slice(0, take)}
      </div>,
    );
    budget -= line.t.length + 1;
  }

  return (
    <div
      style={{
        flex: 1,
        borderRadius: 10,
        background: '#0d0d0f',
        border: `1px solid ${T.border}`,
        overflow: 'hidden',
        opacity: drawIn,
        transform: `translateY(${(1 - drawIn) * 14}px)`,
        padding: '14px 18px',
        fontFamily: FONT_MONO,
        fontSize: 19,
        lineHeight: 1.75,
      }}
    >
      <div
        style={{
          position: 'absolute',
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          marginTop: -6,
          marginBottom: 8,
        }}
      >
        <StatusDot
          size={9}
          color={index === 3 ? T.accent : T.success}
          glow={frame % 60 < 30}
        />
        <span style={{fontSize: 13, color: T.textDim}}>pane-{index + 1}</span>
      </div>
      <div style={{marginTop: 22}}>{rendered}</div>
    </div>
  );
};

export const SceneTerminals: React.FC = () => {
  return (
    <AbsoluteFill>
      <Chrome title="athenas-core — build" />
      <WindowBody pad={16}>
        <div
          style={{
            position: 'absolute',
            inset: TITLEBAR_H + 17,
            display: 'grid',
            gridTemplateColumns: '1fr 1fr',
            gridTemplateRows: '1fr 1fr',
            gap: GRID_GAP,
          }}
        >
          {PANE_SCRIPTS.map((script, i) => (
            <Pane key={i} script={script} index={i} startDelay={i * 5} />
          ))}
        </div>
      </WindowBody>
    </AbsoluteFill>
  );
};
