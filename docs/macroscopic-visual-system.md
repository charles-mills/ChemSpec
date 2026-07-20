# Macroscopic 3D visual system

## Scope and validation boundary

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
The schema shape is illustrated below. This example remains a proposal until
its premise and catalogue mutation receive independent review:

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
this original pass, 172 of the then-205 supported experiences had no reusable
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

For non-combustion gas generation, an optional authored gas-evolution layer
refines the existing generic gas effects without changing `.chems 1`.
Selection requires one exact active `evolves` or gas-phase `forms` observation,
one exact gaseous product binding, and exactly two catalogue-resolved
reactants. Two liquid/aqueous reactants select the reusable liquid-liquid clip;
one solid plus one liquid/aqueous reactant selects the solid-liquid clip.
Unknown phases, additional reactants, multiple gas products, and unsupported
layouts keep the generic fallback. The chemistry-owned combustion process is
checked first and always retains its combustion clip.

Dynamically solved reactions use the same clips through the chemistry-owned
`GasEvolutionLiquidLiquid` and `GasEvolutionSolidLiquid` processes. Selection
requires exactly one validated gaseous product plus the solver's typed
`evolves` claim. Reactant layout is derived structurally: soluble ionic salts
and the acid solution in an established gas-evolution family are mobile,
metallic or insoluble ionic reactants are solid, and unknown layouts remain
unsupported. Process-authorized gas release begins at the first validated
product-assignment ordinal because generated mechanisms do not invent
repository evidence observations.

Reviewed legacy profiles that predate catalogue phase records expose two
distinct generic reactant objects and the exact gas-product binding. The same
object/role selector can therefore choose a clip without inspecting the
reaction name. Acid plus insoluble sulfide is also solved structurally as a
general gas-evolution family; the ionic sulfide graph, acid donor sites,
solubility rules, exact products, and balancing gate the result.

Both gas-evolution variants are six-second, 180-frame authored clips sampled
from the absolute playhead. Their liquid, solid, bubble, and connected-plume
materials are bound to exact reactant/product identities. An exact validated
colour observation outranks role-context catalogue RGB; absent gas colour uses
a pale nearly colourless fallback. Opacity remains phase-specific, the plume
represents released gas rather than smoke, and this category never adds a
flame. The clips reuse the shared beaker and decode into separate lazy caches,
so switching reactions cannot leak geometry, material colour, or accumulated
animation state between scenes.

### Reviewed high-energy heavy-alkali water contact

`ExplosiveMetalWater` is a reusable presentation process, not a reaction-name
route. The reviewed catalogue attaches the bounded `water_contact` capability
to an exact validated metallic structure, with a typed `Rubidium`, `Caesium`,
or project-authorised `Francium` authored-clip variant. The classifier accepts
it only for exactly two reactants—one capability-bearing solid metallic
reactant and one liquid molecular water reactant—and exactly two products—one
aqueous ionic product and one molecular gaseous product. Missing, extra,
unknown, mismatched, or ambiguous material layouts do not select it. The
renderer cannot recover this category from a name, formula, display label,
fixture ID, rule ID, or asset filename.

Static reviewed expansion projects the catalogue material facts into the same
`MacroscopicProcess::ExplosiveMetalWater` value used by a newly researched
claim compiled through the catalogue-aware outcome compiler. Dynamic cache
schema 4 and compiler contract 5 deliberately invalidate older entries, then
recompile their claims at the catalogue boundary before they can receive a
presentation. Structure adoption may preserve an already catalogue-authorised
process only when the exact side, identity, and order invariants hold; it never
recreates the capability from strings.

Existing process precedence remains explicit. The reviewed/local classifier
checks metal displacement, surface oxidation, this high-energy layout, solvent
separation, combustion, then solid-solid synthesis. The dynamic compiler keeps
its existing combustion and surface-oxidation precedence, then checks this
layout before generic gas evolution, precipitation, metal displacement,
solvent separation, and solid-solid synthesis. The candidate layouts are
structurally disjoint from the earlier winners; any future overlap must be
resolved in the chemistry classifier rather than in presentation.

