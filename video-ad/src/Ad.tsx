import React from 'react';
import {AbsoluteFill, Composition, interpolate, Sequence, useCurrentFrame} from 'remotion';
import {T, FPS, W, H} from './theme';
import {loadFonts} from './loadFonts';

import {SceneHook} from './scenes/SceneHook';
import {SceneTerminals} from './scenes/SceneTerminals';
import {SceneAthena} from './scenes/SceneAthena';
import {SceneBoard} from './scenes/SceneBoard';
import {SceneCta} from './scenes/SceneCta';

const SCENE_CUTS = [0, 75, 180, 285, 375, 450];

// Global crossfade: every scene boundary gets a short dip-to-black so the
// whole film reads as one piece with deliberate cuts.
const CrossFade: React.FC<{
  children: React.ReactNode;
  durationInFrames: number;
}> = ({children, durationInFrames}) => {
  const frame = useCurrentFrame();
  const fadeIn = interpolate(frame, [0, 8], [0, 1], {
    extrapolateRight: 'clamp',
  });
  const fadeOut = interpolate(
    frame,
    [durationInFrames - 8, durationInFrames],
    [1, 0],
    {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'},
  );
  return (
    <AbsoluteFill style={{opacity: Math.min(fadeIn, fadeOut)}}>
      {children}
    </AbsoluteFill>
  );
};

export const AdFilm: React.FC = () => {
  loadFonts();
  return (
    <AbsoluteFill style={{background: T.bg}}>
      <Sequence from={SCENE_CUTS[0]} durationInFrames={75}>
        <CrossFade durationInFrames={75}>
          <SceneHook />
        </CrossFade>
      </Sequence>
      <Sequence from={SCENE_CUTS[1]} durationInFrames={105}>
        <CrossFade durationInFrames={105}>
          <SceneTerminals />
        </CrossFade>
      </Sequence>
      <Sequence from={SCENE_CUTS[2]} durationInFrames={105}>
        <CrossFade durationInFrames={105}>
          <SceneAthena />
        </CrossFade>
      </Sequence>
      <Sequence from={SCENE_CUTS[3]} durationInFrames={90}>
        <CrossFade durationInFrames={90}>
          <SceneBoard />
        </CrossFade>
      </Sequence>
      <Sequence from={SCENE_CUTS[4]} durationInFrames={75}>
        <CrossFade durationInFrames={75}>
          <SceneCta />
        </CrossFade>
      </Sequence>
    </AbsoluteFill>
  );
};

export const RemotionRoot: React.FC = () => (
  <Composition
    id="AthenaCoreAd"
    component={AdFilm}
    durationInFrames={450}
    fps={FPS}
    width={W}
    height={H}
  />
);
