# System architecture

## Architectural rule

ChemSpec has four distinct authorities:

> The agent produces text; the validator produces trust; the chemistry engine
> produces meaning; the application produces the experience.

No component may bypass the component immediately downstream of it.

## Runtime flow

```text
Visual reaction-builder request or natural-language request
    -> AgentProvider
    -> research packet + .chems source
    -> chems-lang parser
    -> chem-engine validator
    -> ValidatedExperiment
    -> simulation model
    -> SimulationFrame
    -> Iced/wgpu presentation
```

Reverse dependencies are prohibited:

- the validator does not depend on the UI;
- the simulation does not parse `.chems`;
- the agent does not construct particles or validated domain values;
- the renderer does not infer chemistry;
- the application cannot mark an experiment valid.

## Proposed Rust workspace

```text
                         ┌───────────────┐
                         │ chemspec-app  │
                         └───────┬───────┘
             ┌───────────┬───────┼───────────┐
             ▼           ▼       ▼           ▼
        chems-lang  chem-engine  agent   simulation
             │           │                   │
             └───────────┴─────────┬─────────┘
                                   ▼
                              chem-domain
```

### `chem-domain`

Pure, stable domain types:

- elements and formulas;
- charges and phases;
- quantities and units;
- substance identifiers;
- reactions and observations;
- assumptions;
- derivation records;
- `ValidatedExperiment`.

It has no parsing, networking, Iced, or GPU dependencies.

### `chems-lang`

Source-language concerns:

- lexer and parser;
- source spans;
- syntax tree;
- formatter;
- syntax diagnostics;
- `.chems` serialization.

It may use domain primitives such as formulas and quantities, but it does not
decide whether a reaction is chemically supported.

### `chem-engine`

Trusted chemical meaning:

- versioned catalogue;
- semantic name and type resolution;
- reaction rules;
- equation validation;
- product inference;
- stoichiometry;
- validation derivations;
- construction of `ValidatedExperiment`.

This is the only crate allowed to construct a validated experiment.

### `agent`

Provider-neutral agent orchestration:

- `CodexCliProvider`;
- `ResponsesApiProvider`;
- preflight and availability checks;
- workflow events;
- structured research results;
- source generation;
- provenance;
- bounded repair requests;
- cancellation and timeouts.

It returns `.chems` source and provenance, never trusted chemistry.

### `simulation`

Renderer-independent explanatory model:

- representative particle identities;
- reaction stages;
- phase-specific behaviour;
- precipitation aggregation;
- gas bubbles;
- playback and timeline state;
- conversion from `ValidatedExperiment` to render snapshots.

The chemistry engine determines the maximum reaction extent. Simulation state
cannot consume more of a species than the validated result permits.

### `chemspec-app`

Product composition:

- Iced state, messages, tasks, and subscriptions;
- visual reaction-builder composition and element catalogue presentation;
- startup and provider selection;
- request composer and workflow timeline;
- `.chems` editor;
- diagnostics and derivation views;
- source cards and provenance;
- simulation controls;
- Iced/wgpu particle widget;
- native storage and credential integration.

Only this crate depends on Iced and `iced_wgpu`.

## Shared contracts

### `ValidatedExperiment`

The exact Rust representation will evolve, but it must preserve:

```text
ValidatedExperiment
  catalogue version and digest
  declared conditions
  normalized input substances and quantities
  supported reaction class
  normalized equations
  limiting reagent
  consumed and remaining quantities
  products and phases
  supported observations
  explicit assumptions
  derivation artifact
```

It must be impossible to construct this type through ordinary public fields.
Construction remains inside the chemistry engine.

### `AgentEvent`

Provider-specific events are normalized into product events:

```text
AgentEvent
  interpreting
  substance_identified
  researching
  source_found
  hypothesis_formed
  drafting
  source_generated
  validating
  repair_started
  repair_applied
  completed
  redirected
  unsupported
  failed
```

The UI may preserve raw provider diagnostics for troubleshooting, but it should
not expose hidden chain-of-thought or depend on provider-specific event shapes.

### Research result

Both providers return the same strict structured result:

```text
ResearchResult
  interpreted request
  identified substances
  conditions and assumptions
  evidence claims
  source metadata
  reaction hypothesis
  generated .chems source
  safety disposition
```

## Iced application model

