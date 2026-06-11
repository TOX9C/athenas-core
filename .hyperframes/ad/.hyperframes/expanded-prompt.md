# Athena's Core — Product Advertisement Video

## Title + Style

**Design Reference:** Athenaeum Gold (design.md)
- **Background:** Obsidian (`#0E0C14`) — deep, cinematic dark
- **Accent:** Bronze-gold (`#C9A24B`) — warm, illuminating contrast
- **Text:** Parchment (`#E8E4D5`) on dark, Ivory (`#F5F0E1`) for hero
- **Emotion:** Premium, mythic, powerful. Like firelight in a marble hall.

**Fonts:**
- Display: Cormorant (serif, 700) — brand authority, elegance
- UI Body: Hanken Grotesk — clean, modern
- Accent / Code: Monaspace Neon — terminal aesthetic

## Rhythm Declaration

`hook-PULSE-hold-PULSE-hold-PULSE-PEAK-drift-resolve`

## Global Rules

- **Parallax layers:** Background decoratives (glow, grain, ghost text) drift at 20% speed. Content sits in midground. Accent particles orbit in foreground.
- **Micro-motion:** Every decorative has ambient animation — gold glow breathes (scale 0.9-1.1), grain shifts subtly, hairline rules pulse opacity.
- **Transition style:** Cinematic zoom blur (power2.inOut, 0.8-1.2s) between major features. Fast blur-crossfade for rapid-cut montage.
- **Primary transition:** Cinematic Zoom. Accent transition: Blur Crossfade.
- **Energy:** High for feature highlights, Calm for CTA.

## Per-Scene Beats

---

### Scene 1: The Name (0-4s)

**Concept:** The frame opens on deep darkness. A warm bronze glow emerges from center — like fire catching in obsidian. The name materializes with weight and authority. This isn't a logo reveal — it's an awakening.

**Mood direction:** Cinematic title sequence. The kind where the audience leans forward. Think "A24 opening" — dark, deliberate, powerful.

