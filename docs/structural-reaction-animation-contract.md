# Structural reaction animation contract

## Authority

The ChemS source selects a reviewed generalized rule through its proof block:

```chems
by
  apply Rules.AlkaliMetalWithWater
    metal := lithium
    water := water
    hydroxide := lithiumHydroxide
    gasProduct := hydrogen
```

The source does not contain atom mappings, electron movements, coordinates,
timing, meshes, effects, or camera instructions. Rule expansion, structural
operations, observations, and immutable states remain owned by the trusted
catalogue and kernel pipeline.

## Pipeline

```text
.chems source
  -> lossless CST and source AST
  -> trusted generalized-rule expansion
  -> immutable structural derivation
  -> kernel validation
  -> SimulationFrames
       -> chem-presentation -> guided 2D scenes
       -> host-selected presentation profile -> macroscopic ScenePlan
```

Only privately constructed trusted `SimulationFrames` may enter
`chem-presentation`. The presentation planner and renderers cannot parse
source, resolve rules, construct products, change bonds, move electrons, or
manufacture observations.

Macroscopic effects declare a typed observation predicate. Planning rejects an
effect unless that predicate occurs in the trusted frame generation. The
profile may select reusable meshes, styling, pacing, and cameras; those choices
are illustrative rather than chemical claims.

## Model disclosure

ChemS 1 accepts the representative event model and explanatory sequence model.
The visualization is not molecular dynamics, a measured trajectory, an
elementary-mechanism claim, or a laboratory procedure.

## Invalidation

Editing source immediately discards validated frames and both presentation
plans. Revalidation creates a new frame digest and presentation generation.
Malformed, unsupported, stale, review-candidate, or invalid results cannot be
animated.

## Current vertical slice

The integrated slice uses the AI-attested generalized alkali-water catalogue
under `catalogue/trusted/` and member-specific evidence packets under
`conformance/`. The application selects exact Li, Na, or K experiences from the
same family and renders:

- a continuous guided 2D structural explanation from exact kernel frames;
- operation-specific labels with stable atom identity;
- covalent, dative, ionic, and metallic relationships as distinct concepts;
- a depth-tested macroscopic 3D scene using reusable procedural assets;
- gas and reactant-consumption effects only because corresponding validated
  observation predicates exist.

The canonical presentation profile lives beside the application trust boundary
in `crates/chemspec-app/src/chemistry.rs`. It has no route back into expansion
or validation.