Iced's update loop owns product state transitions. Background provider work and
validation are represented as tasks that return typed messages. Subscriptions
exist only while a continuing source of events is active, such as a provider
process or playing simulation.

Suggested feature state boundaries:

```text
App
  reaction_builder
    element_library
    workspace
    sequence
  startup
  provider
  workflow
  editor
  experiment
  simulation
  sources
```

The reaction builder owns only presentation and user intent. Element metadata
shown by `element_library` is a curated UI catalogue; selecting or dragging an
element cannot construct a compound, reaction, or `ValidatedExperiment`.
`workspace` stores placed element identities and normalized presentation
positions. Its small closed-world combination catalogue may label a grouping
as a composition preview and present its members as one compound card, but the
preview is not a supported-reaction verdict and never constructs a trusted
domain value. Member atom identities remain the durable state; the compound
card is derived presentation. The workspace will eventually
serialize typed user intent for the normal parser/validator path. `sequence`
and result presentation must consume engine or simulation outputs rather than
infer chemistry from tile placement.

Stage 3 atomic canvases are also derived presentation. They consume curated
element shell metadata and existing composition-preview membership, and may
animate only illustrative outer-electron positions. Grouped compositions keep
their member atomic models visible and may mark curated covalent relationships
with shared-electron pairs. They cannot create a validated reaction sequence,
result state, or `SimulationFrame`.

The Stage 4 reaction-candidate catalogue is an input-composition affordance,
not the chemistry catalogue. A match may enable and queue the trigger, while an
exact mismatch disables it. Neither branch validates chemistry. The queued
candidate must still travel through the provider, parser, chemistry engine, and
validated simulation boundary before any validated reaction animation can
begin.

The Stage 5 `reaction_sequence` module is an untrusted storyboard preview, not
the architecture's validated `sequence` or `simulation` feature. It consumes
only the Stage 4 candidate's reviewed presentation data, preserves every listed
visual product, and owns playback presentation state. It cannot produce domain
products, validation dispositions, `ValidatedExperiment`, or `SimulationFrame`.
Validated playback remains blocked until the normal downstream pipeline exists.

Stale asynchronous results carry a request or generation identifier and are
ignored when they no longer match active state.

## GPU boundary

The first renderer is a clear 2D vessel. It should use Iced's existing native
`iced_wgpu` renderer rather than creating a second window surface, adapter, or
device.

The particle widget may begin with Iced canvas or mesh primitives and progress
to a custom shader with instanced buffers. The boundary remains the same:

- simulation owns meaningful particle state;
- the widget owns persistent GPU pipelines, buffers, bind groups, and meshes;
- only changed instance data is uploaded;
- the GPU renders the explanatory state but does not decide chemistry.

## Simulation fidelity

The simulation must faithfully preserve:

- stoichiometric ratios;
- limiting-reagent consumption;
- remaining quantities;
- phases;
- spectator ions;
- the validated reaction sequence;
- supported observable effects.

It explicitly treats particle count, size, colour, speed, spatial density,
water representation, and elapsed time as illustrative.

Fidelity can grow in this order:

1. Stoichiometric transformation.
2. Phase-specific motion and appearance.
3. Concentration- and quantity-sensitive presentation.
4. Qualitative, evidence-backed relative timing.
5. Specialized physical models only for domains that justify them.

## Persistence

The human-authored artifact is the `.chems` file. Generated provenance is stored
separately and associated through a source hash:

```text
experiment.chems
experiment provenance
  source hash
  catalogue version and digest
  provider and model
  evidence packet
  source annotations
  validation report
  timestamps
```

The application may bundle these records for export, but the generated metadata
must not clutter or silently modify the source language.

## Platform decision

ChemSpec remains a native Rust application using Iced and `wgpu` on macOS,
Windows, and Linux. An Electron or browser shell would add a second language
boundary without improving the validator, process integration, native
credential storage, or renderer architecture.

Platform-specific code should be limited to binary discovery, credential
storage, file dialogs, and packaging.

## Framework references

- [Iced 0.14](https://github.com/iced-rs/iced/tree/0.14.0)
- [`iced_wgpu`](https://github.com/iced-rs/iced/blob/0.14.0/wgpu/README.md)
- [`wgpu` API documentation](https://docs.rs/wgpu/latest/wgpu/)
