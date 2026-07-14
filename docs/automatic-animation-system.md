# Automatic animation system

## Decision

ChemSpec generates two renderer-independent plans from one current trusted
frame generation:

```text
SimulationFrames -> EducationalPlan -> Iced Canvas 2D renderer
SimulationFrames + PresentationProfile -> ScenePlan -> Iced/wgpu 3D renderer
```

No supported reaction owns a bespoke chemistry animation module. The 2D
renderer adapts exact frame atoms, electron states, bonds, ionic associations,
metallic domains, operations, and observations into drawing primitives. The 3D
renderer deliberately receives no atom graph.

## Educational plan

`chem-presentation` adds deterministic pacing around the validated frame order:
introduction, reactant setup, equation, structural change, explanation pause,
observation connection, and summary. It does not reorder or synthesize state
transitions.

Every structural scene identifies its exact before and after state digests.
Explanation labels derive from the typed operation view, including dative and
metallic operations supported by the authoritative language.

## Macroscopic plan

The scene plan contains reusable asset, appearance, effect, transform, and
camera enums. Current procedural assets include a bench, cylindrical glassware,
liquid volume, material chunk, particle cluster, and gas cluster. The wgpu
renderer provides depth testing, continuous playback, camera orbit, and zoom.

Each effect is bound to an observation predicate. Compilation fails rather than
guessing when the trusted frames do not contain that predicate. Presentation
geometry, intensity, interpolation, timing, and camera motion remain explicitly
illustrative.

## Blocking and invalidation

The application removes both plans whenever source becomes stale or validation
fails. Only trusted frames from the host-pinned catalogue attestation can open
the guided animation. Review-candidate frames cannot cross this boundary.

## Verification

Tests cover deterministic scene geometry, trusted-plan construction, provider
startup, source invalidation, workspace conformance, and the existing kernel
invariants. A live GUI smoke remains a separate platform check because the test
suite does not create a native window.
