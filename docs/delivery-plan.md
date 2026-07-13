# Build Week delivery plan

## Goal

Deliver one coherent, packaged vertical slice that demonstrates:

```text
Natural-language request
    -> visible GPT-5.6/Codex research
    -> cited evidence
    -> generated .chems
    -> deterministic validation
    -> explanatory particle simulation
```

Breadth is secondary to completing this trust loop.

This document defines milestone strategy. Task IDs, dependencies, timeboxes,
integration gates, and the dated team sequence live in the
[implementation plan](implementation-plan.md).

## Team ownership

### Workstream 1 — Language and kernel

Owns:

- `.chems` grammar, parser, formatter, and diagnostics;
- formula, charge, phase, quantity, and unit syntax;
- proof-tactic execution;
- validation plumbing;
- normalized validated representation.

This workstream owns language correctness, not empirical chemistry.

### Workstream 2 — Chemistry and catalogue

Owns:

- trusted substance catalogue;
- dissociation and solubility facts;
- supported reaction rules;
- product inference;
- stoichiometry and limiting reagents;
- observations and scientific fixtures;
- primary chemistry-accuracy review.

### Workstream 3 — Application and simulation

Owns:

- Iced application structure;
- startup and provider selection;
- request and workflow experience;
- source editor and diagnostic presentation;
- explanation and provenance views;
- particle model and renderer;
- packaging and demo flow.

This workstream develops against static validated fixtures before full engine
integration.

### Workstream 4 — Agent and providers

Owns:

- Codex CLI preflight and invocation;
- direct Responses API provider;
- structured output and JSONL events;
- web research and provenance;
- repair loop;
- provider-neutral workflow events;
- safety routing, cancellation, and failures.

The agent workstream produces text and provenance, never validated domain data.

## Shared fixtures

Create these integration artifacts first:

```text
fixtures/silver-chloride.chems
fixtures/silver-chloride.validated.json
fixtures/silver-chloride.research.json
```

Each workstream can move independently:

- the language parser consumes the source fixture;
- the chemistry engine reproduces the reviewed validated fixture;
- the app renders the validated fixture;
- the agent produces equivalent source and provenance.

## Milestone 0 — Contracts and workspace

Exit conditions:

- Rust workspace and crate boundaries exist;
- shared domain types compile;
- fixture schemas and canonical files exist;
- product result states and agent events are represented;
- CI runs formatting, compilation, and tests.

Do not begin with visual polish or broad catalogue population.

## Milestone 1 — Offline vertical slice

Implement the canonical experiment without an LLM dependency:

1. Open the canonical `.chems` fixture.
2. Parse it and show useful diagnostics.
3. Validate it against the first catalogue facts and precipitation rule.
4. Produce a `ValidatedExperiment` and derivation.
5. Render the initial ion state and silver chloride precipitation.
6. Explain participating and spectator ions.
7. Make a source edit invalidate the simulation.

Exit condition: the complete source-to-simulation trust boundary works offline.

## Milestone 2 — Agent vertical slice

Add the provider-neutral research and authoring loop:

1. Implement provider selection and Codex preflight.
2. Invoke `codex exec` with structured output and visible events.
3. Implement direct Responses API mode.
4. Normalize both into one `ResearchResult`.
5. Validate generated source.
6. Return diagnostics for a bounded repair.
7. Persist provenance separately from `.chems`.

Exit condition: both providers can complete the canonical request or fail with a
clear product state.

## Milestone 3 — Chemistry breadth and trust

Add one reviewed example for each initial family:

- silver chloride precipitation;
- strong acid/base neutralization;
- acid/carbonate gas formation;
- no-net-reaction case.

Complete:

- catalogue provenance;
- assumptions and unsupported states;
- source cards;
- safety scope gate and redirect states;
- agent evaluation corpus;
- deterministic simulation tests.

Exit condition: the product demonstrates variety without leaving the trusted
domain.

## Milestone 4 — Product and submission

Priorities:

1. Coherent startup-to-explanation experience.
2. Reliable packaged demo on the recording machine.
3. Clear setup and run instructions.
4. Cross-platform build evidence proportional to available runners.
5. A public demo video under three minutes.
6. Submission copy explaining where Codex and GPT-5.6 accelerated and power the
   project.
7. Repository licensing, sample data, and testing instructions.
8. The required Codex feedback/session identifier.

Do not trade the working canonical journey for speculative reaction breadth or
renderer sophistication.

## Integration rhythm

Integrate through contracts rather than waiting for whole subsystems:

- app consumes fixtures before the validator is ready;
- agent returns fixture-shaped structured data before live research is ready;
- chemistry rules use hand-authored parsed structures before the parser is
  complete;
- renderer consumes deterministic simulation frames before playback is wired.

At each milestone, run the same canonical experiment through every available
layer and record the first boundary that fails.

## Demo outline

The final video should show:

1. The problem: safe, affordable chemistry exploration.
2. Codex subscription/API-key selection.
3. The canonical natural-language request.
4. Visible research and cited evidence.
5. Generated `.chems` and deterministic checks.
6. The particle simulation and educational explanation.
7. A deliberate source error, diagnostic, and agent repair.
8. The distinction between model proposal and validated result.

The demo should spend more time showing a working product than explaining the
architecture.

## Scope-control rules

Defer work that does not strengthen the canonical journey:

- 3D rendering;
- molecular dynamics;
- user accounts or cloud persistence;
- classroom administration;
- arbitrary material recognition;
- catalogue self-modification;
- plugin systems;
- complex organic or redox chemistry;
- extensive theming or customization.

These are expansion directions, not Build Week dependencies.