The three source clips preserve their authored 1–180, 30 FPS, six-second
window and sample from the absolute playhead. They reuse the shared beaker,
exclude stage geometry, and lazily parse only the selected Rb/Cs/Fr clip.
Shards, explosion, flame, bubbles, splashes, and vapour remain hidden until
their authored contact boundaries. Exact validated colour observations outrank
reviewed material RGB; otherwise water, metal, hydroxide, and hydrogen use
their bound conservative phase colours. Product-water colour moves from the
reactant base toward the product target at the validated product/colour ordinal,
while opacity remains phase-owned. The adjacent authored-asset metadata records
source hashes, runtime hashes, source material aliases, and licensing status.

Combustion is a closed chemistry-process classification. It is established
before presentation only when the validated equation contains one molecular
carbon/hydrogen fuel (which may also contain oxygen) and dioxygen. Gaseous
carbon dioxide plus gaseous water selects `CompleteCombustion`; the presence
of exact gaseous carbon monoxide instead selects `IncompleteCombustion`. The
classifier compares element-count maps, representation kinds, validated
products, and reviewed/resolved phases; it does not compare reaction names,
rule IDs, display names, or renderer assets.

Both typed processes authorize surface flame, hot-vapour release, and—when a
liquid or aqueous reactant is present—surface disturbance from the first
validated product assignment onward. In scientific terms, water vapour and
carbon monoxide are invisible; pale product plume and dark incomplete-burning
smoke are stylised hot-vapour and soot cues, not depictions of visible carbon
monoxide.

### Authored combustion assemblies

`CompleteCombustionAssembly` and `IncompleteCombustionAssembly` use separate
six-second, 30 FPS authored clips with a shared beaker and fuel rig. Complete
combustion contains fuel ripples, ignition sparks, layered blue flame, and a
sparse pale product plume. Incomplete combustion contains stronger fuel
motion, a taller unstable orange/yellow flame, dark smoke, irregular airborne
soot, rim buildup, and glass deposits. Runtime uses the authored geometry
instead of overlaying the procedural flame and product meshes.

The validated fuel formula is not reparsed by the renderer. Chemistry passes
the exact carbon count through `MacroscopicReaction::fuel_carbon_count`, and
presentation assigns a reusable fuel material palette:

- C1-C4: nearly clear;
- C5-C8: pale yellow;
- C9-C12: amber;
- C13-C16: warm brown; and
- C17+: deep brown.

This palette is a conservative visual convention rather than a claim about a
specific compound's purity or room-temperature phase. New validated
hydrocarbon fuels automatically enter the appropriate range without new Rust
reaction branches.

### Authored alkali-metal/water assembly

Reviewed alkali-metal/water profiles select the generic
`ReactiveMetalWaterAssembly` asset. This is still downstream of exact
chemistry: trusted `evolves` and `disappears` observations authorize the gas,
bubble, disturbance, and consumption controls, while reviewed qualitative
metadata sets one reusable activity level. The renderer does not inspect a
reaction name or metal name.

The supplied Blender scene is baked into 92 modular mesh tracks: beaker, water,
metal, flame, bubbles, splashes, and vapour. The application plays those
evaluated models directly and does not overlay the procedural beaker, gas
volume, or effect meshes. Modules are retained deterministically according to
the typed activity level, horizontal skitter and water deformation are scaled
continuously, and a flame module is shown only when a separate reviewed
ignition effect exists. This currently produces:

- subtle fizzing and restrained travel for lithium;
- vigorous fizzing and greater surface travel for sodium; and
- the strongest fizzing, splashing, vapour, and reviewed lilac ignition for
  potassium.

These distinctions follow the Royal Society of Chemistry's reviewed
group-one observations rather than flame-test colours or renderer-side species
branches. Lithium and sodium do not invent self-ignition merely because their
flame-test colours are known.