**Depth layers:**
- **BG:** Deep obsidian (#07050F) with a breathing radial gold glow at center
- **MG:** "ATHENA'S CORE" in massive Cormorant, centered, with a subtle gold text-shadow
- **FG1:** Hairline gold rule that draws itself across the frame at 1px height, pulsing
- **FG2:** Tiny monospaced label "OBSIDIAN // GOLD" in Monospace Neon at 18px, bottom-center, low opacity

**Animation choreography:**
- Radial glow: BREATHES in scale from 0.8 to 1.0 over 2s, then holds
- Title: SLAMS into existence — scale 1.05 → 1.0, y 30 → 0, opacity 0 → 1, expo.out, 0.8s (offset 0.4s)
- Hairline: DRAWS itself from center outward — scaleX 0 → 1, power2.inOut, 0.6s
- Label: FADES in — opacity 0 → 0.6, sine.inOut, 0.5s
- Transition: Cinematic Zoom blur outward to next scene

**Transition out:** Cinematic Zoom, 1.0s, power2.inOut

---

### Scene 2: Terminal (4-9s)

**Concept:** A terminal window materializes — the digital forge. Code syntax elements flash, terminal prompts flicker with the energy of a live shell. The gold is now functional — highlighting the prompt, the cursor, the active state.

**Mood direction:** Hackers meets Lord of the Rings. A command line is an altar. The terminal is not just a tool — it's a temple.

**Depth layers:**
- **BG:** Obsidian with a very faint code-glyph ghost text at 4% opacity, slowly drifting
- **MG:** Terminal window with border — 1px gold at 12% opacity
- **MG1:** Prompt line `$ athena --deploy` in Monospace Neon, bright gold
- **MG2:** Output text in Parchment, Monospace Neon, lower opacity
- **FG:** Cursor blinking in gold, a subtle glow beneath the terminal
- **ACCENT:** "TERMINAL // PTY POWER" label, Hanken Grotesk at 24px, bottom-left

**Animation choreography:**
- Terminal window: SLIDES up from bottom, y: 60 → 0, opacity 0 → 1, power3.out, 0.6s (offset 0.15s)
- Prompt text: TYPES in character-by-character (simulate via stagger)
- Output text: FADES in line by line, staggered 0.15s each
- Cursor: Blinking animation (opacity 0 — 1 → 0 → 1, repeated 4 times over 4s)
- Label: FADES in with slight y: 20 → 0, opacity 0 → 1, power2.out, 0.4s (offset 0.3s)
- Transition: Quick blur-crossfade

**Transition out:** Blur Crossfade, 0.6s, power2.out

---

### Scene 3: AI Chat — 🧠 Athena (9-14s)

**Concept:** The AI assistant reveals herself — Athena, the goddess of wisdom. Chat bubbles cascade in from alternating directions, showing a conversation between user and AI. The name is not coincidental — it's a promise.

**Mood direction:** Conversational but mystical. Like watching two voices whisper across a void, one warm (gold), one cold (parchment-on-dark). Think modern Slack/Docusign AI feature reveal but with mythological weight.

**Depth layers:**
- **BG:** Obsidian with a faint radial glow that shifts to gold-tinted
- **MG:** Chat interface — two message bubbles
- **MG1:** User bubble: "Refactor the auth module" — left-aligned, Parchment, Monospace Neon
- **MG2:** AI bubble: "Done. I've extracted the auth logic, added middleware, and pushed to main. Here's the diff..." — Gold-tinted border, right-aligned, Monospace Neon with `font-style: normal`
- **FG:** "ATHENA // AI CO-PILOT" label, Hanken Grotesk at 24px
- **ACCENT:** Subtle sparkle particles at 6px size, gold, orbiting the AI message

**Animation choreography:**
- User bubble: SLIDES in from left, x: -100 → 0, opacity 0 → 1, expo.out, 0.5s (offset 0.3s)
- AI bubble: SLIDES in from right, x: 100 → 0, opacity 0 → 1, expo.out, 0.5s (offset 1.0s, after a beat)
- AI response text: REVEALS character by character (simulate text "typing" effect via rapid opacity reveals with stagger)
- Label: FADES in top-left with y: -20 → 0, power3.out, 0.4s
- Sparkle particles: PULSE and DRIFT around the AI message
- Transition: Cinematic zoom pull outward

**Transition out:** Cinematic Zoom, 0.8s, power2.inOut

---

### Scene 4: Swarm — Multi-Agent Orchestration (14-20s)

**Concept:** Four cards converge from opposite corners — swarm of agents. Each card represents a different agent type (e.g., "CODE", "ARCHITECT", "DEVOPS", "TESTER"). The cards circle and arrange into a grid, then a central hub glows gold — the orchestrator. This is the brain of Athena's Core.

**Mood direction:** Kaleidoscopic but disciplined. Like watching a formation of fighter jets, but made of code examples and data. Precision and power.

**Depth layers:**
- **BG:** Obsidian with pulsing concentric circles (gold, 8% opacity, radial pattern)
- **MG:** 4 agent cards arranged in a 2x2 grid
- **MG1:** Card text: agent role + one-line description
- **FG:** Central hub — a glowing gold orb/circle at 20% opacity, pulsing
- **FG2:** "SWARM // MULTI-AGENT MIND" label, top-left
- **ACCENT:** Subtle connecting lines between cards and hub (drawn on, dashed, gold at 15% opacity)

**Animation choreography:**
- Cards 1-4: CONVERGE from edges toward center at varying speeds — staggered 0.15s each, power3.out, 0.6s (offset 0.5s from scene start)
- Card stagger: Each card enters from a different direction (left, right, top, bottom)
- Central hub: PULSES into existence — scale 0 → 1.0, opacity 0 → 0.3, power2.out, 0.4s (offset 0.8s), then pulses continuously
- Connecting lines: DRAW themselves from cards to hub scaleX 0 → 1, power2.inOut, 0.5s (offset 1.2s)
- Label: FADES in top-left, 0.3s
- Agent count: A counter "1984 active agents" counts up rapidly below the cards
- Transition: Cinematic zoom outward + blur

**Transition out:** Cinematic Zoom, 1.0s, power2.inOut — zooming out dramatically into the next scene

---

### Scene 5: Feature Montage (20-26s)

**Concept:** A rapid-fire, split-screen montage of *all* features. 6 panels (terminal, AI chat, Kanban, plugins, editor, browser) flash in rapid sequence. Hard cuts, no transitions between tiles. The energy peaks.

**Mood direction:** House of cards intro. Fast-paced, rhythmic, punchy. Precise cuts on beat. Maximum density — the app is everything.

**Depth layers:**
- **BG:** Solid Obsidian (#0E0C14)
- **MG:** 6 feature tiles in a 3x2 grid
- **FG:** Rapid flash transitions between tiles at 0.25s intervals

**Animation choreography:**
- All six tiles: POP in from scale 0 → 1, opacity 0 → 1, stagger 0.1s, power2.out
- Interior of each tile: A "snapshot" (rectangular div block) of each feature
  - Terminal: prompt text + green text
  - Athena: chat bubble lines
  - Kanban: 3 cards
  - Swarm: 4 cards (reuse pattern)
  - Editor: code blocks
  - Browser: simple iframe block
- Flash cut: Every 0.6s, one tile briefly scales to 1.05, border glows gold, then scales back — this pulses clockwise around the grid
- Label: "75+ FEATURES // ONE WORKSPACE" flashes in top-left
- Transition: Hard cut to CTA

**Transition out:** Hard cut to CTA — the energy drops to calm

---

### Scene 6: Call to Action (26-35s)

**Concept:** The final scene. The dark becomes absolute — a void. A single line of text appears: "Redefine your workflow." Below it, a golden CTA. The tone is quiet, confident — a promise after the storm.

**Mood direction:** The end of an A24 trailer. The screen is almost empty. One line, one button. You lean forward again.

**Depth layers:**
- **BG:** Absolute deepest obsidian (#07050F) with a faint central gold glow breathing
- **MG:** Hero text "Redefine your workflow." in Cormorant Italic, center, massive (120px)
- **FG1:** CTA button: "Get Athena's Core" — no background, 1px gold border, gold text, subtle hover animation (scale 1.02)
- **FG2:** Small URL "github.com/yourname/athenas-core" in Monospace Neon, below CTA, Stone color
- **ACCENT:** "OBSIDIAN & GOLD" in Monospace Neon, bottom-right, aligned right

**Animation choreography:**
- Background glow: BREATHES slowly, scale 0.7-1.0, 3s, infinite
- Hero text: FADES in with y: 40 → 0, opacity 0 → 1, power3.out, 1.0s (offset 0.3s)
- CTA: DRAWS in from scale 0.8 → 1, opacity 0 → 1, power2.out, 0.5s (offset 1.2s)
- URL: FADES in last, power2.out, 0.3s (offset 1.8s)
- Badge: FADES in bottom-right, 0.4s (offset icontains 2.0s)
- Final hold: 3.5s for reading

## Recurring Motifs

1. **Gold glow:** Every scene has a breathing radial glow centered near the hero element. The glow is the "lamp" — it anchors every shot.
2. **1px hairline rules:** Anchor text or section breaks. Gold at 12-15% opacity, with subtle opacity pulse.
3. **Monospace metadata labels:** Small labels in Monospace Neon (Monogram substitute) identify the feature being shown. Consistent placement: bottom-left or top-left.
4. **Grain:** Consistent film grain across all scenes at 5-8% opacity.
5. **Centered + asymmetric:** Hero text centered, but accent elements (labels, metadata) anchored to corners. Never everything floating in the middle.

## Negative Prompt

- NO generic AI gradients (purple, cyan, blue)
- NO motion blur, gaussian blur, or radial blur on any text
- NO text-like shapes or unreadable characters
- NO rounded corners (the aesthetic is sharp, precise, industrial-mythic)
- NO bright white text (use Parchment/Ivory and Stone)
