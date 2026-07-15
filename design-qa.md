# Provider Setup Design QA

## Comparison target

- Source visual truth: `/var/folders/0z/kq1k4pwn71g7q_yp6x9r655h0000gn/T/codex-clipboard-a08a6eac-0dad-470b-892a-8ab7abfb39b4.png`, with the user's subsequent annotations removing the sidebar, product logo, setup labels, helper copy, recommendation chip, status/offline content, option subtitles, selection ticks, and divider; the latest pass also calls for a quieter primary-button glow and local API-key format validation.
- Implementation screenshots:
  - `docs/design-qa/provider-setup-codex-selected.jpeg`
  - `docs/design-qa/provider-setup-api-empty.jpeg`
  - `docs/design-qa/provider-setup-api-invalid.jpeg`
  - `docs/design-qa/provider-setup-api-valid.jpeg`
- Normalized full-view comparison: `docs/design-qa/provider-setup-comparison.png`.
- Focused validation-state comparison: `docs/design-qa/provider-setup-api-validation.png`.
- Desktop viewport: 1,187 x 768.
- States checked: Codex selected, API key selected with an empty key, malformed key, plausible OpenAI-style key, and successful continuation from a plausible key.

## Full-view comparison evidence

The current implementation keeps the selected direction's dark, restrained palette and centered provider decision while applying every deliberate simplification requested after the original mock. The page now contains only the question, two provider choices, and the active provider action. The product logo and divider are absent, so the decision reads as one compact group instead of a branded setup panel. The primary action retains a small elevation cue without the earlier blue halo.

## Focused-region evidence

`docs/design-qa/provider-setup-api-validation.png` places the empty and plausible-key states side by side. The input and button remain in the same row, so validation does not move the layout. The empty and malformed states keep continuation disabled; the plausible `sk-…` state enables it. A live click from the enabled state reached the reaction builder.

## Required fidelity surfaces

- Fonts and typography: Iced's native UI type remains legible with one clear heading level and consistent option/action sizing. No wrapping or truncation is visible at the checked desktop viewport.
- Spacing and layout rhythm: the centered 768 px measure, stacked choices, 12 px choice gap, and 24 px section rhythm remain balanced without the removed logo or divider. The action stays visually attached to the choices.
- Colors and visual tokens: existing ChemSpec canvas, surface, accent, muted, and disabled tokens are reused. The selected tint and border remain restrained, while the primary shadow is substantially lower-opacity and shorter than the prior treatment.
- Image quality and asset fidelity: there is no raster imagery in this screen. Visible provider and action icons are vendored Majesticons SVG assets; no text-glyph or handcrafted icon substitutions are used.
- Copy and content: only decision-critical copy remains. The option subtitles, labels, status copy, offline note, logo lockup, and divider are absent.
- States and interactions: provider switching works; the API key is secure; empty, short, wrongly prefixed, and whitespace-containing values remain blocked; a plausible `sk-…` value enables continuation; and the enabled action reaches the builder.
- Accessibility: controls remain keyboard-native Iced widgets, disabled states are explicit, and focus styling remains visible. The intentionally minimal selected state is conveyed by both a tinted surface and an accent border.

## Comparison history

1. The first implementation pass retained too much of the exploratory mock's framing and explanatory content.
2. The initial Iced redesign removed the sidebar, labels, status copy, recommendation/offline content, option subtitles, and selection ticks, then added the Codex-unavailable fallback.
3. The latest pass removed the remaining page logo and divider, reduced the primary shadow from a broad glow to a small elevation cue, and added shared API-key format gating to both the view and transition logic.
4. Post-fix desktop captures and live interaction checks show no layout movement between invalid and valid API-key states, and no actionable P0/P1/P2 visual issue remains.

## Findings

No actionable P0, P1, or P2 fidelity issues remain.

## Follow-up polish

- P3: a future credential check could show an authentication error after the first OpenAI request; local format validation intentionally cannot prove that a key is live or authorized.

## Implementation checklist

- [x] Remove the page-specific product logo.
- [x] Remove the divider above the action.
- [x] Replace the primary-button glow with a restrained elevation shadow.
- [x] Keep the API action row stable across empty, invalid, and valid input.
- [x] Gate continuation with the same OpenAI-style format predicate in the view and update path.
- [x] Verify invalid and valid interaction states in the packaged desktop app.

final result: passed
