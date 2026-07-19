# Embedded macroscopic models

`metal.fbx` is the user-provided source model for the experimental
`newmodeltesting` branch. Its SHA-256 digest is
`45c2f11e9c6f6490e4a5d659dd8c008f48bf5814fd6835cd898d5f863d5308df`.

`metal.mesh` is the deterministic, renderer-ready copy. It contains only the
largest evaluated mesh from the FBX, normalized to a Y-up coordinate system.
The application embeds this compact file at compile time; Blender is not a
runtime or build dependency.

Regenerate it with:

```sh
blender --background --python tools/bake-fbx-mesh.py -- \
  crates/chemspec-app/assets/models/metal.fbx \
  crates/chemspec-app/assets/models/metal.mesh
```

The source asset was supplied for this project. Confirm redistribution rights
before publishing it outside the model-testing branch.

## Alkali metal and water assembly

`potassium_water_reaction_complete.glb` is the user-supplied modular source
asset retained for provenance and future editing. Its SHA-256 digest is
`eef7438c2e38130f5b8a4055fa3e835ca55e5ceda8738010666136eb8ec8cdee`.
The editable source is
`/home/aryan/potassium_water_reaction/source/potassium_water_reaction.blend`.

`alkali_water.clip` is the runtime form. It contains 92 evaluated mesh tracks
over 180 frames at 30 FPS, split into beaker, water, metal, flame, bubble,
splash, and vapour modules. Positions use signed 16-bit quantization and
normals use signed 8-bit quantization. The application interpolates adjacent
samples on a 60 Hz presentation tick, so playback is not limited to the
authored frame cadence. Its SHA-256 digest is
`8f9601b67fb7a1b1a9778c4f9c4480ada895df40de59462c34b2ad1d994049d5`.

Regenerate the clip with:

```sh
ALSOFT_DRIVERS=null blender --background \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/source/potassium_water_reaction.blend \
  crates/chemspec-app/assets/models/alkali_water.clip
```

Blender evaluates all modifiers and transforms during the bake. It is not a
runtime or ordinary build dependency. The runtime does not parse the GLB and
does not create or destroy an entity for each visual particle.

## Neutralisation and solvent separation

`neutralisation.clip` is baked from the user-supplied
`neutralisation/source/neutralisation_evaporation.blend`, whose SHA-256 digest
is `1d3349aaccafe5410889096add6c3e578241e5fae9ed216d45e4127bf915d9f2`.
The runtime clip contains 65 tracks over 240 frames at 30 FPS and has digest
`20b0dedfadac36a0f7b7a777b6aeee5c757d1c9c19354bfbef8add5c1d1efc8d`.

The neutralisation clip intentionally excludes its beaker mesh. Instead, it
stores one quantized `vessel_anchor` position per source frame. Runtime reuses
the beaker vertices and indices already embedded by `alkali_water.clip`, then
applies the neutralisation anchor displacement. Water, the surface rig,
stirring rod, mixing tracers, boiling bubbles, flame, and salt residue remain
neutralisation-specific animated tracks.

Regenerate it with:

```sh
ALSOFT_DRIVERS=null blender --background \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/neutralisation/source/neutralisation_evaporation.blend \
  crates/chemspec-app/assets/models/neutralisation.clip \
  --exclude-module beaker \
  --anchor-object ROOT_NeutralisationVessel
```

Runtime material slots override source colours. In particular, the heating
flame uses a gentle orange/yellow palette; this does not modify the separately
reviewed lilac potassium flame.

The same clip is selected for all currently reviewed acid/hydroxide,
acid/carbonate, and acid/bicarbonate members and for dynamically classified
metal-oxide neutralisations. Carbonate cases layer the generic gas volume over
the clip rather than baking carbon dioxide into reaction-specific geometry.
Liquid and residue RGB comes from validated `.chems` observations, optional
catalogue macroscopic colour, or the conservative structure-derived hydrated
ion palette; missing colour remains colourless.

## Complete and incomplete combustion assemblies

`complete_combustion.clip` and `incomplete_combustion.clip` are baked from the
user-supplied Blender scenes under
`/home/aryan/potassium_water_reaction/combustion/source`. The source digests
are respectively
`2b6f9c54cdd955b5215b99530e604b26211d4593d84984771e040badba671f9b`
and
`b1c7464abfca1981b37418ecfaad8d79412d221da0c7d1df7b482b2986e08ea2`.
Both runtime clips contain 180 frames at 30 FPS. Their digests are
`abecdd6dd94f53638bfdefba19c187b6f58dd42fc10eea177aeb4950282a40f9`
and
`03f6545c582854332080e35f5195f288ddf39e5c58798f6c9ecf44d165d990fe`.

