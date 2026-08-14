---
name: 'One Window — Obsidian'
description: "Fresh instrument-panel product film for Athena's Core — real app captures on a dark obsidian canvas, bronze accents. No myth, no paper."
---

# One Window — Obsidian

A 25-second, single-story promo for **Athena's Core**, driven entirely by real
screen captures of the app. The story: _one native window replaces the five apps
it used to take to build._ Precise, editorial — an instrument panel at night,
matching the app's own dark Nyx theme so the captures sit naturally in the frame.

## Palette

| Token  | Hex                      | Use                                                     |
| ------ | ------------------------ | ------------------------------------------------------- |
| bg     | `#0E0C10`                | Canvas background (warm obsidian)                       |
| panel  | `#16141B`                | Capture card frames, caps                               |
| border | `#2A2731`                | Hairline rules, card borders                            |
| text   | `#F2EFE9`                | Headlines (warm off-white)                              |
| sub    | `#A9A49B`                | Body copy, dimmed text                                  |
| dim    | `#98938A`                | Top/bottom bars, card captions, metadata                |
| gold   | `#C9A24B`                | Kickers, index numerals, registration marks, brand mark |
| ghost  | `rgba(242,239,233,0.05)` | Ghost numerals, decorative marks                        |

The app's own accent is `#C9A24B` — use it for focal elements (kickers, marks,
indexes) and keep structural elements (rules, borders) quiet.

## Typography

- **Voice — Hanken Grotesk** (the product's grotesque): headlines 700, body 400.
  Tight display tracking (-0.03em), 92–122px headlines, 34px sub-copy.
- **Data — Monaspace Neon** (the product's terminal mono): kickers, captions,
  timecodes, ghost numerals. Uppercase, 0.16–0.24em tracking on kickers.
- No serif. The mythic Cormorant stays out of the film.

## Corners & Depth

- Sharp instrument corners: cards 6px, everything else square or 2px.
- Depth: layered. Cards cast `0 30px 70px rgba(0,0,0,0.55)`, captures sit in
  dark `panel` frames with 2px `border` hairlines and gold registration marks.
- Rules draw in with `scaleX`; ghost numerals breathe slowly.

## Motion

- Entrances: `expo.out` / `power3.out`, 0.5–0.75s. Transitions are push-slides
  between scenes with a zoom-through + blur at the climax (T4).
- The climax (scene 5) must keep moving: fast text reveal, continuous ken burns
  (1.06 → 1.28), short dwell, blur-crossfade into the close. No static holds.
- Audio: SFX only (shutter ticks, whooshes, closing chime) — **no music bed**.

## Do / Don't

**Do:**

- Lead with the real app. Captures are the heroes; type supports.
- Keep copy to one story: blank workspace → work loop → scale → one window.
- Use the gold accent visibly (kickers, rules, marks) — 15–25% atmospheric,
  full saturation on focal elements.
- Vary motion per scene: tilt, fan, scroll-reveal, zoom, full-bleed.

**Don't:**

- No Greek mythology, no owls, no shields, no lamp glow, no ghost "ATHENA"
  wordmarks.
- No fake UI mockups — every product shot is a real capture.
- No feature dump. No lists of features. Three steps, one journey.
- No gradient text, no left-edge accent stripes, no neon, no light/paper canvas.
