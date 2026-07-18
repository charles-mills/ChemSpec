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
guided structural simulation becomes the dominant stage. Source and provenance
remain distinct, and presentation never changes validation meaning.

## Reaction-builder composition

Stage 1 is the learner's question. The screen carries exactly two regions and
no chrome: the sentence "What happens when `X` reacts with `Y`?" whose
reactant slots are interactive chips, and the full periodic table below it.
There is no header, no route strip, no status footer, no input-history panel,
and no separate model-preview panel. Compact widths swap the sentence for the
denser `X + Y → ?` equation form.

Slot borders carry the draft's state as colour: green for a valid reactant (a
recognised composition or a lone element), orange for an unrecognised draft,
and grey when empty. An empty slot shows a muted "?" in the formula style.
The selected slot adds a blue background tint and thicker border from the
dedicated selection token, so selection reads independently of state. The
words behind every colour live in the slot tooltip, never in a status strip.
Under the sentence sit the primary "Run reaction" action — disabled until the
pair is supported — followed by Undo and Clear at the same control height.

Hovering a non-empty slot opens a tooltip that never repeats the formula or
symbol already visible in the slot: a single atom shows its full name, atomic
number, and periodic family; a recognised composition shows the compound
cluster with shared pairs and its chemical name; an unrecognised draft shows
its member atoms with an explicit unrecognised label. Every
tooltip ends with the direct gesture hint; empty slots carry no tooltip.
Electrons orbit only while a tooltip is open, so moving the cursor away
pauses motion without a dedicated control; the tooltip itself fades in over
the standard interaction window.

Slots support direct gestures alongside the buttons: clicking an inactive
slot selects it, clicking the active slot undoes its last element, and
pressing and holding any filled slot clears it. The hold shows a radial
progress wheel over the slot that must complete before anything is destroyed;
releasing early falls back to a click, and leaving the slot cancels the hold.

Element keys preserve their group and period positions. Dragging adds a
window-level floating preview that remains visible over every surface, and
the sentence slots are the explicit drop targets.

The periodic grid contains all 118 elements in the standard seven-period
arrangement with separate lanthanide and actinide rows. The table renders as a
quiet keyboard centred in its remaining space: keys are slightly wider than
tall and expand until a readability cap bounded by both the available width
and height. Elements within a block stay close together; slightly larger gaps
after groups 2 and 12 preserve the periodic families as distinct visual
clusters. Every key carries its atomic number, symbol, full element name, and
a bottom colour tick in its periodic-family colour, so no separate legend,
group-number strip, or instruction copy is required; keys switch to a
symbol-first dense presentation at compact sizes. All 18 groups remain visible
without horizontal scrolling.

The atomic tooltips use deterministic Canvas diagrams: concentric hairline
shells, a high-contrast nucleus, and outer-shell electron markers. Recognised
compositions retain those atomic models within one cluster alongside the
formula and chemical name. A tick subscription advances a deliberately slow
orbit and the tooltip's reveal fade only while a slot tooltip is open; leaving
the slot freezes the orbit. Covalent groupings add one or two explicit shared
electron pairs between shell models; ionic associations do not reuse that cue.

The sentence composer and full periodic table occupy one fixed page. The
builder itself has no scroll container. A supported pair receives the primary
reaction action; unsupported combinations keep it disabled. Starting copies
the exact draft identities into the internal request state and opens the 2D
frame sequence only after the canonical language, catalogue, expansion, and
kernel boundaries succeed.

The animation stage replaces the builder/table surface with a full-height guided renderer
of the trusted `SimulationFrames` sequence. Deterministic educational scenes
group the exact validated states without inventing chemistry. Stable atom IDs
persist across frames; covalent edges, dative provenance, ionic associations,
metallic domains, product membership, changes, and observations come directly
from the kernel artifact. Atom charge badges present formal charge outside a
metallic domain. They do not show a metallic site's positive core charge in
isolation because its domain-owned delocalized electrons balance that core;
the renderer continues to show the metallic halo and electron domain instead.
Once a site leaves the metallic domain, its genuine formal charge is eligible
for the ordinary charge badge. Controls expose pause, restart, return, and a
gated transition into the macroscopic view.