The source animation is 180 frames at 30 FPS. Runtime playback requests a
60 Hz presentation tick and linearly interpolates quantized positions and
normals between adjacent authored samples, removing source-frame stepping
without running Blender, an FBX/GLB importer, armature evaluation, or a second
physics solver in the application. Clip position is derived from normalized
wall-clock timeline time rather than reaction ordinal, so unequal chemistry
beat durations cannot accelerate one section and stretch another. A monotonic
contact-aware cubic time remap reaches the authored water-contact frame after
about half a second, then spreads the reaction and settling frames across the
remaining playback. Its two spans share a derivative at contact, preserve
deterministic seeking and the exact final frame, and leave the six-second
duration unchanged. Track
topology and buffer bounds are validated once, and the resulting scene remains
below the fixed GPU vertex and index budgets. The fixed orthographic camera
uses a deterministic authored assembly framing and remains noninteractive.

### Authored neutralisation and solvent separation

Validated acid/base neutralisation profiles select the reusable
`NeutralisationEvaporationAssembly` while retaining the existing
`SolventEvaporationCrystallization` process and trusted reaction observations.
The renderer does not inspect an acid, base, or reaction name.

The supplied 240-frame scene provides stirring, subsurface mixing tracers,
surface ripples, vessel lift, gentle heating, nucleate boiling, liquid
evaporation, salt-residue growth, and vessel lowering. Runtime plays these
authored tracks instead of layering the older procedural stirring, burner,
bubble, and crystal systems on top. The source has no vapour module, so the
existing reusable advected vapour volume remains as a complementary
process-driven effect during the boiling stage.

The beaker is genuinely shared with the alkali-metal/water assembly. The
neutralisation clip excludes all beaker geometry and stores only a lightweight
vessel-motion anchor. Runtime samples the existing shared beaker mesh once and
applies the anchor displacement, while neutralisation-specific water and
contents retain their evaluated deformation. This removes duplicated beaker
samples across 240 frames and keeps both scenes visually consistent.

Flame geometry remains reusable, but neutralisation heating maps its material
slots to a restrained orange/yellow flame. Potassium/water continues to use
its separately reviewed lilac ignition palette. Missing indicator or salt
colour evidence remains conservative: colourless mixing tracers and an
off-white residue are presentation defaults, not new chemical claims.

### Authored aqueous precipitation

The reusable `AqueousPrecipitationAssembly` is selected in presentation, never
by the renderer. Reviewed `.chems` profiles select it from a mobile
liquid/aqueous context, a phase-reviewed solid product, a product object bound
to its exact validated `forms` ordinal, and both observation-authorized
`PrecipitateFormation` and `Clouding` effects beginning at that ordinal. A
generic product `forms` observation without those phase/effect conclusions
cannot select the clip.

Dynamically solved double-displacement reactions use the same assembly through
the chemistry-owned `AqueousPrecipitation` process. That process requires two
structurally validated soluble ionic reactants, exactly one validated ionic
solid product, an aqueous ionic coproduct, and the solver's formation
observation. The presentation begins at the first validated product-assignment
ordinal and emits process-authorized precipitation/clouding effects. This
bridges generated mechanisms whose trusted frames do not contain repository
evidence observations without inventing a frame observation or allowing the
renderer to inspect names and formulas.

The 180-frame, 30 FPS authored clip retains separately baked initial liquid,
added pouring vessel/liquid, mixing, temporary cloud, falling fragments, and
persistent sediment modules. Its beaker and stage are excluded; runtime reuses
the shared beaker already embedded for authored vessel scenes. Frame sampling
uses absolute presentation milliseconds from the formation ordinal over an
exact six-second interval. No frame counter is accumulated, so seeking,
pausing, replaying, and backwards scrubbing reconstruct the same mesh sample.

Material colour resolution stays upstream of rendering. For each exact
binding, an active validated `.chems` colour observation outranks the reviewed
role-specific `macroscopic_materials` RGB, which outranks colourless-liquid or
off-white-solid defaults. For dynamically generated common inorganic
precipitates, a small reviewed structure-keyed colour table may supply a
conservative family colour after the exact cation, charge, anion graph, solid
phase, and `AqueousPrecipitation` process all validate. It never authorizes the
reaction or phase, and unknown families retain the off-white fallback. RGB is
stored independently from opacity:
`MAT_PrecipitateCloud` uses the product RGB with low opacity while falling and
settled `MAT_Precipitate` geometry stays opaque. The cloud and fragments
collapse at the authored end state; the sediment geometry remains present.