Both clips intentionally exclude beaker geometry and reuse the beaker embedded
by `alkali_water.clip`. Complete combustion retains animated fuel, ripples,
ignition sparks, layered blue flame, and a pale hot-product plume. Incomplete
combustion retains animated fuel, a taller orange/yellow flame, dark smoke,
airborne soot agglomerates, rim buildup, and translucent soot deposits.
Carbon monoxide remains invisible; the dark plume represents authored soot.

Regenerate the clips with:

```sh
ALSOFT_DRIVERS=null blender --background \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/combustion/source/complete_combustion.blend \
  crates/chemspec-app/assets/models/complete_combustion.clip \
  --exclude-module beaker

ALSOFT_DRIVERS=null blender --background \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/combustion/source/incomplete_combustion.blend \
  crates/chemspec-app/assets/models/incomplete_combustion.clip \
  --exclude-module beaker
```

Runtime selects the incomplete assembly only when upstream validated chemistry
classifies a C/H(/O)-fuel/dioxygen reaction whose exact gaseous products
include carbon monoxide. Presentation receives the exact fuel carbon count as
a typed value and maps C1-C4 to nearly clear, C5-C8 to pale yellow, C9-C12 to
amber, C13-C16 to warm brown, and C17+ to deep brown. It does not inspect a
hydrocarbon name or parse a display formula.

## Aqueous precipitation assembly

`precipitation.clip` is baked from the user-supplied
`/home/aryan/potassium_water_reaction/precipitation/source/precipitation_reaction.blend`.
It contains 133 modular tracks over the complete 180-frame, 30 FPS timeline.
Its SHA-256 digest is
`409e2751adfffe50fe14101746efbd60d5761651cc05f7a7f2502f9333412c66`.
The adjacent `precipitation.asset.json` records the reviewed source contract,
module inventory, material bindings, exclusions, and runtime digest.

The bake excludes both `beaker` and `stage`. Runtime reuses the exact shared
beaker topology embedded by `alkali_water.clip` and retains only the separately
authored initial liquid, pouring vessel and added liquid, mixing currents,
temporary precipitate cloud, falling fragments, and persistent sediment. The
modular part FBXs under the supplied `exports/fbx/parts` directory remain
editing/interchange assets; ChemSpec does not duplicate or parse them at
runtime.

Regenerate the runtime clip with:

```sh
ALSOFT_DRIVERS=null blender --background \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/precipitation/source/precipitation_reaction.blend \
  crates/chemspec-app/assets/models/precipitation.clip \
  --exclude-module beaker \
  --exclude-module stage
```

The presentation compiler selects the assembly only when validated data
authorizes both precipitation and clouding for the same exact solid-product
`forms` observation in a liquid or aqueous context. `forms` alone is
insufficient. `MAT_LiquidInitial`, `MAT_LiquidAdded`, `MAT_PrecipitateCloud`,
and `MAT_Precipitate` are recoloured at runtime from exact validated bindings;
the cloud keeps a lower opacity than the solid, and `MAT_Glass` maps to the
existing shared material. Playback derives the clip frame from the absolute
playhead starting at the product-formation ordinal, so seeking and reverse
scrubbing do not integrate mutable animation state.

## Generic gas-evolution assemblies

`gas_evolution_liquid_liquid.clip` and
`gas_evolution_solid_liquid.clip` are baked from the supplied complete FBXs
under `/home/aryan/potassium_water_reaction/gas_evolution/exports/fbx`. Each
preserves the complete 180-frame, 30 FPS, six-second authored timeline. The
adjacent `gas_evolution.asset.json` records both source manifests, editable
Blender scenes, complete FBX bake inputs, runtime digests, module inventories,
and exact material bindings. The FBX exporter omits Blender custom properties,
so the offline baker restores module labels from the stable exported object
prefixes; this does not add runtime reaction classification.

