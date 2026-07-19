# Reusable low-poly scene assets

ChemSpec's Stage 5 real-world renderer resolves host-selected `AssetProfile` values
through `crates/chemspec-app/src/scene_registry.rs`. The current library stores
small deterministic procedural mesh recipes for the common low-poly primitives
used by the first dioramas: benches/platforms, cylindrical glassware, liquid
volumes, generic material chunks, particle clusters, and gas clusters.

These recipes are shared assets. Presentation profiles select them downstream
of validated chemistry and attach a separate semantic identity and appearance
profile. Do not add reaction-named geometry. A future imported `.glb` replaces
or extends one registry recipe and is stored under this directory once; runtime
scene plans continue to refer to the stable generic `AssetProfile`.

Recommended imported-asset layout:

```text
assets/
  laboratory/
  materials/
  environments/
  effects/
```