### Authored metal displacement

`MetalDisplacement` is classified upstream from exact validated structures and
phases. It requires one solid metallic reactant and one aqueous soluble ionic
reactant, followed by an aqueous ionic product containing the original metal
and a different solid elemental-metal product. The deposited metal's element
must occur in the initial ionic reactant. This cross-side element inventory
check distinguishes surface deposition from a broad redox label, an ionic
precipitate throughout solution, or an arbitrary `forms` observation.

Classification fails closed when structures, phases, or products are
ambiguous. Combustion retains its assembly; any gaseous product takes the
gas-evolution route (including acid plus metal producing hydrogen);
`AqueousPrecipitation` remains responsible for insoluble ionic solids.
Presentation and rendering do not inspect reaction names, formula text, or
species display names.

The selected `MetalDisplacementAssembly` samples a six-second, 180-frame,
30 FPS authored clip containing the initial and final solution surfaces,
original-metal erosion, surface deposit growth, and detached deposited-metal
flakes. The clip excludes its duplicate beaker and stage, and runtime appends
the existing shared beaker geometry. Sampling comes from absolute timeline
milliseconds, not mutable animation state, so pause, replay, seek, and
backwards scrub reproduce identical geometry.

The presentation profile carries four exact role bindings. Active validated
`.chems` colour observations have first authority, reviewed catalogue
`macroscopic_materials` RGB is second, and conservative colourless-solution or
neutral-metal values are the fallback. A small structure-derived elemental
fallback distinguishes the two naturally coloured elemental metals, copper
and gold, when no reviewed RGB record is present; it examines an exact
single-element metallic structure rather than a reaction, display name, or
formula string. All other elemental metals retain the conservative neutral
metal fallback.

The renderer preserves liquid opacity separately from RGB, keeps both metal
slots in the opaque lit pass, leaves erosion detail dark neutral, and does not
mutate cached colours shared by another reaction. To keep small authored
surface crystals legible through the liquid and glass, deposit and detached
flake tracks receive a modest deterministic silhouette expansion plus a thin
low-opacity highlight shell. This presentation-only readability layer retains
the bound product RGB as its base, does not create additional chemistry, and
samples directly from the same absolute clip frame. Tiny setup geometry is
culled until the authored module begins: deposit growth at source frame 54 and
detached flakes at source frame 104. This prevents pre-growth specks without
changing the clip timing or integrating mutable visibility state.

### Exposed metal oxidation

`SurfaceOxidation` is a second typed chemistry-process classification. It is
selected before presentation when the validated transformation has:

- exactly two reactants, one structurally metallic and one molecular
  dioxygen;
- exactly one validated ionic product containing oxygen; and
- a `product ... forms` observation in the kernel-validated frames.

The classifier compares structure representations, exact element-count maps,
roles, and validated products. It does not compare a reaction name, rule ID,
display name, renderer asset, or macroscopic phase catalogue entry. Static and
dynamic outcomes with this typed shape therefore share one presentation path;
unknown macroscopic phases do not suppress an otherwise validated oxidation
transformation.

The surface path deliberately contains no vessel and does not render dioxygen
as a thrown object. The metal is already resting on the laboratory bench. A
process-authorized oxide front spreads irregularly across the same solid mesh,
so the object does not disappear and get replaced by a robotic product model.
Minor seeded displacement communicates reaction activity without moving the
fixed camera.

The experimental `newmodeltesting` branch embeds the user-supplied
`metal.fbx`. Blender evaluates and normalizes it once into `metal.mesh`; the
runtime reads the compact 2,321-vertex mesh directly and has no Blender or FBX
dependency. `tools/bake-fbx-mesh.py` documents the reproducible conversion.

