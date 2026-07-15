# ChemSpec interface design system

This document records the visual contract for the `U-101` application shell
and the `U-106` reaction-builder entry experience.
It translates the design principles of the local `ChemistrySim` reference into
ChemSpec without importing its chemistry model, Bevy structure, or user flow.

## Reference audit

The reusable characteristics of `ChemistrySim` are:

- an inset, instrument-like workspace on a quiet near-black canvas;
- one dominant learning surface instead of a grid of equal-weight cards;
- compact uppercase context labels paired with larger, plain-language titles;
- low-contrast panel boundaries, nested surfaces, and restrained soft depth;
- a single cool-blue interaction accent with semantic colours reserved for
  status;
- dense controls at the perimeter and generous space around the focal model;
- visible hover, pressed, selected, disabled, and drop/focus states;
- staged information that moves from setup to explanation to result;
- motion used to explain state changes, with stable geometry and clear
  playback context.

ChemSpec applies these principles to a validation-first product. During
composition, the reaction builder is the dominant stage; after validation, the
simulation becomes the dominant stage while workflow, source, validation, and
evidence live in an explicit inspector. Source and provenance remain distinct,
and presentation never changes validation meaning.

## Reaction-builder composition

The builder uses a persistent five-step route—Elements, Workspace, Explain,
Observe, Result—to communicate progression without implying that later stages
are already available. Stage 1 combines three levels of hierarchy:

- a compact product context bar and route;
- a concise task header without redundant filter or selection panels;
- the two-reactant equation composer and periodic table as a connected working surface.

The composer mirrors a conventional reaction equation: an atomic preview,
Reactant 1, `+`, Reactant 2, and `→`, with a compact input history at the edge.
One slot always has an explicit active-input treatment. Recognised composition
previews use a named status; unknown or intermediate drafts preserve their
formula but render as one `?` particle. The viewer accepts a complete structure
only when the current host-pinned catalogue resolves the exact atom multiset to
one unambiguous structural graph. It does not run a separate permissive valence
guess. The active atomic preview uses the same deterministic shell canvas as
the former workspace: electrons orbit slowly, catalogue covalent bonds show
their shared pairs, ionic components show formal charges and association links,
and an explicit control pauses continuous motion.

Element tiles preserve their group and period positions. The reaction box sits
above the full-width periodic table, matching the direction in which elements
are carried into the workspace. Dragging adds a window-level floating preview
that remains visible over either panel and the reaction box provides the
explicit drop target.

The periodic grid contains all 118 elements in the standard seven-period
arrangement with separate lanthanide and actinide rows. It measures its
available width. Square cells expand until a readability cap, then the column
rhythm distributes the remaining width across the s, d, and p blocks. Elements
within a block stay close together; larger gaps after groups 2 and 12 preserve
the periodic families as distinct visual clusters. Horizontal and vertical
gaps are independent, allowing the table to fill its panel without making the
nine-row arrangement too tall for the one-page composition. Cells
switch to a symbol-first dense presentation at compact sizes. Populated desktop
cells show atomic number, symbol, name, and atomic mass. The redundant table
instruction strip is removed and bottom padding is zero, leaving maximum height
for the grid. All 18 groups remain visible without horizontal scrolling. The
workspace stores normalized atom positions, so its layout scales without
changing the learner's composition.

Placed atoms use direct manipulation: drag to reposition, move compatible atoms
near one another to group, and remove the selected atom with an explicit
control. Supported groups settle into a single compound card with a short-lived
animation subscription; moving or removing that card affects every member
atom. A visible reduced-motion control applies final positions immediately.
Compound cards always remain composition previews pending validation.

Stage 3 embeds deterministic Canvas diagrams inside the same draggable object
surfaces. Loose atoms use concentric hairline shells, a high-contrast nucleus,
and outer-shell electron markers. Recognised compositions retain those atomic
models within one grouped surface alongside the formula and name. A 20 FPS
subscription advances a deliberately slow orbit only while visualised atoms
are present and motion is enabled; reduced motion freezes the orbit without
hiding chemical labels. Covalent groupings add one or two explicit shared
electron pairs between shell models; ionic associations do not reuse that cue.