Both bakes exclude the duplicate main `beaker` and the presentation `stage`;
runtime reuses the beaker topology already embedded by `alkali_water.clip`.
The liquid-liquid variant retains its pouring vessel and added solution,
mixing currents, bubbles, surface bursts, and connected gas plume. The
solid-liquid variant retains the falling/consuming solid, bubbles, bursts, and
connected plume. The supplied modular FBXs remain interchange assets and are
not duplicated or parsed at runtime.

Regenerate the runtime clips with:

```sh
ALSOFT_DRIVERS=null blender --background \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/gas_evolution/exports/fbx/liquid_liquid_gas_evolution_complete.fbx \
  crates/chemspec-app/assets/models/gas_evolution_liquid_liquid.clip \
  --exclude-module beaker

ALSOFT_DRIVERS=null blender --background \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/gas_evolution/exports/fbx/solid_liquid_gas_evolution_complete.fbx \
  crates/chemspec-app/assets/models/gas_evolution_solid_liquid.clip \
  --exclude-module beaker
```

Presentation selects a variant only when trusted macroscopic data identifies
one gaseous product with an exact active `evolves` or `forms` observation and
exactly two reactants whose reviewed phases are either mobile/mobile or
solid/mobile. Combustion stays on its combustion assembly. Missing, unknown,
extra, or unsupported reactant phases retain the existing generic animation.
No reaction, formula, or species string participates in selection.

The two clips are embedded but parsed into separate lazy caches, so only the
selected layout is decoded. Per-scene vertex colours bind
`MAT_LiquidInitial`, `MAT_LiquidAdded` or `MAT_SolidReactant`,
`MAT_GasBubble`, and `MAT_GasCloud` to exact validated identities. Exact
`.chems` colour observations outrank reviewed catalogue RGB; absent gas colour
uses the supplied pale nearly colourless material. Bubble/plume opacity
remains renderer-owned, the plume is gas rather than smoke, and the generic
category never introduces a flame.

## Generic metal-displacement assembly

`metal_displacement.clip` is baked from the supplied
`/home/aryan/potassium_water_reaction/metal_displacement/source/metal_displacement_reaction.blend`.
It contains 42 modular tracks over 180 frames at 30 FPS. The adjacent
`metal_displacement.asset.json` records the source and runtime digests,
timeline, module inventory, material bindings, exclusions, and shared beaker.

The bake retains separate initial/final solution, original-metal erosion,
replacement-metal growth, and detached-flake modules. It excludes the duplicate
beaker and presentation stage, so runtime reuses the exact beaker topology in
`alkali_water.clip`.

Regenerate the runtime clip with:

```sh
ALSOFT_DRIVERS=null blender --background \
  /home/aryan/potassium_water_reaction/metal_displacement/source/metal_displacement_reaction.blend \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/metal_displacement/source/metal_displacement_reaction.blend \
  crates/chemspec-app/assets/models/metal_displacement.clip \
  --exclude-module beaker
```

Selection is chemistry-owned. It requires one solid metallic reactant, one
aqueous soluble ionic reactant, one aqueous ionic product containing the
original metal, and a different solid metallic product whose element was in
the initial solution. Any gas product routes to gas evolution; combustion,
precipitation, missing structures, and ambiguous phases retain their existing
selection. No reaction name, display formula, or species name is inspected.

Per-scene colour instances bind `MAT_SolutionInitial`, `MAT_SolutionFinal`,
`MAT_OriginalMetal`, and `MAT_DepositedMetal` to the exact validated roles.
Exact `.chems` colour observations outrank reviewed catalogue RGB; missing
solution colours remain colourless and missing metal colours use conservative
neutral metallic values, except for conservative exact-structure copper/gold
elemental fallbacks. Opacity remains phase-owned, the original/deposited metal
tracks stay opaque, `MAT_MetalErosion` remains dark neutral, and the shared
`MAT_Glass` material is preserved. Deposits and flakes use a restrained
deterministic silhouette expansion and low-opacity highlight shell so their
authored growth remains visible through overlapping liquid and glass. Runtime
also suppresses their tiny source-scene setup geometry until their documented
start frames: replacement-metal growth begins at source frame 54 and detached
flakes begin at source frame 104.
Playback is sampled from the absolute six-second reaction playhead, so seek,
pause, replay, and backwards scrub are deterministic.

## Generic solid-solid synthesis assembly