Current oxygen-family `.chems` sources establish product formation and reactant
consumption but do not always provide product appearance colour. In Codex mode,
once the catalogue and kernel have established the exact oxide identity and
`SurfaceOxidation` process, a separate bounded appearance lookup may research
that exact product with live search. The provider must echo the product
binding, structure ID, formula, and current catalogue digest, cite one to three
unique HTTPS sources, and choose one restrained colour family from a closed
palette. Local validation rejects stale identities, arbitrary RGB values,
malformed sources, procedural content, and schema drift.

An accepted lookup remains `ModelAsserted`; it is not written into `.chems` or
the catalogue and cannot authorize a product, phase, reaction, or effect.
Reviewed catalogue colour always wins. A valid provisional colour supplies the
oxide front's target while preserving the same product-bound,
process-authorized coating animation. Rejected, unavailable, cancelled, or
missing claims leave the original white-silver metal appearance unchanged
instead of presenting a generic grey as chemical fact. In Codex mode the
appearance task starts as soon as the static reaction is validated, in parallel
with presentation enrichment; a matching revalidated cache entry can therefore
be applied before the 3D scene opens.

Appearance cache entries bind the complete request, catalogue digest, schema,
and local contract version. They store the untrusted model claim and its
provider/model provenance, are revalidated on every load, and never confer
reviewed authority. No `.chems` or catalogue schema change is required.

Missing ambient phase records remain `Phase::Unknown`; the surface scene is
authorized from the validated metallic, molecular-dioxygen, and ionic-product
structure rather than writing a phase fact into the catalogue.

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
   a gentle orange/yellow virtual burner ignites beneath it;
2. wall and floor nucleation sites grow, detach, accelerate upward, merge into
   occasional larger bubbles, disturb the falling surface, and feed the shared
   advected vapour field while the solvent level decreases;
3. the burner and boiling decay, residual vapour dissipates, and seeded faceted
   nuclei grow into a persistent salt bed on the vessel floor.

The process is selected from trusted structure, phase, and product information,
not a reaction or species name. Dynamic outcomes require a structurally
identified proton donor, an ionic base, liquid water, and an aqueous ionic
product. This covers hydroxides and metal oxides as well as carbonate and
bicarbonate bases; gas-evolving members retain their typed carbon-dioxide
effect during the authored mixing phase. The reviewed legacy acid/hydroxide,
acid/carbonate, and acid/bicarbonate families supply the same typed process
while older catalogues lack macroscopic phase records. Precipitation,
combustion, phase-unknown products, and neutralisations that do not leave an
aqueous ionic product remain excluded from solvent separation.

The UI labels every added beat **Virtual separation** so heating is not
misrepresented as additional neutralisation chemistry or as a laboratory
procedure. The orange/yellow flame belongs to the reusable heating apparatus and is not
a claim that the reaction mixture is combustible. Salt colour comes from the
already-authorized product appearance; the renderer never chooses a salt by
name.

Neutralisation colour follows a strict authority order:

1. an exact validated `.chems` colour observation;
2. optional evidence-backed `colour: [red, green, blue]` on a catalogue
   macroscopic-material record;
3. a small structure-derived hydrated-ion palette for unambiguous common
   aqueous ions (currently Cu(II), Fe(II), Fe(III), Co(II), and Ni(II));
4. the conservative colourless fallback.

The third tier is keyed by the validated ionic cation and charge, never a
compound name or reaction name. It deliberately declines ligand-sensitive or
ambiguous cases. The authored mixing interval diffuses from the initial liquid
appearance toward the product colour, and the isolated salt inherits the same
trusted RGB with solid opacity. OpenStax notes that coordination and ligand
environment can change transition-metal colours, which is why an exact
`.chems` observation or catalogue RGB always wins over the simplified hydrated
ion palette.

The boiling approximation follows the observable nucleate-boiling cycle rather
than spawning uniform bubbles throughout the liquid. Detailed boiling research
models conjugate heat transfer, microlayer evaporation, surface nucleation,
bubble growth, and departure; ChemSpec uses a bounded deterministic
approximation of those visible stages suitable for real-time playback.
Crystallisation likewise separates staggered nucleation from subsequent crystal
growth instead of revealing a finished solid all at once.

