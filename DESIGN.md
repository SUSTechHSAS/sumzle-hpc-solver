# DESIGN.md

## 01 Overview
Sumzle HPC Solver is a product interface, not a campaign page. The visual system should feel like an engineering console: compact, neutral, high contrast, and deliberate. Use color to encode state and action, not decoration.

## 02 Color
- Background: warm off-white / near-ink dark mode.
- Surface: raised cards with subtle borders, not heavy shadows.
- Accent: blue for primary action and focus only.
- State: green = correct/recommended/success, amber = present/warning, grey = absent/neutral, red = destructive/error.
- Avoid gradients on text and avoid purple-blue AI-default backgrounds.

## 03 Typography
- UI font: system sans, with tight but readable line-height.
- Numeric / expression font: ui-monospace / SFMono / Consolas.
- Hierarchy comes from size, weight, spacing, and casing; no gradient text.
- Prefer concise Chinese labels with optional terse technical hints.

## 04 Spacing and layout
- Desktop: two-column workbench, left for input, right for output/tools.
- Mobile: single-column with results after input.
- Cards use 12–18px internal spacing, 10–14px radii, and consistent gaps.
- Avoid identical-card soup; group by task stage.

## 05 Components
- Button: solid primary, quiet secondary, explicit danger. Preserve focus rings.
- Panel: surface + border + small shadow, no thick side stripe.
- Help/notice: tinted inset panel with full border, not left-tab accent.
- Results row: table-like list, hover with background only, no slide-on-hover.
- Progress: transform-based animation only; do not animate width.

## 06 Motion
Subtle and functional. Use transform/opacity for active and loading states. Respect reduced motion.

## 07 Accessibility
- 44px-ish hit targets where practical.
- Visible focus state on every interactive control.
- Color is supported by text and shape, not used alone.
- Do not let long expressions overflow the viewport.