`synthesis_combination.clip` is baked from
`/home/aryan/potassium_water_reaction/synthesis_combination/source/synthesis_combination_reaction.blend`.
It preserves 29 modular tracks across all 180 frames at 30 FPS. The adjacent
`synthesis_combination.asset.json` records source/runtime digests, material
bindings, modules, and the presentation-stage exclusion. The ceramic dish is
part of this authored layout; the unrelated stage mesh is omitted. The current
polished source uses closed beveled dish topology, deformed rounded granules,
staggered mixing and product nucleation, and broken reaction-front bands so it
remains readable under ChemSpec's sampled-normal lighting without Blender PBR.

Regenerate it with:

```sh
ALSOFT_DRIVERS=null blender --background \
  /home/aryan/potassium_water_reaction/synthesis_combination/source/synthesis_combination_reaction.blend \
  --python tools/bake-blender-clip.py -- \
  /home/aryan/potassium_water_reaction/synthesis_combination/source/synthesis_combination_reaction.blend \
  crates/chemspec-app/assets/models/synthesis_combination.clip \
  --exclude-module stage
```

Selection requires exactly two validated solid reactants and exactly one
validated solid product, with no gaseous product. Combustion, surface
oxidation, gas evolution, precipitation, metal displacement, and
neutralisation are classified first. Three or more reactants and unknown
phases retain the generic fallback because ChemSpec has no reviewed rule for
silently aggregating or omitting solid inputs.

Runtime binds `MAT_ReactantA`, `MAT_ReactantB`, and `MAT_Product` to their exact
validated identities. Exact `.chems` colour observations outrank reviewed
catalogue RGB, followed by conservative solid fallbacks. Ceramic
`MAT_ReactionVessel`, metal `MAT_MixingTool`, and warm emissive
`MAT_ReactionFront` are presentation materials and never receive chemical
catalogue colours. The reaction-front module can be suppressed independently.
All material colours are computed per scene and absolute-playhead sampling
makes pause, seek, replay, and backwards scrub deterministic.

## Heavy-alkali water-contact assembly

`rubidium_water_explosion.clip`, `caesium_water_explosion.clip`, and
`francium_water_explosion.clip` are the three complete supplied Blender-scene
bakes for the reusable high-energy metal/water category. Each has 185 evaluated
mesh tracks across source frames 1–180 at 30 FPS (six seconds), is embedded at
compile time, and is parsed lazily only for its selected typed variant. Exact
source and runtime digests, byte counts, source-scene inventory, material
aliases, exclusions, visibility boundaries, and provenance are recorded in
[`heavy_alkali_water_explosion.asset.json`](heavy_alkali_water_explosion.asset.json).

The editable Blender sources are deliberately not committed. They came from
the user-supplied `assets/alkali_explosion.zip` archive; the archive did not
include licence or redistribution terms. The scenes use the authored
metal-specific `Rb`, `Cs`, and `Fr` material aliases, which the offline baker
maps to existing stable ChemSpec slots without adding reaction-specific runtime
IDs. Authored `FX_Spark_*` meshes carry the source `flame` module tag and are
therefore included in the existing Flame module.

The bake excludes the duplicate beaker and presentation stage. Runtime reuses
`alkali_water.clip#module=beaker`; the heavy clips retain water, metal,
explosion, Flame, bubbles, splashes, vapour, and beaker-shard geometry. The
source's setup geometry is held invisible before its authored contact ranges:
frame 40 for explosion/shards/flame and frame 45 for bubbles/splashes/vapour.
This is a deterministic absolute-playhead visibility rule, not mutable
animation state.

Regenerate one selected variant after extracting the supplied archive outside
the repository's runtime assets:

```sh
nix shell nixpkgs#blender --command blender --background \
  --python tools/bake-blender-clip.py -- \
  /path/to/alkali_explosion/source/rubidium_water_explosion.blend \
  crates/chemspec-app/assets/models/rubidium_water_explosion.clip \
  --exclude-module beaker
```

Selection is chemistry-owned: it requires the exact reviewed water-contact
capability on one validated solid metallic reactant, liquid molecular water,
an aqueous ionic product, and a molecular gaseous product. The clips receive
only exact role bindings. Validated `.chems` colour observations outrank
reviewed catalogue RGB, then conservative phase fallbacks; water transitions
toward the bound hydroxide colour at the validated formation/colour ordinal.
Chemical RGB is separate from phase-owned opacity. Glass remains translucent,
flame slots retain their shared emissive semantics, metal remains opaque, and
hydrogen bubble/vapour slots retain low gas opacity.