Sources:

- [Royal Society of Chemistry: Preparing a soluble salt by neutralisation](https://edu.rsc.org/experiments/preparing-a-soluble-salt-by-neutralisation/1760.article)
- [OpenStax: Spectroscopic and magnetic properties of coordination compounds](https://openstax.org/books/chemistry-2e/pages/19-3-spectroscopic-and-magnetic-properties-of-coordination-compounds)
- [Royal Society of Chemistry: Testing transition-metal cations](https://edu.rsc.org/download?ac=17360)
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

Solid-solid combination uses the same closed process route. Once chemistry has
validated exactly two solid reactants and one solid product, and excluded the
more-specific categories, presentation selects the shared six-second granular
mixing clip. Reactant and product colours remain exact-identity bindings from
`.chems` observations or reviewed catalogue records. Adding another
two-solid-to-one-solid reaction therefore requires validated chemistry data, not
a renderer branch. Reactions with extra or unknown-phase reactants keep the
fallback instead of hiding chemically important material.

Phase-aware synthesis extends that route with two sealed-chamber layouts.
Exactly one typed solid plus one typed gas producing one typed gas selects the
solid-gas clip; exactly two typed gases producing one typed gas selects the
gas-gas clip. Solid-gas binding is independent of equation order, while the two
gas reactants retain their deterministic validated order. Combustion, surface
oxidation, aqueous/solid-liquid gas evolution, precipitation, metal
displacement, and neutralisation remain higher-priority classifications.

Both phase-synthesis clips contain 180 frames at 30 FPS and are sampled from
the absolute six-second playhead. Their translucent irregular volumes are
sealed-chamber concentration cues, not smoke, bubbles, molecule models, or
released plumes. Exact reaction-scoped catalogue RGB values bind the chemical
slots; absent gas colour uses a pale highly transparent fallback, while the
glass, neutral frame, and optional warm reaction front remain presentation
materials. Changing reaction plans replaces the complete binding profile, so
no prior clip colour or visibility state survives a switch.

The solid-gas clip retains all eight opaque granular solid tracks alongside
the translucent concentration tracks. The solid slot uses the exact solid
reactant binding. Reviewed visible colours currently distinguish iodine,
sulfur, and chlorine, while hydrogen, nitrogen, hydrogen halides, ammonia, and
hydrogen sulfide deliberately retain the nearly colourless gas fallback rather
than receiving invented educational colours.

The `.chems 1` format is unchanged. Molecular representation does not establish
gas phase: a current or future reaction activates these clips only when its
validated outcome, promoted catalogue material records, or a newly researched
reaction claim carries the required reaction-scoped typed phases. Dynamic
claims provide one reactant phase per exact request identity; the compiler
checks request order and count, prefers reviewed catalogue phase authority,
and stores the checked result in the same macroscopic phase map consumed by
static reactions. Promoted standard-state records now route the exact
hydrogen/chlorine, hydrogen/iodine, hydrogen/sulfur, and nitrogen/hydrogen
fixtures through their gas-gas or solid-gas assemblies. Dynamic outcomes use
the same structure-keyed lookup before chemistry classifies their macroscopic
process; an absent or ambiguous reactant phase still retains the fallback.
Cached claims are recompiled through this catalogue-aware boundary, and cache
contract changes invalidate older entries rather than replaying a stale
fallback classification.

The hydrogen/bromine reviewed fixture uses reaction-role material authority:
bromine is presented as red-brown reacting vapour and hydrogen bromide as a
colourless gaseous product, so the local path selects the gas-gas assembly
without mislabelling ambient liquid bromine as its standard state. For
researched reactions, the compact claim requests a colour observation whenever
the exact phase has a characteristic visible bulk colour. Values are restricted
to the renderer's closed named palette or `Rgb.HexRRGGBB`; the product slot
transitions at the validated observation ordinal, while absent colour facts
retain the conservative colourless-gas fallback.

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