The structural renderer does not apply the main-group Lewis-dot layout to
transition metals. It keeps their metallic-domain halo but omits stationary
site and domain electron dots, including bookkeeping-only release/join motion.
Typed electron-transfer operations still draw the exact electrons in flight,
and validated ionic charge badges remain visible. Main-group atoms retain the
ordinary Lewis-dot presentation.

Ionic lattice layout includes every validated component for non-1:1 formula
ratios. Charge-alternating grid neighbours are preferred; any excess ions are
connected to their nearest opposite-charge lattice neighbour for presentation,
so no component becomes a detached layout island. These connectors express
many-body ionic membership, not covalent bonds or discrete molecules.

The subsequent 3D page is a separate illustrative scale. It consumes a scene
plan containing reusable visual assets and observation-gated effects, not the
structural atom graph. Its elevated near-isometric camera is orthographic and
fixed: there is no orbit, pan, zoom, shake, or cinematic camera motion. Timing
and fluid/effect motion remain illustrative and carry a persistent
virtual-model disclosure.

Completing the macroscopic timeline unlocks a final product record. On desktop,
the page uses an even split: the left side is a full-height, continuously
rotating perspective 3D product model, while the right side presents molecule
properties as a staggered typewriter readout. The model is compiled from
final-frame product membership, atoms, formal charges, and covalent
relationships; ionic products use an association enclosure instead of fake
bonds. Covalent lines and their multiplicity come only from validated edges;
generic VSEPR-informed geometry uses connectivity and non-bonding-electron
counts to keep bonded atoms legible without reaction-specific layout code.
Formulae, composition, structure class, atom and bond counts,
net formal charge, and exact-decimal reference molar mass are deterministic
local presentation values. The renderer never selects a product by reaction
name.

The result screen keeps the established instrument language: nested near-black
surfaces, one-pixel borders, compact uppercase eyebrows, restrained green and
blue accents, broad model whitespace, and explicit validation wording. Below
1080 pixels the model and properties stack in the same information order inside
one vertical scroll region. Property rows switch to a vertical label-value
composition, and the desktop record uses height-aware density plus its own
scroll boundary so long values and short viewports do not clip. The 3D product
completes one 360-degree revolution in roughly 18 seconds, while property rows
reveal sequentially with a visible cursor and
stable row geometry.

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

Electricity-driven electron-transfer scenes label oxidation at the anode and
reduction at the cathode. They do not draw an electron route directly between
ions; the explanation identifies the external circuit as the electron path.

The motion cadence and step tokens live in `theme::motion`. The composer
subscribes to ticks only while a slot tooltip is open: the same subscription
advances the slow orbit and the tooltip's ease-out reveal, and ends when the
cursor leaves. An activated element key does not stay highlighted; it flashes
in its family colour and fades back over about a second with a quadratic
tail, like a released key, ticking only while the fade runs. Playback ticks
run only while the trusted frame sequence is playing. Native hover, press,
focus, selected, and disabled feedback requires no unconditional
subscription.

The product record is the one intentional post-playback continuous view: its
subscription advances the user-requested rotating 3D model and deterministic
typewriter reveal only while that screen is visible. Leaving the screen removes
the subscription. The same elapsed value reconstructs the same model angle and
text state.

## Responsive composition

- Desktop, 720 pixels and wider: the question sentence sits above the
  periodic table, which centres in the remaining space at its readability cap.
- Compact, below 720 pixels: the sentence becomes the `X + Y → ?` equation
  form, and the periodic table retains all 18 groups using dense,
  symbol-first keys without horizontal scrolling.

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
