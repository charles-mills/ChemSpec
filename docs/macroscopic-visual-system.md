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
  -> closed chemistry-process classification (when exact facts support it)
  -> generic phase-driven PresentationProfile or reviewed legacy profile
  -> observation- or process-authorized PresentationEffect values
  -> normalized ReactionVisualInputs
  -> reusable liquid, gas, bubble, splash, flame, and precipitate rendering
```

`ReactionVisualInputs` contains normalized illustrative controls such as
reaction progress, gas and hot-vapour generation, bubble rate, pressure
impulse, heat output, liquid turbulence, precipitation, colour transition,
splash rate, and flame rate, plus a bounded container-vibration control derived
from active dynamic channels. These
values are not measured kinetics or thermodynamics. They are derived only from
typed effects authorized by a validated observation or a closed chemistry
process established upstream. A missing effect produces zero rather than a
chemical-name guess.

## `.chems` compatibility

This system makes no `.chems 1` schema change. Existing files remain valid.
The language continues to describe reviewed reactants, products, equation,
representative/explanatory model disclosures, typed observations, and the
selected reviewed rule. It does not describe particles, animation frames,
camera paths, rate constants, pressure, temperature, or apparatus.

The current generic visual mapping is:

| Typed effect | Continuous visual inputs |
| --- | --- |
| `ReactionActivity` | conservative phase-neutral secondary motion |
| `BubbleEmitter` | bubble rate and mild liquid turbulence |
| `GasRelease` | gas generation and a conservative pressure contribution |
| `VapourRelease` | buoyant hot-vapour/condensing-mist cue, heat, and mild expansion |
| `SurfaceDisturbance` | liquid turbulence |
| `LiquidMixing` | subsurface rotational flow, liquid turbulence, and—when an existing liquid contents phase is present—a reusable virtual stirring rod |
| `SplashEmitter` | droplets, turbulence, and pressure impulse |
| `SolidFormation` | dry solid nucleation and faceted growth |
| `PrecipitateFormation` / `Clouding` | precipitate generation |
| `ColourTransition` | colour transition |
| `HeatDistortion` | heat-output presentation |
| `FlameEmitter(palette)` | flame rate, illustrative heat, and mild local turbulence |
| `ObjectShrinkage` | reactant-scale progression only |

Foam, exact heat, and violent pressure behavior remain inactive unless a
future reviewed, general-purpose presentation effect authorizes them. Flame is
also zero by default. It appears only when reviewed qualitative presentation
metadata selects `FlameEmitter`, or when the chemistry layer establishes the
closed `CompleteCombustion` process described below.

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

Older host profiles are completed by one generic compatibility pass. Before
this pass, 172 of the 205 currently supported experiences had no reusable
macroscopic effects: the three halogen-displacement profiles and all 169
registry-generated oxygen, fixed-charge-ion-pair, and covalent-combination
profiles. The compatibility pass now requires every supported profile to have
visible progression:

- an otherwise inert profile receives `ReactionActivity` at its validated
  `forms` ordinal, producing a small seeded interaction front and secondary
  object/vessel response;
- an already-authorized gas product asset receives `GasRelease`;
- an already-authorized dry solid product asset receives `SolidFormation`;
- an already-authorized liquid asset can receive `LiquidMixing`; and
- phase-specific effects are never selected when the older profile does not
  already establish that phase.

This pass checks object roles and typed assets, not reaction IDs, species names,
formula strings, or family names. It does not make an old profile more
scientifically specific than its reviewed data. For example, a
halogen-displacement profile whose source only establishes `forms` gains
phase-neutral progress motion, but it does not invent a liquid, gas, colour, or
precipitate. Adding reviewed phase and colour observations later automatically
replaces that conservative fallback with the corresponding liquid and colour
systems.

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

Complete combustion is the first closed chemistry-process classification. It
is established before presentation only when the validated equation contains
exactly one molecular carbon/hydrogen fuel (which may also contain oxygen) and
dioxygen, and exactly gaseous carbon dioxide and gaseous water products. The
classifier compares element-count maps, representation kinds, validated
products, and reviewed/resolved phases; it does not compare reaction names,
rule IDs, display names, or renderer assets. That typed process authorizes a
natural surface flame, hot-vapour release, and—when a liquid or aqueous
reactant is present—surface disturbance from the first validated product
assignment onward. A liquid fuel therefore burns at the liquid surface rather
than displaying a detached decorative flame. In scientific terms, water
vapour itself is invisible; the pale escaping plume is an intentionally
stylised hot-vapour/condensing-mist cue for educational visibility.

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
Carbon oxidation now demonstrates this boundary end to end. Its rule validates
the dioxide product, its `.chems` source observes that product forming, and
NIST-backed catalogue records establish carbon as solid plus oxygen and the
dioxide as gases in the standard presentation context. The generic compiler can
therefore select in-vessel gas mixing, a persistent gas product, and gas
expansion. The renderer still does not guess carbon dioxide from the names
"carbon" and "oxygen", from the formula, or from the molecular representation.
Hydrogen oxidation follows the same route: NIST-backed standard-context records
establish both molecular reactants as gases and water as a bulk liquid. The
generic compiler therefore replaces the old phase-unknown solid fallback with
in-vessel gas mixing and liquid formation. These are structure-keyed catalogue
facts reusable by other rules, not renderer branches for the reaction name.

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

- The reaction scene is strictly macroscopic. Exact catalogue atom and bond
  graphs remain available to Structural 2D and the separate product-inspection
  view, but are not accepted by the macroscopic renderer API and cannot replace
  a phase asset with a ball-and-stick model. Reviewed phase selects a mobile
  liquid volume, a faceted bulk solid, or a connected gas density field.
- Gas reactants, gas products, gas release, and hot vapour advance through a
  low-resolution Eulerian volume containing density, temperature, pressure, and
  three-dimensional velocity. Semi-Lagrangian advection, pressure projection,
  scalar diffusion, concentration-pressure coupling, temperature buoyancy,
  conservative density contrast against ambient air, wall drag, deterministic
  wind, and vorticity confinement make neighbouring density cells push, mix,
  curl, spread, merge, and dissipate as one fluid. A cylindrical boundary holds
  gas inside the vessel below the rim while the domain above the rim is open.
  Mixed reactant/headspace gas uses broad secondary density injection to occupy
  the available volume. A gaseous product authorized by `forms` uses a
  role-and-observation-selected retained regime: its seeded upper interface
  fills upward with product formation, density weight drives a low gravity
  current, horizontal pressure spreads it across the vessel floor, wall drag
  slows the boundary layer, and interface vorticity keeps the edge rolling and
  entraining rather than freezing. Heat continuously weakens stratification and
  restores buoyancy. A product authorized by `evolves` remains mixed and feeds
  a continuous plume through the open rim instead of inheriting the dense-layer
  default; vapour uses the hotter escaping regime. The renderer draws
  occupied cells as one depth-sorted field of overlapping soft GPU splats.
  Retained splats are flattened into overlapping interface sheets; escaping
  splats stretch along velocity. Beer-Lambert-style extinction, smooth
  multi-octave density noise, restrained self-shadowing, and irregular optical
  falloff produce thick fog without presenting simulation cells as molecular
  beads or rigid low-poly shells.
- Liquid uses a low-resolution displaced surface. Multiple damped waves,
  seeded phase variation, edge damping, and a raised meniscus respond to the
  continuous turbulence input. Observation-gated liquid mixing adds faint
  subsurface helical flow tracers so convection remains readable in an
  otherwise colourless solution without pretending that new coloured matter
  exists.
- A typed `LiquidMixing` effect in a scene with an existing liquid `Contents`
  volume adds one reusable virtual glass stirring rod. It enters around the
  vessel rim on a curved path, immerses before the main flow develops, follows
  a seeded elliptical stirring path with non-uniform angular travel and
  velocity-dependent lean, then withdraws along a separate curve. Local wake
  rings, subsurface currents, and the `LiquidMixing` share of liquid turbulence
  remain at zero during insertion and immersion, then ramp with the rod's
  active stroke. Independently authorized bubbling, splashing, heat, and
  surface disturbance remain unaffected. A small liquid film can trail the
  withdrawing tip. The apparatus is absent for non-mixing effects and for
  gas-only reactants that merely form a liquid product. Selection uses typed
  effects and phase assets, never reaction or species names.
- Bubbles remain discrete because bubbles are visible macroscopic interfaces.
  Their rise, wobble, size, birth phase, and fade are seeded and reproducible.
- Dry solid products use staggered nuclei that grow outward into independently
  rotated faceted shards. This is separate from precipitation: no liquid fall,
  sediment, or clouding is shown unless the profile already establishes a
  mobile phase and precipitate.
- Splashes use ballistic arcs, while precipitate forms persistent pointed,
  flat-shaded shards. The fragments accelerate under gravity, tumble with
  seeded angular momentum, lose lateral drift through the liquid, make a small
  damped vessel-floor rebound, and then settle. Final crystal, powder, and
  precipitate product clusters use the same faceted shard language rather than
  perfect spheres. Both remain selected by typed effects rather than reaction
  names.
- Solid and liquid additions are released above the vessel centre and
  accelerate downward under gravity with seeded air drift, restrained angular
  momentum, and a short plunge-and-rebound impact response instead of
  following an eased approach spline. Gas reactants are present inside the
  vessel headspace from the setup stage and follow bounded buoyancy, drag, and
  curl flow; they are never dropped, tossed as rigid objects, or assigned a
  rigid-body spin. Products remain invisible before their trusted observation,
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

## Virtual solvent separation and crystallisation

A validated neutralisation and the later isolation of its salt are deliberately
separate states. The chemistry frame sequence ends first. A typed
`SolventEvaporationCrystallization` presentation process may then append three
beats without changing products, atom mappings, observations, or the final
trusted frame:

1. the vessel rises a small deterministic distance onto a reusable support and
   a blue virtual burner ignites beneath it;
2. wall and floor nucleation sites grow, detach, accelerate upward, merge into
   occasional larger bubbles, disturb the falling surface, and feed the shared
   advected vapour field while the solvent level decreases;
3. the burner and boiling decay, residual vapour dissipates, and seeded faceted
   nuclei grow into a persistent salt bed on the vessel floor.

The process is selected from trusted structure, phase, and product information,
not a reaction or species name. Dynamic outcomes require a structurally
identified proton donor, an ionic base, liquid water, and an aqueous ionic
product. The reviewed legacy neutralisation profile supplies the same typed
process while older catalogues lack macroscopic phase records. Gas-evolution
acid/carbonate reactions and precipitation profiles do not receive this
two-product separation sequence.

The UI labels every added beat **Virtual separation** so heating is not
misrepresented as additional neutralisation chemistry or as a laboratory
procedure. The blue flame belongs to the reusable heating apparatus and is not
a claim that the reaction mixture is combustible. Salt colour comes from the
already-authorized product appearance; the renderer never chooses a salt by
name.

The boiling approximation follows the observable nucleate-boiling cycle rather
than spawning uniform bubbles throughout the liquid. Detailed boiling research
models conjugate heat transfer, microlayer evaporation, surface nucleation,
bubble growth, and departure; ChemSpec uses a bounded deterministic
approximation of those visible stages suitable for real-time playback.
Crystallisation likewise separates staggered nucleation from subsequent crystal
growth instead of revealing a finished solid all at once.

Sources:

- [Royal Society of Chemistry: Preparing a soluble salt by neutralisation](https://edu.rsc.org/experiments/preparing-a-soluble-salt-by-neutralisation/1760.article)
- [Journal of Fluid Mechanics: Comprehensive simulations of boiling with a resolved microlayer](https://www.cambridge.org/core/journals/journal-of-fluid-mechanics/article/comprehensive-simulations-of-boiling-with-a-resolved-microlayer-validation-and-sensitivity-study/C52BA3387A09F19E9945B2CB8193E887)
- [Lattice Boltzmann Simulation of Nucleate Pool Boiling in Saturated Liquid](https://global-sci.org/index.php/cicp/article/view/5840)

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
the same state exactly. Solids still use closed-form trajectories, while gas
reconstructs a fixed-step, coarse Eulerian fluid state from the absolute
playhead. The gas solver uses stable advection, incompressibility projection,
concentration-pressure coupling, scalar diffusion, vorticity confinement,
external drafts, buoyancy, density weight, wall drag, and bounded dissipation.
This is real fluid simulation at an intentionally low resolution, not
scientific CFD: the normalized visual inputs are not measured thermodynamic
properties and the vessel collision is an analytic beaker boundary rather than
arbitrary mesh collision. Solid fragments use point trajectories and a
simplified liquid/floor collision rather than arbitrary mesh-to-mesh contact.
Carbon dioxide and oxygen are colourless; their pale neutral gas rendering is
an explicit educational concentration cue, not smoke, soot, or a claimed
observable colour. Since `.chems` and the current catalogue do not yet provide
relative density, viscosity, or diffusion coefficients, persistent products
observed only as `forms` receive a conservative mild stable-layer default and
validated heat can loft them. An explicit `evolves` observation instead selects
the mixed/buoyant route. Exact heavier-than-air versus lighter-than-air behavior
requires future reviewed generic material metadata rather than a chemical-name
branch.

The stirring rod follows the same absolute-playhead rule. Entry, immersion,
active stirring, and withdrawal are renderer motion inside the already
authorized `LiquidMixing` interval; they do not add a chemistry stage, alter
trusted ordinals, or claim a laboratory procedure. The persistent virtual-only
disclosure remains the authority for the presentation.

The procedural choices follow established real-time graphics guidance:

- NVIDIA's real-time fluid treatment tracks density and temperature separately,
  applies buoyancy to hot smoke, and notes that vorticity confinement restores
  rotational detail lost on coarse grids.
- Stam's stable-fluids method supplies the semi-Lagrangian advection and
  pressure-projection basis.
- Fedkiw, Stam, and Jensen use vorticity confinement to restore rolling smoke
  motion lost through numerical dissipation.
- Curl noise supplies deterministic, divergence-free procedural wind without
  introducing reaction-specific motion.
- A low-resolution physical macrostructure plus soft optical reconstruction
  avoids both rigid blobs and an expensive high-resolution ray-marched volume.
  Layer opacity uses exponential extinction so overlapping density builds into
  a continuous fog instead of revealing a stack of circular sprites.

Sources:

- [Jos Stam: Stable Fluids](https://graphics.stanford.edu/courses/cs468-05-fall/Papers/p121-stam.pdf)
- [Fedkiw, Stam, and Jensen: Visual Simulation of Smoke](https://www.graphics.stanford.edu/papers/smoke/)
- [NVIDIA GPU Gems 3: Real-Time Simulation and Rendering of 3D Fluids](https://developer.nvidia.com/gpugems/gpugems3/part-v-physics-simulation/chapter-30-real-time-simulation-and-rendering-3d-fluids)
- [NVIDIA GPU Gems: Fast Fluid Dynamics Simulation on the GPU](https://developer.nvidia.com/gpugems/gpugems/part-vi-beyond-triangles/chapter-38-fast-fluid-dynamics-simulation-gpu)
- [Bridson, Hourihan, and Nordenstam: Curl-Noise for Procedural Fluid Flow](https://www.cs.ubc.ca/~rbridson/docs/bridson-siggraph2007-curlnoise.pdf)
- [SideFX: Pyro simulation background](https://www.sidefx.com/docs/houdini/pyro/background.html)
- [USGS: Carbon Dioxide—Dangers of a Colorless, Odorless Gas](https://pubs.usgs.gov/of/2010/1174/)
- [NIST Chemistry WebBook: carbon dioxide](https://webbook.nist.gov/cgi/cbook.cgi?ID=C124389)
- [NIST Chemistry WebBook: oxygen](https://webbook.nist.gov/cgi/cbook.cgi?ID=C7782447)
- [NIST Chemistry WebBook: carbon](https://webbook.nist.gov/cgi/cbook.cgi?ID=C7440440)

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

Complete hydrocarbon combustion demonstrates the closed process route. A
future validated `.chems` reaction with exact molecular fuel and dioxygen
reactants, gaseous carbon-dioxide and water products, reviewed phases, and a
validated product-assignment progression is classified upstream. The same
generic compiler then selects natural flame, hot vapour, gaseous headspace
motion, and optional liquid-surface disturbance without adding Rust code for
the reaction or chemical names.

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

The gas grid is bounded to 768 cells, uses five pressure iterations at an
18 Hz fixed simulation step, and exposes at most 4,096 GPU-instanced optical
splats. The renderer batches those instances in one gas pass and reuses the
existing Iced/wgpu device, depth target, camera bind group, and event loop.
Back-to-front sorting is deterministic for the fixed camera. The same absolute
playhead position reconstructs the same field, so pause, replay, and scrubbing
do not depend on wall-clock integration or mutable particle history.

The CPU implementation quantizes continuous renderer controls conservatively
and retains up to 64 deterministic fixed-step fields in a bounded cache.
Redraws between solver ticks share the same reference-counted density and
velocity arrays instead of integrating again from time zero. A changed seed,
step, or materially changed control produces a fresh field, so replay and
seeking remain deterministic. Boiling uses 28 reusable nucleation sites and
crystallisation uses 48 bounded faceted nuclei, with no per-frame allocation of
unbounded particle populations. Future work can interpolate between cached
solver frames or move grid integration to a compute shader. High-resolution
scientific CFD, resolved boiling microlayers, measured
supersaturation or solubility curves, arbitrary vessel mesh collisions,
physically measured kinetics and gas density, general rigid-body collision,
shard-to-shard contact, flame-fluid coupling, heat refraction, and
multiple-scattering volume transport are intentionally not claimed.
