# Automatic animation system

## Decision

ChemSpec provides two separate generated experiences downstream of one current,
trusted `SimulationFrames` generation:

```text
.chems -> parse/expand/validate -> SimulationFrames
  -> EducationalScenePlanner
  -> typed scenes + narration + absolute playhead
  -> reusable 2D renderer

SimulationFrames + host-selected PresentationProfile
  -> RealWorldScenePlanner
  -> ScenePlan + typed annotations + macroscopic beat timeline
  -> asset/effect/camera registries -> 3D renderer
```

Raw structural frames are not a finished teaching timeline. The 3D experience
is not a spatialized molecular diagram. No supported reaction owns a bespoke
animation module.

## Authority boundaries

`.chems` selects the reviewed rule and preserves the mandatory representative
and explanatory disclosures. The immutable catalogue owns stable atoms,
structural states, typed operations, and observation predicates. `chem-kernel`
is the only layer that can construct the trusted `SimulationFrames` consumed by
either planner. The application may select visual assets, timing, camera cues,
and display copy through a `PresentationProfile`, but that profile cannot add or
alter chemistry.

The educational planner may choose pacing, grouping, focus, annotations, and
transitions. It may not invent or reorder chemical changes in a way that changes
their meaning. The real-world planner may instantiate only assets, effects, and
camera behaviours permitted by reviewed presentation metadata and typed
observations. It never interprets bonds or atoms as laboratory apparatus.

Educational wording follows the same boundary. `.chems` selects the experiment,
reactants, reviewed structural rule, expected products, and observations; it does
not need to contain a bespoke animation script or unstructured caption prose.
After validation, `chem-presentation` composes learner-facing wording from the
current trusted frames, typed operations, and typed observations. This is a local,
deterministic template system. It does not call an AI model, provider, network
service, or runtime text generator.

## Educational plan

The renderer-independent `EducationalPlan` is a deterministic sequence of
scenes and cues. Reusable scene roles include introduction, reactant setup,
equation, conceptual setup, structural change, product formation, and summary.
Reusable cues include focus, highlight, electron-state display, typed
structural-operation animation, equation emphasis, and transition. Typed
observations remain available to the macroscopic plan, but the guided
structural timeline does not add separate observation-explanation scenes.

The host-selected profile supplies the display equation for the current pinned
experience. Neither planner nor renderer derives stoichiometry from drawn atoms,
and the displayed equation never authorizes structural or observational state.

### Deterministic educational narration

The educational planner owns a reusable narration vocabulary keyed by typed
meaning rather than reaction names. For example, a `FormCovalent` operation
produces a bond-formation explanation targeted at the affected atoms, while an
observation predicate selects gas-evolution, consumption, product-formation, or
colour wording. Introductory and summary copy remains generic unless the pinned
presentation profile supplies learner-facing display text.

The planner emits completed typed labels such as `ContextLabel` and
`ExplanationLabel`. A label carries its semantic kind, already composed title
and text, stable atom targets, and connector policy. The 2D renderer may choose
placement, colour, typography, reveal motion, and collision avoidance for that
label. It must not:

- infer chemistry or choose scientific terminology;
- reconstruct formulae or stoichiometry from the drawing;
- turn internal stable IDs such as `species.*` or atom IDs into display names;
- contain reaction-specific caption branches; or
- substitute stale or unvalidated source text for the planner output.

Within one learning beat, context and explanation have deliberately different
jobs. Context is a concise description of the concrete trusted operation or
observation (for example, the participating element symbols and bond order),
while the explanation states its chemical meaning. Their normalized copy must
not be identical; if the planner cannot produce distinct useful context, it
omits that cue instead of repeating the explanation.

This separation makes narration modular without moving authority into prose.
A new supported reaction reuses operation and observation templates, while its
validated atom state, bond orders, electron counts, products, and observations
supply the reaction-specific content. The same trusted frame sequence, profile,
template version, and playhead always produce the
same wording, targets, and timing. A change to source or catalogue meaning
invalidates the plan before any replacement narration can be shown.

