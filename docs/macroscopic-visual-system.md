# Macroscopic 3D visual system

## Scope and trust boundary

The macroscopic page is an illustrative, stylised 3D presentation downstream
of the current trusted `SimulationFrames`. It is implemented with Iced 0.14 and
the application's existing `wgpu` device; ChemSpec does not use Bevy.

```text
.chems 1 source
  -> parse, reviewed-rule expansion, and structural validation
  -> trusted SimulationFrames and typed observations
  -> host-selected PresentationProfile
  -> observation-gated PresentationEffect values
  -> normalized ReactionVisualInputs
  -> reusable liquid, gas, bubble, splash, flame, and precipitate rendering
```

`ReactionVisualInputs` contains normalized illustrative controls such as
reaction progress, gas generation, bubble rate, pressure impulse, heat output,
liquid turbulence, precipitation, colour transition, splash rate, and flame
rate. These
values are not measured kinetics or thermodynamics. They are derived only from
typed effects already authorized by a validated observation. A missing effect
produces zero rather than a chemical-name guess.

## `.chems` compatibility

This system makes no `.chems 1` schema change. Existing files remain valid.
The language continues to describe reviewed reactants, products, equation,
representative/explanatory model disclosures, typed observations, and the
selected reviewed rule. It does not describe particles, animation frames,
camera paths, rate constants, pressure, temperature, or apparatus.

The current generic visual mapping is:

| Typed effect | Continuous visual inputs |
| --- | --- |
| `BubbleEmitter` | bubble rate and mild liquid turbulence |
| `GasRelease` | gas generation and a conservative pressure contribution |
| `SurfaceDisturbance` | liquid turbulence |
| `SplashEmitter` | droplets, turbulence, and pressure impulse |
| `PrecipitateFormation` / `Clouding` | precipitate generation |
| `ColourTransition` | colour transition |
| `HeatDistortion` | heat-output presentation |
| `FlameEmitter(palette)` | flame rate, illustrative heat, and mild local turbulence |
| `ObjectShrinkage` | reactant-scale progression only |

Foam, exact heat, and violent pressure behavior remain inactive unless a
future reviewed, general-purpose presentation effect authorizes them. Flame is
also zero by default and appears only when reviewed qualitative presentation
metadata selects `FlameEmitter`.

## Reusable rendering

- Gas products and gas release render as connected, irregular low-poly shells.
  Seeded procedural lobes create expansion, curl, density variation, and
  dissipation without exposing the invisible parcels as molecular beads.
- Liquid uses a low-resolution displaced surface. Multiple damped waves,
  seeded phase variation, edge damping, and a raised meniscus respond to the
  continuous turbulence input.
- Bubbles remain discrete because bubbles are visible macroscopic interfaces.
  Their rise, wobble, size, birth phase, and fade are seeded and reproducible.
- Splashes use ballistic arcs, while precipitate uses persistent settling
  particles. Both remain selected by typed effects rather than reaction names.
- Reactants are thrown with seeded impulse, gravity, low air drag, angular
  momentum, and a short inelastic impact response instead of following an eased
  approach spline. Products remain invisible before their trusted observation,
  then form with an asymmetric response rather than popping into the scene.
- Bubbles and flame parcels accelerate toward terminal rise, droplets use
  drag-limited parabolic trajectories, sediment accelerates as it settles, and
  parcel drift combines multiple seeded rotational frequencies to avoid
  synchronized mechanical motion.
- Flame uses bounded, tapered low-poly lobes with staggered lifetimes, upward
  acceleration, widening curl after detachment, seeded turbulence, and smooth
  attack/release. Its coloured body is alpha blended; only the smaller core
  and sparse sparks use the additive emissive pass. This preserves palette
  control while providing a bright centre without saturating the whole plume.
- Opaque geometry is depth-written first. Liquid, effects, and glass share a
  later depth-tested alpha pass, followed by one batched additive flame-core
  pass, to keep vessel contents legible.

## Evidence-backed alkali-water ignition

