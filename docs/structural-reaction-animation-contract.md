# Structural reaction animation contract

## Decision

`.chems 1` requires explicit representative/explanatory model disclosures and
an applied, catalogue-defined reaction rule:

```chems
model
  event := representative
  sequence := explanatory

observe from Evidence.AlkaliWaterLithium@1
  gas hydrogen evolves claim R1
  reactant lithium disappears claim R2

by
  apply Rules.AlkaliMetalWithWater
    metal := lithium
    water := water
    hydroxide := lithiumHydroxide
    gasProduct := hydrogen
```

The source selects chemistry and evidence. It does not contain atom maps,
electron paths, coordinates, animation timing, assets, effects, or camera
instructions.

## Authority

The trusted catalogue owns elements, species templates, generalized rule
families, structural certificates, and observation predicates. The kernel
expands and validates the selected rule, then privately constructs
`SimulationFrames`. Application and renderer code cannot construct that type.

The application may bind the trusted generation to a `PresentationProfile`
containing reusable assets, effects, camera cues, timing, and a display
equation. Presentation metadata cannot change atoms, bonds, products,
observations, validation status, or frame order.

## Required downstream pipeline

```text
.chems source
  -> lossless CST and source AST
  -> trusted catalogue and evidence resolution
  -> generalized-rule expansion
  -> typed structural certificate
  -> kernel validation and deterministic graph execution
  -> SimulationFrames
     -> EducationalScenePlanner -> continuous 2D educational page
     -> PresentationProfile + typed observations
        -> RealWorldScenePlanner -> reusable assets/effects/cameras
           -> dedicated macroscopic 3D page
```

No planner or renderer may parse source, resolve a rule, infer atom mappings,
derive electron movement, or create chemistry. Both plans are bound to the same
current `SimulationFrames` digest.

## Model disclosures

For language major 1 the accepted values are:

- `event := representative`
- `sequence := explanatory`

The visualization is a representative educational event and an explanatory
structural sequence. It is not a molecular-dynamics result, measured
trajectory, laboratory procedure, or experimentally established elementary
mechanism. Geometry, timing, interpolation, and camera movement remain
illustrative.

## Invalidation and rendering

Source, catalogue, evidence, expanded certificate, validation, and frame
digests form one generation. Editing source or changing an upstream digest
immediately makes both animation pages stale.

The 2D page teaches operation-specific changes, stable atom identity,
electrons, charges, bonding domains, products, and synchronized observations.
Each operation and its deterministic explanation share one learning beat. Its
absolute timeline is scrubbable, supports chapter stepping, and preserves
elapsed-time overshoot across scene boundaries.

The 3D page is a separate macroscopic presentation. It uses reusable low-poly
XYZ geometry, complete scene transforms, deterministic seeded effects, opaque
and transparent depth passes, an absolute scrubbable timeline, and elevated
near-isometric camera motion on the existing Iced/wgpu device and surface. It
does not render structural atoms as laboratory objects.

Only trusted, current frames may produce either plan. Malformed, ill-typed,
invalid, unsupported, system-error, and stale states are non-animatable.

## Implemented vertical slice

The promoted catalogue lives at `catalogue/trusted/core-chemistry/`. It
contains the generalized alkali-water, silver-halide precipitation,
neutralization, carbonate/bicarbonate gas-evolution, and halogen-displacement
families. The application has a typed finite request and deterministic source
authoring path for all 36 supported bindings. The reactant composer currently
recognises and routes every one of those bindings in either reactant order.
Recognised but deliberately unsupported pairs explain the missing model;
recognised uncatalogued pairs and unrecognised drafts remain distinct states.

The kernel verifies exact structural transitions, electron bookkeeping,
represented-electron and charge conservation, bond changes, stable atoms,
observation activation, and complete product assignment. Every trusted binding
opens the structural 2D explanation and a family-selected macroscopic scene.
The macroscopic profiles cover alkali-water, silver-halide precipitation,
acid-base neutralization, carbonate/bicarbonate gas evolution, and aqueous
halogen displacement. Effects remain gated by observations in the trusted
frames and cannot begin before the matching observation becomes active.
Value-bearing visuals such as precipitate colour are selected from and bound
to the exact trusted value. A family with no supported visible effect is
presented without inventing one.

Future operation or presentation variants must be added vertically: catalogue
schema, trusted transition validation, frames, planner primitives, renderers,
conformance, and disclosures must remain aligned.

For direct live smokes on macOS:

```sh
just agent-smoke 2d
just agent-smoke 3d
just agent-smoke 3d silver-halide-precipitation-bromide
just agent-smoke stop
```

These commands recreate and byte-check the uniquely identified
`ChemSpec Agent Smoke.app` bundle before launching it. Computer Use must target
`ChemSpec Agent Smoke` and verify the mode-specific `Structural 2D` or
`Structural 3D` window title. It must not target a release-named `ChemSpec`
bundle. The optional second argument is an exact supported reaction ID; an
unknown ID fails a non-GUI validation handshake before the bundle is opened,
instead of silently opening the default reaction.

On other platforms, the direct binary flags remain available:

```sh
cargo run -p chemspec-app -- --structural-2d-smoke
cargo run -p chemspec-app -- --structural-3d-smoke \
  --smoke-reaction=acid-carbonate-sodium-chloride
```

These launch options still cross the complete bundled source, trusted
catalogue, evidence, expansion, validation, and frame boundary. They never
construct renderer chemistry directly.
