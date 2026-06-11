---
name: "Athenaeum Gold"
description: "Premium dark cinematic identity for Athena's Core — Obsidian & Gold, Greek mythology meets modern IDE."  

---

## Overview

The "Athenaeum Gold" visual identity is built on the contrasts of deep obsidian darkness and warm bronze-gold illumination — like firelight in a marble hall. The aesthetic is premium, mythic, and rooted in the Greek mythology identity of Athena. It is not "dark mode" — it is the night sky before a constellation rises.

## Colors

| Token              | Value     | Usage                                              |
|--------------------|-----------|----------------------------------------------------|
| Obsidian           | `#0E0C14` | Background, the canvas of the void                 |
| Obsidian-deep      | `#07050F` | Deep shadow, edge dissolve                         |
| Obsidian-light     | `#1A1624` | Card surfaces, panels, secondary background        |
| Gold               | `#C9A24B` | Primary accent, primary CTAs, highlights           |
| Gold-dim           | `#8B6E2F` | Secondary accent, decorative lines, muted glow     |
| Gold-bright        | `#E5C566` | Focal accents, the brightest element in a frame      |
| Parchment          | `#E8E4D5` | Primary text on dark backgrounds                   |
| Stone              | `#6E6A74` | Secondary text, labels, metadata                   |
| Ivory              | `#F5F0E1` | Hero text, highest emphasis                        |

## Typography

| Token                  | Font              | Weight | Size (video)   | Usage                          |
|------------------------|-------------------|--------|----------------|--------------------------------|
| Display (brand)        | Cormorant         | 700    | 96-160px       | Scene titles, brand moments    |
| Display Italic         | Cormorant         | 700i   | 48-72px        | Sub-titles, quotes             |
| UI (body/data)         | Hanken Grotesk    | 400    | 28-42px        | Descriptions, labels           |
| Data / Code            | Monaspace Neon    | 400    | 16-24px        | Terminal/code snippets         |
| Mono (accents)         | Monogram          | 400    | 18-24px        | Labels, metadata tags          |

- Headline tracking: `-0.03em` (tighter than web for video compression)
- Line-height for display: `0.9`
- Line-height for body: `1.2`
- Font license: Cormorant (Google Fonts), Hanken Grotesk (Google Fonts), Monaspace Neon (GitHub Fonts)

## Elevation

| Level     | Usage                                           |
|-----------|-------------------------------------------------|
| Flat      | Backgrounds, no shadow                          |
| Raised    | Cards, panels — subtle 1px border at `#1A1624`  |
| Floating  | Hero elements — gold glow only, no shadow       |

## Components

- Borders: `1px solid` with 8% opacity of Parchment — near-invisible but structural
- Buttons: No visible background, gold text, hover state shifts color to `Gold-bright`
- Cards: Minimal, `1px` border, no border-radius (sharp edges for industrial precision)

## Motion Principles

- **Energy:** Cinematic. Slow, deliberate entrances. Holds breathe.
- **Easing:**
  - Entry: `power3.out` to `expo.out`
  - Exit: `power4.in`
  - Transition: `power2.inOut`
- **Duration:**
  - Entrance: 0.6-1.2s
  - Hold: 1.5-3.0s
  - Transition: 0.8-1.5s
- **Atmosphere:**
  - Radial gold glow (breathing)
  - Grain overlay (filmic texture)
  - Hairline rules (animated, structural)
  - Ghost text in background (3% opacity)
- **Transition:** Cinematic Zoom or Blur Crossfade

## Do's and Don'ts

**DO:**
- Use gold as illumination — a warm glow against the obsidian void
- Let the dark win — at least 60-70% of any frame should be dark
- Use typography as decoration — large ghost characters in the background
- Add grain/noise for filmic texture
- Anchor text to edges or use asymmetric layouts

**DON'T:**
- Use purple gradients, cyan neon, or any color not in this palette
- Center everything — entries should feel directional
- Use rounded corners on hero elements — this is architecture, not UI
- Add bright gradients that dilute the premium feel
- Use small type sizes — everything should read at a glance in the dark
