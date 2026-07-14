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

The promoted catalogue lives at
`catalogue/trusted/periodic-table-and-alkali-water/`. One generalized
`Rules.AlkaliMetalWithWater` family supports the lithium, sodium, and potassium
experiences. The three `.chems` sources live under `conformance/end-to-end/` and
their observation evidence packets live under `conformance/observations/`.

The kernel verifies exact structural transitions, electron bookkeeping,
represented-electron and charge conservation, bond changes, stable atoms,
observation activation, and complete product assignment. The host then selects
the same reusable presentation machinery for all three trusted outcomes.

Future operation or presentation variants must be added vertically: catalogue
schema, trusted transition validation, frames, planner primitives, renderers,
conformance, and disclosures must remain aligned.

For direct live smokes:

```sh
cargo run -p chemspec-app -- --structural-2d-smoke
cargo run -p chemspec-app -- --structural-3d-smoke
```

These launch options still cross the complete bundled source, trusted
catalogue, evidence, expansion, validation, and frame boundary. They never
construct renderer chemistry directly.
