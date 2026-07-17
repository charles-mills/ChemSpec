# Macroscopic 3D visual system

## Scope and trust boundary

The macroscopic page is an illustrative, stylised 3D presentation downstream
of the current trusted `SimulationFrames`. It is implemented with Iced 0.14 and
the application's existing `wgpu` device; ChemSpec does not use Bevy.

```text
.chems 1 source
  -> parse, reviewed-rule expansion, and structural validation
  -> trusted SimulationFrames and typed observations
  -> reviewed catalogue macroscopic phases (when available)
  -> generic phase-driven PresentationProfile or reviewed legacy profile
  -> observation-gated PresentationEffect values
  -> normalized ReactionVisualInputs
  -> reusable liquid, gas, bubble, splash, flame, and precipitate rendering
```

`ReactionVisualInputs` contains normalized illustrative controls such as
reaction progress, gas generation, bubble rate, pressure impulse, heat output,
liquid turbulence, precipitation, colour transition, splash rate, and flame
rate, plus a bounded container-vibration control derived from active dynamic
channels. These
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
| `LiquidMixing` | subsurface rotational flow and liquid turbulence |
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

## Phase and outcome selection

Macroscopic phase is now an optional schema-1 catalogue fact. It is not
inferred from a chemical name, formula, colour, or structural representation.
The generic record is:

```json
{
  "structure": "CarbonDioxide",
  "context": { "kind": "standard" },
  "phase": { "kind": "gas" },
  "premise_ids": ["premise.material.carbon-dioxide.standard-phase"]
}
```

When a material's state depends on the reaction medium, a reviewed rule-role
override can be more specific:

```json
{
  "structure": "ExampleSalt",
  "context": {
    "kind": "reaction_role",
    "rule": "Rules.ExampleAqueousReaction",
    "role": "saltProduct"
  },
  "phase": { "kind": "aqueous" },
  "premise_ids": ["premise.material.example-salt.aqueous"]
}
```

Resolution order is exact rule-role context first, then standard context. Each
record must reference an existing structure, an existing rule role, and at
least one existing reviewed premise. Duplicate contexts and unknown roles fail
catalogue validation with `CHEMS-C024`. The field is optional and omitted by
older catalogues, so their canonical bytes and digest remain unchanged.

The trusted-run adapter retains the expanded reactant/product bindings and
resolves all of them through this catalogue layer before constructing generic
visual input. If even one phase is unavailable, the application does not fill
the gap with a name-based guess: current reviewed experiences retain their
existing host profile. A future reaction becomes fully generic once its
validated structures/rule and reviewed macroscopic records are present; no Rust
animation branch for that reaction is required.

The phase-driven compiler applies the following reusable rules:

- solid reactants use faceted powder/shard geometry, or a low-poly metallic
  chunk when the already-validated representation is metallic;
- liquid and aqueous reactants share one visible mobile phase, avoiding stacked
  transparent liquid volumes;
- a gas product observed as either `evolves` or `forms` uses the connected gas
  cloud and gas expansion; bubbles are added only when a liquid/aqueous phase
  is present;
- a solid product observed as `forms` becomes a settling precipitate only when
  it forms in a mobile phase, otherwise it uses a dry crystal/shard cluster;
- a liquid or aqueous product drives mixing rather than precipitation; and
- colour observations bind to the phase object independently, so the same
  reviewed colour can propagate through liquid, gas, or solid geometry.

These rules cover phase combinations such as gas + gas -> gas, gas + gas ->
liquid, solid + gas -> gas, aqueous + aqueous -> solid, and solid + liquid ->
gas without knowing the reaction name. Effect intensity remains conservative
unless separately reviewed kinetics, heat, flame, or pressure metadata exists.

The renderer does not derive products from chemical names or formula patterns.
It accepts only the validated products and typed observations that survive the
catalogue and kernel boundary:

- `gas <product> evolves` can authorize a `GasCloud`, gas release, and, when a
  mobile phase exists, bubbles;
- `product <product> forms` can authorize a gas cloud or gas expansion only
  when the same product is catalogue-resolved as gas;
- `product <product> forms` establishes formation but does not, by itself,
  claim that the product is a precipitate;
- precipitate and clouding visuals additionally require reviewed
  precipitation presentation metadata bound to that `forms` observation;
- a colour observation may select the reviewed appearance of an already
  authorized solid product, but cannot turn a dissolved or gaseous product into
  a solid; and
- acid-base neutralization remains an aqueous mixture when its validated source
  only establishes acid disappearance and water formation. Those observations
  may authorize generic liquid mixing and surface response, but do not invent
  gas, precipitate, flame, or an indicator colour.

