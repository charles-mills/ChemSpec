# `.chems` structure impact of the animation overhaul

## Summary

This animation and presentation overhaul makes **no normative change** to the
`.chems 1` file structure.

The change does not modify:

- `grammar/chems.ebnf`;
- the language rules in `docs/chems-specification.md`;
- the lossless parser, formatter, diagnostics, or source AST;
- existing `.chems` fixtures; or
- the required order or meaning of experiment sections.

Existing `.chems 1` sources therefore remain valid without migration. No new
animation, caption, camera, material, effect, or timing fields are required in
source files.

## Existing structural-model binding

Animated experiments continue to use the already-established required `model`
section:

```chems
model
  event := representative
  sequence := explanatory
  structuralRule := ChemSpec.Structural.Redox.LithiumWater
```

This section selects a reviewed structural rule and carries the required model
disclosures. It does not describe electron paths, atom coordinates, animation
stages, narration, 3D objects, physical effects, or camera movement.

## Data flow introduced by the overhaul

The new presentation behavior is entirely downstream of validation:

```text
.chems source
  -> parse and type-check
  -> reviewed catalogue and structural-rule resolution
  -> trusted SimulationFrames
     -> immutable structural frame sequence
        -> deterministic educational scenes and labels
        -> reusable 2D renderer
     -> typed observations + reviewed presentation profile
        -> deterministic macroscopic timeline and annotations
        -> reusable 3D assets, effects, cameras, and renderer
```

The educational planner composes wording from typed operations, validated atom
states, reviewed equation formulae, and typed observations. The macroscopic
planner composes scene beats from reviewed presentation metadata and those
validated observations. Neither planner reads raw source text, calls an AI
service, generates runtime code, or lets the renderer infer chemistry.

## Authoring impact

Reaction authors do not add bespoke animation instructions to `.chems` files.
Natural 3D easing, particle variation, emission envelopes, shared reaction
motion, and camera drift are likewise reusable renderer behaviours selected by
typed reviewed effect and intensity metadata; they do not extend the language
or introduce reaction-specific animation fields.
The final product record also requires no new syntax: it reads validated final
product membership and structural relationships, with reference atomic masses
coming from bundled element presentation metadata.
Supporting a new animated reaction still requires a reviewed catalogue rule and,
for the macroscopic view, a reviewed reusable presentation profile. If that
trusted metadata is unavailable or incompatible, animation remains unavailable
rather than being guessed from prose.

Any future proposal to add presentation syntax to `.chems` is a language-contract
change and must update the normative specification, grammar, parser, formatter,
diagnostics, conformance fixtures, producers, consumers, and migration guidance
in one reviewed change.
