# Phase 4: Workspace Modal Contrast Regression Checklist

Purpose: quick manual QA for Terminal Workspace modal visual clarity across themes.

## Representative Themes

- Dark theme with strong accent (for example: high-contrast dark + blue/purple accent)
- Light theme with strong accent (for example: light + blue/green accent)
- Low-saturation accent theme (muted/gray-ish accent in either dark or light mode)

## Modal States to Verify (exact flow)

1. Open Terminal Workspace modal from the main workspace UI.
2. Verify default/open state before any input.
3. Enter valid input (name/config) and verify "ready to submit" state.
4. Clear or invalidate required input and verify disabled submit state.
5. Hover/focus interactive controls (buttons, close icon, fields).
6. Trigger inline validation/error state (if available).
7. Close and reopen modal to verify state reset and baseline readability.

## Pass/Fail Criteria (visibility + contrast)

- **Modal container** is clearly separated from backdrop in every theme.
- **Title/body text** is readable without strain; no low-contrast text on background.
- **Input borders/placeholders/value text** remain distinguishable in all states.
- **Primary action (enabled)** is visually prominent and clearly actionable.
- **Primary action (disabled)** is clearly non-actionable and visibly different from enabled.
- **Secondary/cancel action** remains visible and does not visually compete with primary action.
- **Focus/hover states** are visible on keyboard and pointer interaction.
- **Error/validation text** is readable and clearly associated with the relevant field.

Fail if any state requires guessing whether an action is enabled, if text is hard to read, or if controls blend into the background.

## Quick Manual Test Procedure (5-10 min)

1. Test all states in dark theme first; capture quick notes/screenshot on any ambiguity.
2. Repeat in light theme and compare enabled vs disabled button clarity side-by-side.
3. Repeat in low-saturation accent theme; pay extra attention to muted borders and disabled controls.
4. For each theme, record: `PASS` / `FAIL` + one-line reason.
5. If any `FAIL`, log affected state, theme, and exact UI element (text/button/input/focus/error).