The reviewed alkali-water presentation metadata deliberately distinguishes a
reaction observation from a flame-test colour. The Royal Society of Chemistry
describes lithium in water as fizzing, while potassium in water moves very
quickly and self-ignites with a lilac flame. Therefore potassium-water selects
`FlameEmitter(Lilac)` at its trusted gas-evolution ordinal; lithium-water and
sodium-water do not select a flame. A lithium compound's crimson flame-test
colour is not used to invent ignition in the water reaction.

Sources:

- [RSC alkali metals with water observation table](https://edu.rsc.org/download?ac=512063)
- [RSC Group 1 periodic trends and flame colours](https://edu.rsc.org/infographics/looking-at-groups-1-7-and-0-on-the-periodic-table/4020691.article)

The rendering structure follows established particle guidance rather than a
reaction-specific animation. NVIDIA's fire case study notes that alpha
blending retains flame/smoke colour control while fully additive particles can
saturate, and recommends keeping young particles attached to a moving emitter
before allowing them to detach. ChemSpec applies those principles with a
small additive core, continuous seeded births, and age-dependent curl. The
implementation stays inside Iced 0.14's custom `wgpu` shader widget and batches
each blend class into one indexed draw range.

- [NVIDIA GPU Gems: Fire in the Vulcan Demo](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-6-fire-vulcan-demo)
- [Iced 0.14 custom shader widget](https://docs.rs/iced/0.14.0/iced/widget/shader/)

## Fixed 2.5D camera

The macroscopic page uses one orthographic, near-isometric camera angle. Mouse
dragging, wheel zoom, free panning, camera shake, focus drift, and cinematic
camera interpolation are disabled. The camera targets the reaction region from
above the vessel rim. A deterministic orthographic view height is calculated
from vessel scale so differently sized vessels remain framed without changing
the viewing angle.

Legacy camera-cue values remain in `PresentationProfile` for serialized and
timeline compatibility, but the macroscopic renderer deliberately ignores
their pose changes.

## Adding reactions and effects

A new member of an existing reviewed reaction family does not need a Rust
animation script. For example, the existing acid-carbonate source declares:

```chems
observe from Evidence.AcidCarbonateGasEvolution@1
  gas carbonDioxide evolves claim R1
  reactant hydrochloricAcid disappears claim R2
```

After the normal catalogue and kernel checks, its family presentation profile
binds those observations to the shared bubble, gas-release, and surface-motion
effects. Sodium, potassium, carbonate, and bicarbonate members reuse the same
effect implementation and continuous visual-input mapping. No renderer branch
checks a reaction or chemical name.

Potassium-water demonstrates a second generic route. Its reviewed family
metadata selects the palette-bearing `FlameEmitter` and binds it to the trusted
`Evolves` observation. The renderer receives only the effect, palette,
intensity, active ordinal, and seed; it contains no potassium, lithium, source,
formula, or reaction-name branch.

A genuinely new visual phenomenon should be added vertically:

1. define a general `EffectProfile` variant and normalized dynamics;
2. authorize it only from reviewed presentation metadata and a compatible
   typed observation;
3. map it to `ReactionVisualInputs`;
4. implement one reusable renderer primitive;
5. add trigger-validation, determinism, boundary, and multi-reaction tests; and
6. update this document and the verification contract.

Do not add reaction-specific renderer modules or `.chems` animation fields.

## Performance and diagnostics

The gas and liquid meshes use bounded low-resolution geometry, deterministic
seeds, fixed vertex/index buffer capacities, and no additional GPU device or
event loop. The same playhead position reconstructs the same geometry. The
normal UI shows the active typed effect labels; detailed visual metrics remain
test/debug data and are not presented as scientific measurements.

The current CPU renderer rebuilds bounded procedural geometry for each redraw.
A future optimization can cache static vessel/environment geometry and update
dynamic gas/liquid meshes at a lower fixed rate with interpolation. A true
volumetric fluid solver, collision-complete gas field, physically measured
kinetics, foam, flame-fluid coupling, heat refraction, and soft-particle depth
fading are intentionally not claimed by this implementation.
