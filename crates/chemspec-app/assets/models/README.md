# Embedded macroscopic models

`metal.fbx` is the user-provided source model for the imported metal asset.
Its SHA-256 digest is
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

The source asset is an original ChemSpec project asset created in-house.

## Reaction assemblies

Every macroscopic reaction scene (alkali water, gas evolution,
precipitation, neutralisation, combustion, metal displacement, solid–solid
synthesis, and the heavy-alkali explosions) is generated procedurally in
`crates/chemspec-app/src/structural_3d/`. The baked `.clip` animations that
previously lived here — and the Blender pipeline that produced them — were
removed in favour of deterministic in-code choreography; see
`structural_3d/<scene>.rs` for each scene and git history for the retired
assets.
