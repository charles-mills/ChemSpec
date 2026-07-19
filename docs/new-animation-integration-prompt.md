# Reusable Codex prompt for importing a new 3D reaction animation

Copy the prompt below into a new Codex session. Replace every value in angle
brackets before sending it. The prompt deliberately assumes the session has no
prior knowledge of ChemSpec.

```text
You are working in an existing Rust application named ChemSpec. The repository
is at:

<ABSOLUTE_PATH_TO_CHEMSPEC_REPOSITORY>

Integrate a new reusable 3D reaction-animation category using assets at:

<ABSOLUTE_PATH_TO_NEW_ASSET_DIRECTORY>

The intended generic category is:

<GENERIC_CATEGORY_NAME>

The scientifically meaningful selection requirements are:

<TYPED_CLASSIFICATION_REQUIREMENTS>

The category must remain lower priority than:

<HIGHER_PRIORITY_CATEGORIES>

The supported phase/layout variants are:

<SUPPORTED_VARIANTS_OR_NONE>

The expected runtime material slots and their chemical roles are:

<MATERIAL_SLOT_TO_REACTION_ROLE_MAPPING>

The source timeline is:

<FRAME_RANGE, FPS, DURATION, LOOP_OR_PLAY_ONCE>

Do not assume anything about ChemSpec from this prompt. Begin by inspecting the
repository and discovering the current architecture, contracts, versions, and
working-tree state. Read the repository-level AGENTS.md completely, then read
the relevant durable documentation under docs/. At minimum inspect:

- docs/system-architecture.md
- docs/chemistry-engine.md
- docs/chems-specification.md
- docs/automatic-animation-system.md
- docs/macroscopic-visual-system.md
- docs/verification.md
- crates/chemspec-app/assets/models/README.md
- any active plan under docs/plans/ that governs this work

Read the new asset directory's README files, manifests, source-scene notes, and
licensing/provenance information before changing code. Inspect the existing
integrations most similar to this category. Depending on the layout, these may
include alkali-metal/water, neutralisation, combustion, precipitation, gas
evolution, metal displacement, solid-solid synthesis, solid-gas synthesis, and
gas-gas synthesis.

Do not redesign or extend `.chems` merely to make renderer selection easier.
The `.chems` language, catalogue, chemistry engine, and validated frame sequence
are upstream authorities. Preserve this boundary:

    source / researched claim
        -> parsing and identity resolution
        -> exact balancing and chemistry validation
        -> validated reaction outcome and SimulationFrames
        -> typed macroscopic process/material inputs
        -> presentation profile
        -> reusable animated assembly renderer

Unvalidated or stale chemistry must never reach the animation. The renderer
must not decide chemistry from a reaction name, equation string, formula
substring, display name, fixture ID, asset filename, or species-name branch.
Classify from validated identities, typed phases, structural representations,
product multiplicity, trusted observations, reviewed catalogue facts, and any
other typed chemistry properties already available.

Before implementing, trace both production paths:

1. Reviewed/local `.chems` reactions expanded from the trusted catalogue.
2. Newly researched dynamic reactions compiled from a provider claim and,
   when needed, structure/mechanism enrichment.

The new category must work through both paths. A local fixture and an
equivalent future researched reaction with the same validated physical pattern
must reach the same generic macroscopic process. Ensure cached dynamic claims
are recompiled through the catalogue-aware boundary. If a provider claim lacks
a generic factual property needed for all future reactions, extend that
provider contract in a bounded, validated, backwards-compatible way and
invalidate stale cache entries deliberately. Do not add a field for one named
reaction.

Implement selection in the chemistry-owned classifier before presentation.
Keep the existing category priority explicit. Unsupported, missing, ambiguous,
or extra reactant/product layouts must retain the current fallback rather than
silently omitting chemically important material. A broad label such as
"synthesis", "redox", or "product forms" is not sufficient authority for a
specific physical animation.

Use the existing renderer-independent macroscopic process and material
pipeline. If new enum variants are required, add them vertically through every
exhaustive match:

- chemistry outcome classification;
- static/local application projection;
- dynamic outcome projection;
- chem-presentation authorization and validation;
- ScenePlan;
- scene registry;
- 3D assembly selection;
- focused tests and documentation.

Do not create a reaction-specific renderer module. One reusable animation
category may have multiple phase/layout variants, selected only from typed
validated inputs.

Asset integration requirements:

- Use the supplied complete FBX or Blender source as the offline bake input
  unless the existing architecture clearly requires modular assembly.
- ChemSpec does not load FBX or Blender at runtime. Use the existing
  tools/bake-blender-clip.py workflow to produce embedded `.clip` files.
- Preserve the authored frame range, FPS, duration, and one-shot/loop contract.
- Runtime sampling must use an absolute deterministic playhead. Seeking,
  pausing, replaying, changing speed, and scrubbing backwards must reproduce
  identical geometry, visibility, and colours.
- Reset animation state, module visibility, transforms, and reaction-specific
  material bindings whenever the reaction changes.
- Add new stable module and colour-slot IDs only by appending them. Never
  renumber an existing ID or invalidate old clips.
- Prefer existing IDs when the new source uses a true semantic alias already
  represented by the runtime contract.
- Preserve source material transparency, emission, metallic, and roughness
  semantics. Override only reaction-dependent colour properties.
- Keep chemical RGB separate from phase-owned opacity.
- Reuse shared geometry such as the main beaker when the existing integrations
  do so. Exclude duplicate modules during baking and record the shared asset
  reference in metadata.
- Parse clips lazily and avoid decoding/loading variants not selected for the
  current reaction.
- Keep material instances reaction-scoped; recolouring one reaction must not
  mutate globally cached material state.
- Do not add a second GPU device, renderer, window, or event loop.
- Preserve the fixed, noninteractive 2.5D camera.

Inspect the source scene's actual object/module names and material slots. Do
not guess them from this prompt. If names do not map to the baker's stable
contract, either correct the source/manifest or append generic runtime IDs in
the baker and matching Rust enums. Validate that every required module is
actually present and visible in the baked clip. Check for source setup geometry
that should remain hidden until its authored start frame.

Create or update an adjacent asset metadata JSON file. Follow the exact schema
used by existing assets and record, as applicable:

- source and exported asset paths;
- source SHA-256;
- runtime clip SHA-256 and byte count;
- frame range, frame count, FPS, and duration;
- track/module inventory;
- runtime material bindings;
- deliberately excluded modules;
- shared geometry references;
- visibility start/end expectations;
- bake command and provenance notes.

Do not fabricate hashes or sizes. Compute them from the exact files after the
final bake. Push the actual `.clip` assets required by the application, but do
not duplicate large FBX/Blender sources in the repository unless existing
policy and the user explicitly require that.

Colour authority must remain:

1. Exact validated `.chems` colour observation at its validated ordinal.
2. Reviewed `macroscopic_materials` catalogue RGB for the exact structure and
   reaction-role context.
3. Conservative phase-appropriate fallback.

Never infer colour from a chemical name or raw formula. Bind each asset slot to
the exact validated reactant/product identity and role. If the product changes
colour during formation, preserve the initial/base colour and animate toward
the validated target at the product-formation or colour-observation ordinal.
Never bake one reaction's chemical colour permanently into the generic asset.

Treat catalogue changes as trust-boundary changes, not ordinary renderer
metadata. Add catalogue material records only when the exact structure,
reaction-role context, premise/evidence coverage, review attestation, canonical
catalogue digest, review digest, and host pins can all be updated consistently.
Do not self-promote unsupported scientific facts merely to activate an asset.

Rendering and timing requirements:

- Use the supplied geometry as authored rather than replacing it with
  placeholder primitives.
- Preserve the project's restrained stylised/low-poly physical presentation.
- Do not display molecule models in the normal 3D macroscopic view.
- Do not add camera shake or free camera controls.
- Make transparent layers readable through the shared glass without excessive
  overdraw.
- Do not globally warp a clip to hide badly distributed source keys without
  measuring its event timing. Identify meaningful boundaries such as entry,
  contact, formation, peak activity, decay, and settling. If runtime retiming
  is necessary, use a monotonic, velocity-continuous, deterministic mapping and
  retain the exact final frame.
- Keep chemistry progression authoritative. Authored visual timing may map onto
  validated observation ordinals but must not create, remove, or reorder
  chemical events.

Testing requirements:

Add narrow tests that prove the architecture rather than merely checking one
fixture name:

- positive selection for every supported typed layout;
- rejection for missing/ambiguous phases and incompatible layouts;
- every higher-priority category still wins;
- local/reviewed and dynamically researched outcomes reach the same category;
- exact material-slot binding for all reactants and products;
- reviewed RGB propagation and conservative missing-colour fallback;
- product colour transition at the validated ordinal;
- deterministic absolute timeline sampling, reset, replay, and backwards seek;
- shared-geometry reuse and lazy variant loading where applicable;
- clip frame/track/module/material contract and bounds;
- absence of reaction-specific renderer branching.

Use representative reactions only as test inputs to exercise generic rules.
Do not add production branches for those examples.

Run the narrowest focused tests while iterating. Finish with:

    cargo fmt --all --check
    cargo test -p chem-presentation
    cargo test -p chemspec-app <focused_filter>
    cargo clippy -p chem-presentation -p chemspec-app --all-targets -- -D warnings

Run wider catalogue, agent, or workspace checks when those layers changed.
Normal tests must not consume Codex subscriptions, network access, credentials,
or launch a GPU window. Do not perform screenshots or render previews unless
the user explicitly requests visual verification.

Update:

- crates/chemspec-app/assets/models/README.md
- docs/macroscopic-visual-system.md
- docs/verification.md
- any governing plan or catalogue-review document affected by the change

Preserve unrelated dirty-worktree changes. Do not upgrade Rust, Iced, Bevy, or
major dependencies as part of asset integration. Note that current ChemSpec is
an Iced application using its existing GPU surface; verify the actual current
dependency versions instead of relying on historical assumptions.

At completion, report:

1. The architecture and closest existing integration you followed.
2. The generic classification and priority rules implemented.
3. How local and researched reactions both reach the animation.
4. Files and binary assets changed.
5. Runtime module/material bindings.
6. Asset hashes, sizes, timeline, and shared-geometry decisions.
7. Colour authority and fallbacks.
8. Focused tests and exact results.
9. Any live GPU/Blender/network verification not performed.
10. Known limitations and the next general-purpose improvement.

Do not commit, push, or open a pull request unless I explicitly request those
operations after reviewing the implementation.
```

