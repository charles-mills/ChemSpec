# Structural reaction animation contract

## Decision

`.chems 1` requires one explicit reviewed structural-rule binding for every
experiment. The binding lives in a required `model` section after `procedure`
and before any `expect` blocks:

```chems
model
  event := representative
  sequence := explanatory
  structuralRule := ChemSpec.Structural.Precipitation.SilverChloride
```

This is a deliberate compatible-evolution decision made before a public
validated-artifact compatibility surface exists. Existing repository fixtures
are migrated in the same change.

## Authority

The source selects a stable rule identity and makes two mandatory disclosures.
It does not contain an atom map, electron movements, bond operations,
intermediates, coordinates, animation timing, or camera instructions.

The selected immutable catalogue bundle owns the reviewed rule record. Rule
resolution must prove that the record exists, is reviewed, belongs to the
selected catalogue digest, applies to the resolved reaction opportunity, and
matches the authored claims. Unknown rules are ill-typed; known but
inapplicable or out-of-domain rules are unsupported; a rule whose certificate
violates a mandatory invariant is invalid.

## Required downstream pipeline

```text
.chems source
  -> lossless CST and source AST
  -> catalogue and structural-rule resolution
  -> deterministic reviewed-rule expansion
  -> typed structural certificate
  -> trusted graph-state and chemistry validation
  -> ValidatedStructuralReaction
     -> renderer-independent structural frames
        -> EducationalScenePlanner -> continuous 2D educational page
     -> typed observations + reviewed presentation metadata
        -> RealWorldScenePlanner -> reusable assets/effects/cameras
           -> dedicated macroscopic 3D page
```

No planner or renderer may parse source, resolve rules, infer atom mappings,
derive electron movement, or create chemistry. The 2D planner consumes the
trusted frame sequence. The 3D planner consumes validated reaction identity,
typed observations, and reviewed macroscopic presentation metadata; it does
not reinterpret atom coordinates as a physical laboratory scene.

## Model disclosures

For language major 1 the only accepted values are:

- `event := representative`
- `sequence := explanatory`

The visualization is a representative educational event and an explanatory
structural sequence. It is not a molecular-dynamics result, measured trajectory,
or experimentally established elementary mechanism. Presentation geometry and
interpolation remain illustrative.

## Invalidation and rendering

The source digest, catalogue digest, rule identity, certificate digest,
validated artifact digest, and frame digest form one generation. Editing source
or changing an upstream digest immediately makes both animation pages stale.

The 2D page teaches operation-specific changes, stable atom identity, electrons,
charges, bonding domains, products, and synchronized observations through a
deterministic multi-scene educational plan. Every operation is followed by a
generated explanation pause whose contextual in-canvas label uses a traced
outline, adaptive reading hold, fade transitions, and a no-arrow connector to
the affected structure. It then navigates to a separate
macroscopic 3D page using reusable low-poly XYZ geometry, scene transforms,
depth, continuous media playback, bright multi-source lighting, and an elevated
near-isometric camera targeted at the reaction surface on the existing
Iced/wgpu device and surface.

Only `Validated` and `ValidatedWithAssumptions` may produce frames. Malformed,
ill-typed, incomplete, invalid, unsupported, system-error, and stale states are
non-animatable.

## Implemented canonical vertical slice

The canonical `ChemSpec.Structural.Precipitation.SilverChloride` rule is stored
in `fixtures/catalogue/silver-chloride.catalogue.json`. Its reviewed record
binds the two authored reactants and two products to seven stable atom
identities, four immutable states, one ionic association, two product
assignments, and synchronized product/colour observations. Nitrate connectivity
is represented with covalent bond orders; silver/chloride association remains a
distinct ionic relationship.

`chem-catalogue` validates the immutable record and its evidence/review links.
`chem-engine` resolves the source binding, proves that the rule matches the
authored reactants and claimed products, validates state transitions,
conservation, product coverage, observations, and disclosures, and privately
constructs `ValidatedStructuralReaction`. Only that trusted type can generate
`StructuralFrame` values.

The 2D planner consumes those frames as a generated teaching sequence with active atoms, formal
charges, non-bonding electrons, shared covalent electron pairs, distinct dotted
ionic associations, operation captions, and pending/active/established
observations. The following 3D page consumes the rule's reviewed
`presentation.aqueous-precipitation` profile and resolves a reusable bench,
beaker, liquid volume, precipitate cluster, clouding effect, and camera cues.
It does not render the structural atoms as a macroscopic event.

The reviewed lithium/water slice lives in `fixtures/lithium-water.chems` and
`fixtures/catalogue/lithium-water.catalogue.json`. It exercises the reusable
`TransferMetallicElectron`, `CleaveCovalent`, `FormCovalent`,
`AssociateIonic`, and `AssignProduct` operations across eleven immutable
states. The kernel verifies exact endpoint electron states, metallic-domain
electron changes, represented-electron and charge conservation, bond changes,
stable atoms, and complete product assignment. Its separate reviewed
`presentation.reactive-metal-on-water` profile selects the same generic asset,
effect, and camera registries used by other reactions. The sequence is an
explanatory electron-bookkeeping model, not a claimed elementary mechanism.
Its macroscopic plan drives generic contact motion, surface travel, bubbles,
gas release, ripples, splashes, and reactant shrinkage without exposing
structural atom data to the 3D renderer.

Future operation or presentation variants must be added vertically: catalogue
schema, trusted transition validation, shared frames, planner primitives,
renderers, fixtures, conformance, and disclosures must land together.

For direct live smokes of the trusted canonical 2D operation and final 3D
frame, use:

```sh
cargo run -p chemspec-app -- --structural-2d-smoke
cargo run -p chemspec-app -- --structural-3d-smoke
```

This launch option still runs the complete bundled source, catalogue,
expansion, validation, and frame pipeline; it does not construct renderer data
directly or bypass trust checks.