Every structural-operation cue references the exact before/after frame digests
and stable affected atom IDs. Stable atom identity is preserved visually.
Interpolation never creates a semantic state.

Each generated action is one coherent `StructuralChange` learning beat. It
carries `ApplyOperations`, `ShowContext`, and `ShowExplanation` cues together.
Normally that cue contains one exact transition. Adjacent operations are
grouped only when their typed operation signatures are equivalent, their atom
sets are disjoint, and no active observation boundary would be crossed. Every
original before/after digest remains in the cue, so the renderer can animate
the independent changes concurrently under one explanation without rewriting
the validated sequence. Callout cards target the first deterministic operation
instance rather than averaging all repeated instances into a point between
molecules. The action receives the first part of the beat, then
the explanation fades in and remains available for the adaptive reading hold. The Canvas draws the
explanation as a rounded glass card with a semantic eyebrow, accent rail, short
connector, and target halo. Connector lines have no arrowhead unless direction
itself carries scientific meaning. This merged beat keeps the change and its
meaning in one visual context without duplicating labels or interrupting flow.

The 2D renderer evaluates every visual from an absolute educational playhead.
The media timeline is scrubbable, displays elapsed and total time, exposes
chapter boundaries, and provides explicit previous/next chapter controls.
Seeking uses `EducationalPlan::locate`; exact boundaries, zero-duration scenes,
and end clamping therefore share one deterministic contract with automatic
playback. A tick preserves elapsed overshoot across scene boundaries instead of
discarding it.

Atoms are laid out as deterministic connected components using only trusted
covalent bonds, full charged components of ionic associations, and
metallic-domain membership. An ionic association is never collapsed to an
arbitrary atom pair: its visual anchor is the formal-charge-bearing atom when
one is present, with a component-centre fallback. Stable
component slots prevent unrelated molecules from reshuffling when a local
operation changes. Before/after relations are rendered together: new bonds and
associations trace in, removed ones retract, and metallic domains morph without
a hidden midpoint frame swap. Electron presentation is conserved rather than
crossfaded: persistent shell electrons are drawn once, transferred electrons
move from the metallic domain to their validated acceptor slots, covalent
formation moves the contributing shell electrons into one shared pair, and
cleavage moves that same pair into the validated new shell states. Lone-pair and
unpaired-electron placement comes from the typed atom state. All motion is a pure
function of the playhead, so pause and scrub reproduce the same visual state
exactly.

## Real-world scene plan

The renderer-independent `ScenePlan` contains:

- reusable environment, vessel, material-form, and physical-state asset IDs;
- semantic identities and reviewed appearance profiles kept separate from mesh
  identity;
- full deterministic translation, rotation, and scale transforms;
- deterministic semantic annotations composed from the reviewed equation and
  typed observations;
- an absolute, variable-duration macroscopic beat timeline;
- typed reusable effects with bounded parameters;
- reusable near-isometric camera behaviours and timing;
- the validated reaction/catalogue identities and virtual-only disclosure.

The macroscopic planner derives beat boundaries from reviewed object
visibility, effect ranges, camera ranges, and typed observation ordinals. The
same plan drives playback, the scrubbable elapsed/total-time control, current
scene number, annotations, objects, and effects. The macroscopic renderer uses
a fixed orthographic camera; legacy camera cues may still partition timeline
beats but do not change the rendered pose.
`RealWorldTimeline::locate` clamps at the exact end and maps every playhead
position to a typed ordinal and beat progress; the UI does not advance through
an unrelated fixed list of manual stages.

Macroscopic annotations are planner output, not renderer prose. Initial and
final labels use reviewed equation sides, while event labels use typed
observations. Effects remain distinct typed phenomena: for example, bubbles in
the liquid and colourless gas leaving the surface are separate reusable effects.
No effect is selected unless reviewed presentation metadata authorizes it and
its declared observation trigger belongs to the current validated reaction.

The initial registry is intentionally small but architectural: a laboratory
bench, presentation platform, beaker, liquid volume, generic metal chunk,
precipitate cloud, bubbles, and reusable lighting/camera rigs. New common assets
extend the registry; new reactions select existing profiles whenever possible.
Assets may be stored meshes or deterministic procedural low-poly meshes, but
runtime selection never regenerates reviewed common assets per reaction.