The consolidated composer and full periodic table occupy one fixed page. The
builder itself has no scroll container. A supported pair receives the primary
reaction action; unsupported combinations keep it disabled. Starting copies
the exact draft identities into the internal request state and opens the 2D
frame sequence only after the canonical language, catalogue, expansion, and
kernel boundaries succeed.

Stage 5 replaces the builder/table surface with a full-height guided renderer
of the trusted `SimulationFrames` sequence. Deterministic educational scenes
group the exact validated states without inventing chemistry. Stable atom IDs
persist across frames; covalent edges, dative provenance, ionic associations,
metallic domains, product membership, changes, and observations come directly
from the kernel artifact. Controls expose pause, restart, return, and a gated
transition into the macroscopic view.

The subsequent 3D page is a separate illustrative scale. It consumes a scene
plan containing reusable visual assets and observation-gated effects, not the
structural atom graph. Orbit, zoom, camera motion, timing, and fluid/effect
motion are presentation controls and carry a persistent virtual-model
disclosure.

## Tokens

The executable tokens live in `crates/chemspec-app/src/theme.rs`.

### Colour

- Canvas: `#090B0E` and raised canvas `#0C0F13`.
- Panels: `#101419`; nested surfaces: `#151A20`.
- Primary text: `#F4F7FA`; soft text: `#C3CED9`; muted text: `#9AA6B2`.
- Hairline: `#2A323C`; strong border: `#3B4856`.
- Interaction accent: `#8FC5FF` with darker accent tints for selected states.
- Semantic status: green for validated, amber for assumptions, red for errors.

Semantic colour must not be the only status cue. Pair it with text and a shape
or icon. Muted text remains readable against every canvas or panel surface.

### Type

The scale is 10, 12, 14, 16, 22, and 30 pixels. Ten-pixel text is limited to
short uppercase metadata. Body content is at least 14 pixels, and source uses a
monospace face. Sentence case is used for tasks and explanations.

### Space, radius, border, and depth

Spacing follows a 4, 8, 12, 16, 24, and 32 pixel rhythm. Controls use an 8
pixel radius, panels 12 pixels, and the outer workspace 16 pixels. Most surfaces
use a one-pixel hairline; stronger borders indicate focus, selection, or a
nested interactive surface. Only the outer workspace receives a broad, soft
shadow.

### Motion

Interaction feedback should complete in roughly 120–180 ms, panel or state
transitions in 220–300 ms, and explanatory scene transitions in 400–600 ms.
Motion must describe a state change, not run decoratively. Continuous ticks are
allowed only while playback or a real loading state is active. Reduced-motion
mode removes spatial movement while preserving opacity and status feedback.

The composer subscribes to slow orbital ticks only while its model is visible
and motion is enabled. The internal workspace subscribes only while a recognised
group is settling or the trusted frame sequence is playing. Native hover, press, focus,
selected, and disabled feedback requires no unconditional subscription. `U-104`
should apply these motion tokens when playback becomes real.

## Responsive composition

- Desktop, 1120 pixels and wider: the reactant composer and periodic table form
  one vertical working surface, with the complete table below the composer. Validated
  views place simulation and inspector side by side, with simulation receiving
  the larger share.
- Tablet, 720–1119 pixels: builder controls and validated-view regions stack
  vertically while navigation remains visible.
- Compact, below 720 pixels: header metadata and navigation labels shorten,
  controls stack, and the periodic table retains all 18 groups using dense,
  symbol-first cells without horizontal scrolling.

Responsive changes may alter composition but must not hide a product region
without a visible navigation path to it.

## Accessibility and states

- Keyboard focus on controls and the request field uses the same high-contrast
  accent as a selected inspector section.
- Interactive controls retain a visible border on dark surfaces and never rely
  on hover alone.
- Validated, assumption, offline, and future error states use explicit wording.
- The simulation disclosure remains visible near the model at every breakpoint.
- Scroll regions use stable heights and visible rails; source remains selectable
  text rather than being painted into the canvas.
