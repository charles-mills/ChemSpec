# Automatic animation system

## Decision

ChemSpec provides two separate generated experiences downstream of one current
`ValidatedStructuralReaction`:

```text
ValidatedStructuralReaction + StructuralFrame[]
  -> EducationalScenePlanner -> EducationalPlan -> 2D renderer

ValidatedStructuralReaction + reviewed PresentationMetadata
  -> RealWorldScenePlanner -> ScenePlan
  -> asset/effect/camera registries -> 3D renderer
```

Raw structural frames are not a finished teaching timeline. The 3D experience
is not a spatialized molecular diagram. No supported reaction owns a bespoke
animation module.

## Authority boundaries

`.chems` selects the reviewed rule and preserves the mandatory representative
and explanatory disclosures. The immutable catalogue owns stable atoms,
structural states, typed operations, observations, and separately reviewed
macroscopic presentation metadata. `chem-engine` is the only layer that can
construct the trusted reaction consumed by either planner.

The educational planner may choose pacing, grouping, focus, annotations, and
transitions. It may not invent or reorder chemical changes in a way that changes
their meaning. The real-world planner may instantiate only assets, effects, and
camera behaviours permitted by reviewed presentation metadata and typed
observations. It never interprets bonds or atoms as laboratory apparatus.

## Educational plan

The renderer-independent `EducationalPlan` is a deterministic sequence of
scenes and cues. Reusable scene roles include introduction, reactant setup,
equation, conceptual setup, structural change, observation connection, product
formation, and summary. Reusable cues include focus, highlight, electron-state
display, typed structural-operation animation, observation display, equation
emphasis, and transition.

Balanced coefficients and display formulae are reviewed catalogue fields bound
into the validated reaction. The planner passes that typed equation through;
neither the planner nor the renderer derives stoichiometry from drawn atoms.

Every structural-operation cue references the exact before/after frame digests
and stable affected atom IDs. Stable atom identity is preserved visually.
Interpolation never creates a semantic state.

Each generated structural stage is followed by a typed `ExplanationPause`.
The planner selects a reusable semantic label kind, concise text, affected atom
targets, connector policy, and an adaptive reading duration. The Canvas draws
the label outline progressively, fades text in after the outline, holds it for
reading, and fades it before playback continues. Connector lines have no
arrowhead unless direction itself carries scientific meaning. Explanations are
inside the animation composition, never in a permanent caption panel.

## Real-world scene plan

The renderer-independent `ScenePlan` contains:

- reusable environment, vessel, material-form, and physical-state asset IDs;
- semantic identities and reviewed appearance profiles kept separate from mesh
  identity;
- deterministic transforms and variation seeds;
- typed reusable effects with bounded parameters;
- reusable near-isometric camera behaviours and timing;
- the validated reaction/catalogue identities and virtual-only disclosure.

The initial registry is intentionally small but architectural: a laboratory
bench, presentation platform, beaker, liquid volume, generic metal chunk,
precipitate cloud, bubbles, and reusable lighting/camera rigs. New common assets
extend the registry; new reactions select existing profiles whenever possible.
Assets may be stored meshes or deterministic procedural low-poly meshes, but
runtime selection never regenerates reviewed common assets per reaction.

Vessel camera poses begin above the rim and target the liquid/reaction surface.
The default lighting combines ambient, key, fill, hemispheric, and rim terms so
clear glass, liquid, reactants, and effects remain readable. Three-dimensional
playback advances continuously through the complete scene plan; play/pause,
restart, timeline, speed, orbit, and zoom are presentation controls rather than
manual chemistry-stage gates.

## Blocking and invalidation

Malformed, ill-typed, incomplete, invalid, unsupported, stale, and system-error
results produce neither plan. Source, catalogue, validated-artifact, frame, or
presentation-profile digest changes invalidate both experiences. A missing
presentation profile may still permit the 2D educational plan, but blocks the
real-world scene honestly rather than guessing.

## Verification

Tests must prove:

- planners are deterministic and reaction-agnostic;
- every educational operation cue maps to a validated operation and frame pair;
- every scene object/effect/camera cue resolves in its registry;
- no effect appears without reviewed metadata and, where required, a matching
  typed observation;
- semantic identity and mesh identity remain distinct;
- deterministic variation changes with the declared seed only;
- source or catalogue changes stale both plans;
- both renderers use Iced's existing renderer/device boundary;
- live smoke tests show continuous 2D playback and a depth-tested macroscopic
  3D diorama.