The fixed vessel camera begins above the rim and targets the liquid/reaction surface.
Scene anchors ground the vessel on the bench, keep liquid inside it, place
reactants at the reaction interface, and locate gas or precipitate in the
appropriate region. The renderer applies each complete reviewed transform and
uses a stable object-ID seed for controlled low-poly variation, so meshes do not
change shape as the playhead advances.

The default lighting combines ambient, key, fill, hemispheric, and rim terms so
clear glass, liquid, reactants, and effects remain readable. Three-dimensional
playback advances continuously through the complete scene plan; play/pause,
restart, timeline, and speed are presentation controls rather than manual
chemistry-stage gates. Orbit, pan, zoom, shake, and cinematic camera motion are
disabled. Opaque geometry is depth-written first, followed by alpha-blended
liquid, effects, and glass with depth writes disabled. A bounded additive pass
follows only for typed flame cores and sparse sparks; the coloured flame body
stays alpha blended to preserve its reviewed palette. All passes share Iced's
existing GPU target and depth buffer; the app does not open or own a second
renderer or event loop.

### Reusable natural motion

Natural motion is a renderer concern downstream of the typed scene plan. Each
reviewed effect profile and intensity resolves to reusable dynamics such as
particle count, emission rate, spread, lift, turbulence, attack, and release.
`ReactionVisualInputs` converts those active typed effects into continuous
normalized gas, bubble, liquid, pressure, heat, precipitation, colour, and
splash controls, plus a flame rate only for a typed `FlameEmitter`. Missing
metadata remains zero. These dynamics never select chemistry and contain no
reaction identity checks.

Particles use a stable seed derived from the plan and typed effect. Their birth
times, speeds, sizes, directions, arcs, curls, and settling positions therefore
vary organically while remaining exactly reproducible when paused or scrubbed.
Motion uses the absolute ordinal plus progress as one continuous phase, so an
emitter does not visibly restart at a beat boundary. Smooth attack and release
envelopes prevent effects from popping into existence; persistent phenomena,
such as an accumulated precipitate, keep their final state instead of fading
away.

State boundaries do not restart motion. A reactant begins beyond the vessel rim,
receives a seeded launch impulse, and follows an analytic gravity-driven arc
with low horizontal drag and independent angular momentum across the complete
setup interval. The conservative missing-metadata default completes this entry
in 0.9 seconds, replacing the previous four-second slow glide. At the first
observation-authorized effect it reaches the reaction surface exactly, receives
an inelastic contact impulse, and settles through rapidly damped rebound,
tangential slip, and rotational follow-through. Product geometry remains absent
before its trusted visibility ordinal, then grows from zero with an asymmetric
formation response. This removes static holds, spline-like easing, and one-frame
pops without overlapping into an unvalidated chemistry state.

Bubble and flame lift accelerate toward terminal velocity rather than following
an ease curve. Splash droplets use a parabolic ballistic arc with horizontal
drag, precipitate accelerates downward before settling, and gas, flame, bubble,
and liquid-interface drift combine independently seeded flow frequencies so
particles do not move in synchronized waves.

Macroscopic beat durations also use faster generic defaults: strong activity
uses 2.6 seconds, moderate activity 3.4 seconds, and subtle activity 4.4
seconds. A final inactive settling beat lasts at least 2.4 seconds. These values
are presentation defaults, not claimed kinetic measurements; reviewed timing
metadata can supersede them when the language gains a generic measured-rate
contract.

### Motion implementation references

The model follows primary implementation guidance rather than a bespoke visual
timeline:

- [Box2D simulation documentation](https://box2d.org/documentation/md_simulation.html)
  distinguishes airborne damping from contact friction, describes restitution
  as an impact response, and recommends stable fixed simulation steps. ChemSpec
  uses the same force/impact concepts but samples closed-form trajectories from
  its trusted playhead so pause, replay, and scrubbing remain exact.
- Bridson, Houriham, and Nordenstam's
  [Curl-Noise for Procedural Fluid Flow](https://www.cs.ubc.ca/~rbridson/docs/bridson-siggraph2007-curlnoise.pdf)
  motivates rotational multi-scale procedural flow for natural turbulence.
  ChemSpec uses a bounded curl-like approximation, not a claimed fluid solver.
- NVIDIA's [Fire in the Vulcan Demo](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-6-fire-vulcan-demo)
  motivates age-dependent emitter attachment, detachment, rolling motion, and
  varied particle lifetimes.

The flame emitter uses tapered faceted particles rather than a
reaction-specific timeline. Young lobes remain concentrated at the reaction
surface; age-dependent lift, detachment, curl, scale, and dissipation produce a
continuous turbulent plume. The seed includes the palette-bearing typed effect,
so replay and scrubbing reproduce the same flame geometry whenever a reviewed
profile authorizes that palette.

The reacting object, splash, ripples, bubbles, and gas share a gently moving
reaction-surface anchor. Liquid uses a seeded displaced surface and raised
meniscus. Normal-view gas uses connected irregular low-poly shells rather than
visible parcel or molecule dots. The fixed orthographic camera keeps this
motion readable without changing the reviewed outcome, inventing an effect, or
adding animation instructions to a particular `.chems` file.

## Post-simulation product record

After the macroscopic timeline reaches its exact end, the application may
compile a final product record directly from the trusted final
`SimulationFrame`. Product membership determines which atoms belong together;
the frame supplies element identity, formal charge, non-bonding electrons,
covalent edges, ionic associations, and metallic domains. Duplicate validated
instances are grouped for presentation without changing their coefficient.

The record renders a perspective-projected model rotating around a real
three-dimensional axis. This summary model is explicitly representative
geometry, not the macroscopic
real-world `ScenePlan` and not a molecular-dynamics claim. Ionic associations
remain enclosures rather than invented covalent bonds. Property wording and
values are deterministic local templates; the optional reference molar mass is
summed using fixed decimal element metadata rather than binary floating point.
No AI, network request, reaction-name branch, or runtime code generation is
used.

## Blocking and invalidation

Malformed, ill-typed, incomplete, invalid, unsupported, stale, and system-error
results produce neither plan. Source, catalogue, validated-artifact, frame, or
presentation-profile digest changes invalidate both experiences. A missing
presentation profile may still permit the 2D educational plan, but blocks the
real-world scene honestly rather than guessing.

## Verification

Tests must prove:

- planners are deterministic and reaction-agnostic;
- educational labels are composed without AI or network access from the current
  validated reaction, frames, typed operations, equation, and observations;
- operation and observation templates interpolate validated display values
  across multiple reactions rather than selecting reaction-specific prose;
- every rendered chemistry label is supplied by the educational plan, and the
  renderer neither humanizes internal IDs nor authors chemistry wording;
- every educational operation cue maps to a validated operation and frame pair;
- every scene object/effect/camera cue resolves in its registry;
- macroscopic annotations and timeline beats are deterministic planner output
  derived from the current validated reaction and reviewed metadata;
- no effect appears without reviewed metadata and, where required, a matching
  typed observation;
- semantic identity and mesh identity remain distinct;
- full transforms are applied, scene roles resolve to physically coherent
  anchors, and deterministic variation changes with the stable object seed only;
- source or catalogue changes stale both plans;
- both renderers use Iced's existing renderer/device boundary;
- educational seeking maps to the typed scene and end-frame at exact variable
  duration boundaries, pauses playback, and clamps at the end;
- 2D relation/electron transitions are continuous and deterministic at every
  playhead position, including scrubbing backwards;
- macroscopic seeking, beat interpolation, exact-end clamping, and ordinal
  synchronization are deterministic, and scrubbing pauses playback;
- effect phase remains continuous across ordinal boundaries, seeded particle
  geometry reproduces exactly, and transient envelopes begin and end at rest;
- opaque and transparent draw ranges are non-overlapping, depth-tested, and
  rendered in the required order;
- live smoke tests show continuous 2D playback and a depth-tested macroscopic
  3D diorama.