The plan compiler rejects precipitate effects driven by `evolves`, gas effects
driven by `disappears`, and similar mismatches. A gas asset or gas-expansion
effect may use `forms` because the phase-driven compiler supplies the separate
reviewed gas-phase fact; `forms` alone still cannot establish phase.
For example, a future carbon oxidation simulation would need a reviewed rule
that validates carbon dioxide as the product and a `.chems` observation stating
that the gas evolves. The renderer will not guess carbon dioxide from the names
"carbon" and "oxygen" or from participant phases alone.

### Colour values and transitions

The existing `.chems 1` statement is the colour authority:

```chems
product productBinding has colour Cream claim R2
```

The exact subject, value, evidence claim, catalogue compatibility, and trigger
ordinal must survive validation before colour can reach the scene plan. A
presentation colour binding that names another product, starts early, uses an
unsupported value, or supplies RGB values inconsistent with the observed name
is rejected.

Common values include `Colourless`, `White`, `Cream`, `Yellow`, `Amber`,
`Orange`, `Red`, `Crimson`, `Pink`, `Purple`, `Violet`, `Blue`, `Cyan`, `Green`,
`Olive`, `Brown`, `Grey`/`Gray`, and `Black`. An exact arbitrary sRGB colour can
use the already-valid qualified-name form `Rgb.HexRRGGBB`, for example
`Rgb.Hex12ABEF`. The evidence packet and reviewed catalogue compatibility must
use that exact value; the RGB convention does not bypass chemistry validation
and does not change the `.chems 1` grammar.

Colour is independent of phase. The renderer applies the observed RGB target
to liquid, solid, or gas geometry while preserving the phase material's
opacity. Liquid colour spreads from the reaction region with radial and
vertical delay, gas lobes transition with turbulent seeded offsets, and solid
facets change in staggered patches. At the trusted transition endpoint every
vertex reaches the exact requested colour. Gas release, precipitate fragments,
bubbles, droplets, ripples, and mixing-current highlights inherit the current
macroscopic phase colour rather than using a hardcoded blue or white.

## Reusable rendering

- Gas products and gas release render as connected, irregular low-poly shells.
  Seeded procedural lobes create expansion, curl, density variation, and
  dissipation without exposing the invisible parcels as molecular beads.
- Liquid uses a low-resolution displaced surface. Multiple damped waves,
  seeded phase variation, edge damping, and a raised meniscus respond to the
  continuous turbulence input. Observation-gated liquid mixing adds faint
  subsurface helical flow tracers so convection remains readable in an
  otherwise colourless solution without pretending that new coloured matter
  exists.
- Bubbles remain discrete because bubbles are visible macroscopic interfaces.
  Their rise, wobble, size, birth phase, and fade are seeded and reproducible.
- Splashes use ballistic arcs, while precipitate forms persistent pointed,
  flat-shaded shards. The fragments accelerate under gravity, tumble with
  seeded angular momentum, lose lateral drift through the liquid, make a small
  damped vessel-floor rebound, and then settle. Final crystal, powder, and
  precipitate product clusters use the same faceted shard language rather than
  perfect spheres. Both remain selected by typed effects rather than reaction
  names.
- Reactants are released above the vessel centre and accelerate downward under
  gravity with seeded air drift, restrained angular momentum, and a short
  plunge-and-rebound impact response instead of following an eased approach
  spline. Products remain invisible before their trusted observation,
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
- Dynamic gas, pressure, turbulence, splash, and flame channels contribute to a
  tiny seeded vessel displacement. Vessel, liquid, products, and effects move
  together by less than one percent of the vessel radius; the bench and fixed
  camera never move. Persistent precipitate alone contributes no vibration, so
  a gentle settling reaction is not portrayed as violent.
- Opaque geometry is depth-written first. Liquid, effects, and glass share a
  later depth-tested alpha pass, followed by one batched additive flame-core
  pass, to keep vessel contents legible.

## Scripted deterministic physics

The trusted scene plan decides **what** may happen and the absolute ordinal and
time range in which it happens. Reusable renderer physics decides **how** that
authorized motion travels between those boundaries. The current model uses
closed-form gravity, acceleration toward terminal velocity, drag, ballistic
arcs, angular momentum, seeded curl-like flow, damped contact impulses, surface
waves, buoyant rise, and dissipation. This makes motion physical-looking without
allowing a solver to invent a product, effect, or reaction stage.

Every position and rotation is sampled from the absolute playhead and a stable
seed. Pause, replay, seeking, and identical validated inputs therefore reproduce
the same state exactly. This is intentionally not a free-running rigid-body or
computational-fluid-dynamics solver: such a solver would make scrubbing and
reviewed timing nondeterministic, require collision geometry for every vessel,
and could drift across a trusted observation boundary. Gas and liquid use
bounded macroscopic approximations; solid fragments use point trajectories and
a simplified liquid/floor collision rather than arbitrary mesh-to-mesh contact.

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
kinetics, general rigid-body solver, shard-to-shard collision, foam,
flame-fluid coupling, heat refraction, and soft-particle depth fading are
intentionally not claimed by this implementation.
