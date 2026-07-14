# `.chems` implementation plan

## Purpose

This plan implements the complete
[`.chems` specification](chems-specification.md) through independently
verifiable slices. It is the execution authority for language work.

The language design is fixed before implementation begins. A slice may expose a
spec problem, but code convenience alone is not permission to change semantics. A
design change first updates the specification, compatibility analysis,
requirement coverage, and affected conformance cases.

## Current state

The language specification and normative grammar are the only source contract.
Slices 0–4 are complete. The executable conformance scaffold has stable
requirement IDs, schemas, fixture validation, grammar and reserved-word checks,
and coverage reporting. `chem-domain` implements exact values and stable
chemistry identities; `chems-lang` implements the complete lossless source
frontend and formatter; `chem-catalogue` implements versioned digest-bearing
bundles, canonical record ordering, validation, deterministic indexes, and the
reviewed silver-chloride fixture. No legacy grammar or compatibility parser is
retained. `chem-kernel` now resolves complete source into catalogue-backed
`TypedExperiment` HIR with stable IDs, exact typed conditions and quantities,
resolved species/materials/operands, explicit premise and assumption tracing,
and source origins. Procedure execution and claim validation remain outside the
completed boundary.

## Target crate boundaries

```text
chems-compiler
├── chems-lang       lossless source, AST, formatting, diagnostics
├── chem-catalogue   immutable facts, evidence, bundle validation
├── chem-kernel      goals, rules, tactics, derivations, artifacts
└── chem-domain      exact values and stable chemistry/state types
```

### `chem-domain`

Owns pure, serializable values:

- exact integers/rationals and source decimals;
- dimensions, units, quantities, and temperature points;
- elements, formulae, charge, phase, species, and substances;
- materials, inventories, vessels, operations, stages, and events;
- identifiers and schema-version primitives.

It has no parsing, catalogue I/O, proof search, UI, networking, or GPU
dependencies.

### `chems-lang`

Owns:

- language-version dispatch;
- normative encoding/layout lexer and lossless CST;
- source AST and comment attachment;
- syntax diagnostics and safe edits;
- canonical formatter;
- source-origin maps.

It constructs syntax, never a validated chemical result.

### `chem-catalogue`

Owns:

- catalogue schemas and canonical serialization;
- bundle validation and digesting;
- stable fact/substance/evidence identities;
- condition applicability and lookup indexes;
- the reviewed fixture catalogue.

It supplies typed premises and never decides proof goals.

### `chem-kernel`

Owns:

- typed elaboration that requires catalogue resolution;
- procedure state transitions;
- goals, tactics, reaction families, and exact stoichiometry;
- derivation construction/replay;
- private `ValidatedExperiment` construction.

This is the trusted chemistry boundary.

### `chems-compiler`

Owns composition:

- complete result envelopes;
- version/schema selection;
- incremental language-service API;
- CLI commands;
- conformance runner integration.

It has no alternative path around `chem-kernel`.

## Implementation rules

1. Every slice starts by adding or selecting its conformance cases.
2. Golden chemistry outputs are independently authored before implementation
   produces them.
3. Every public interchange envelope carries its relevant schema version.
   Nested domain values are governed by that envelope and do not repeat the
   version on every scalar, identifier, or formula node.
4. No chemistry value uses binary floating point.
5. No invalid, unsupported, or incomplete result can construct an artifact.
6. Normal tests are deterministic and require no provider, API, or network.
7. Unsafe Rust is forbidden throughout language/catalogue/kernel crates.
8. A slice stops at its acceptance boundary; adjacent features wait for their
   own slice.
9. There is no legacy syntax or compatibility path outside the normative grammar.
10. Exact commands and unsupported test boundaries are recorded at every gate.

## First cross-team handoff

Slice 0 is owned by the language workstream and does not require implementation
from another workstream. As soon as Slice 0 establishes requirement IDs,
conformance fixtures, and stable naming, the wider product's contracts phase
starts in parallel:

- the chemistry owner reviews the chemical identities and exact-value boundary,
  then authors the first catalogue facts and expected silver-chloride derivation;
- the agent owner freezes `ResearchResult`, `EvidenceClaim`, and `AgentEvent`
  against a fake-provider fixture;
- the application owner freezes `SimulationFrame` and exercises the Iced shell
  against a hand-authored validated-result fixture.

Within the language plan itself, Slice 3 is the first slice that requires
substantial implementation from another owner: `chem-catalogue` and its
scientifically reviewed fixture belong to the chemistry workstream. The earlier
contract work prevents the other members from waiting for Slice 3 to begin.

## Slice 0 — specification and conformance scaffold

### Depends on

Nothing.

### Deliverables

- Assign stable requirement IDs to every normative language rule.
- Create the conformance manifest schema and runner skeleton.
- Establish fixture directories and naming conventions.
- Add grammar-production validation and reserved-word coverage checks.
- Add canonical-JSON and digest test helpers.
- Add machine-readable schemas for requirements and conformance cases.

### Acceptance

- Every locked specification chapter has requirement IDs.
- Every grammar production is defined and reachable.
- Every grammar keyword appears in the reserved-word test.
- The empty conformance runner reports component coverage rather than silently
  succeeding.
- CI runs specification and manifest validation.

### Explicitly excluded

No normative source parsing or chemical domain implementation.

## Slice 1 — exact domain foundation

### Depends on

Slice 0.

### Deliverables

- `chem-domain` crate.
- Arbitrary-precision rational and source-decimal representations.
- Written precision metadata.
- Dimension vectors, affine temperature points, and the language unit registry.
- Exact unit-expression normalization and conversions.
- Formula structural tree, element resolution interface, adduct/group
  normalization, charge, and phase.
- Typed IDs and canonical serialization primitives.

### Conformance focus

- every accepted unit and exact factor;
- equivalent unit expressions;
- invalid/unknown units;
- negative Celsius and absolute-zero rejection;
- grouped/adduct formula normalization;
- charge/phase equality;
- no-float serialization.

### Acceptance

- All arithmetic and conversion golden cases match exact rational values.
- Property tests cover conversion round trips and formula normalization.
- Canonical JSON is stable across repeated runs.
- `cargo clippy -- -D warnings`, formatting, unit/property tests pass.

### Explicitly excluded

No `.chems` lexer/parser, catalogue facts, materials, or reactions.

## Slice 2 — lossless source frontend

**Status:** complete.

### Depends on

Slices 0 and 1.

### Deliverables

- `chems 1` dispatch without guessing headerless source.
- UTF-8/BOM/NUL/tab validation.
- exact layout lexer with nested comments and lossless tokens.
- CST, source AST, recovery nodes, and comment attachment.
- complete normative grammar parsing.
- source spans and initial `CHEMS-L`/`CHEMS-P` diagnostics.
- canonical formatter and format CLI path.

### Conformance focus

- every grammar production;
- blank/comment-only layout behavior;
- nested/unclosed comments;
- reserved identifiers;
- formula/unit lexical contexts;
- inline/multiline equations;
- all operation, claim, hole, assumption, and tactic forms;
- parse/format/parse and comment preservation.

### Acceptance

- All syntax fixtures produce their golden CST/AST or exact diagnostic codes and
  spans.
- Formatter is idempotent and preserves semantic AST plus all comments.
- Parser fuzzing produces no panic on arbitrary bytes/UTF-8.
- The frontend accepts only the normative grammar selected by `chems 1`.

### Explicitly excluded

No name resolution, units, catalogue lookup, or proof semantics.

## Slice 3 — catalogue foundation

**Status:** complete.

### Depends on

Slices 0 and 1.

### Deliverables

- `chem-catalogue` crate and versioned bundle schema.
- Canonical catalogue JSON and SHA-256 digest binding.
- element, substance/species, medium, fact, evidence, assumption, and coverage
  record variants, with stable evidence-bearing premise identity for both
  identity records and empirical facts.
- internal-consistency validator and lookup indexes.
- independently reviewed minimal silver-chloride catalogue fixture.

### Conformance focus

- duplicate/conflicting records;
- formula/charge/phase inconsistencies;
- condition-domain boundaries;
- evidence required for reviewed facts;
- invalid coverage declarations;
- digest binding across every semantic record category;
- provisional facts excluded from production bundles.

### Acceptance

- Valid bundle loads to one deterministic digest.
- Every corrupt fixture is rejected as a catalogue system error.
- The silver-chloride fixture resolves all required elements, species, medium,
  dissociation, solubility, observation, and evidence facts.

### Explicitly excluded

No source elaboration or reaction inference.

## Slice 4 — typed elaboration and initial materials

**Status: complete.** The canonical silver-chloride source byte-compares with
its checked-in typed-HIR oracle. The quantity/type, formula/species, and
materials conformance components are fully covered through this slice.

### Depends on

Slices 1, 2, and 3.

### Deliverables

- experiment namespaces and stable typed IDs;
- condition and catalogue-selection elaboration;
- quantity and unit typing from source AST;
- formula, species, substance, and medium resolution;
- all material constructors and prepared composition normalization;
- explicit assumption schema resolution;
- complete `TypedExperiment` HIR and source-origin map;
- `CHEMS-T`/`CHEMS-C` diagnostics.

### Conformance focus

- exact Sample/Solution constructor selection;
- wrong dimensions and positivity;
- unknown element versus unsupported substance;
- analytical/actual species separation;
- molar-mass/density/gas/solvent premise boundaries;
- duplicate names and wrong assumption targets.

### Acceptance

- Canonical source produces the independently authored typed-HIR fixture.
- Malformed, ill-typed, and unsupported inputs remain distinct.
- No HIR contains unresolved names, units, dimensions, species, or operands.

### Explicitly excluded

No procedure execution, claims, goals, reactions, or validated artifacts.

## Slice 5 — procedure and stage engine

### Depends on

Slice 4.

### Deliverables

- immutable Stage and inventory-ledger types;
- initial-stage construction and prepared-material validation boundary;
- exact semantics for every procedure operation;
- capacity, location, closure, temperature, pressure, and partition checks;
- reaction-opportunity creation without reaction inference;
- operation/stage source mapping.

### Conformance focus

- successful and failed place/add/combine;
- whole and proportional transfer;
- stir/heat/cool/wait/seal/open;
- ideal filtration and decanting;
- capacity and duplicate-inventory failures;
- state equality and deterministic StageIds.

### Acceptance

- Nonreactive fixtures produce exact independently authored timelines and
  ledgers.
- Every operation conserves inventory.
- Missing physical premises become unsupported rather than guessed.
- Property-generated legal transitions preserve stage invariants.

### Explicitly excluded

Reaction outcomes remain open opportunities. No claim proof or artifact.

## Slice 6 — claims, holes, and goal generation

### Depends on

Slices 4 and 5.

### Deliverables

- typed claim and expectation aggregation;
- snapshot/cumulative evaluation windows;
- typed holes and stable HoleIds;
- immutable Goal/ProofState types;
- generation of author-requested and mandatory kernel goals;
- explicit/omitted/hole conflict handling;
- result-envelope classification through `Incomplete`.

### Conformance focus

- every claim form and target stage;
- duplicate/conflicting claims;
- hole expected types;
- amount aggregation across locations;
- explicit assumptions and used/unused distinction;
- deterministic goal IDs/dependency ordering.

### Acceptance

- Canonical source produces a golden open-goal graph.
- Omitted claims do not remove mandatory artifact goals.
- No tactic or chemistry rule is required to inspect the goal graph.

### Explicitly excluded

No goal solving, derivations, or validated artifacts.

## Slice 7 — derivation kernel and tactic framework

### Depends on

Slices 1, 3, 5, and 6.

### Deliverables

- `chem-kernel` trusted rule/checker core;
- content-addressed derivation nodes and DAG replay;
- exact equation normalization and balance solving;
- atom/charge conservation and stoichiometric extent;
- tactic dispatcher and proof-state transitions;
- semantics for balance, derive prerequisites, cancel, solve, verify, and close;
- private artifact builder that remains unreachable until all fields are proved.

### Conformance focus

- balanced/unbalanced equations;
- exact coefficient normalization;
- charge and atom mismatches;
- limiting/extents/residuals;
- changed-node/premise replay rejection;
- open, solved, disproved, and unsupported goals.

### Acceptance

- The checker rejects every mutated golden derivation.
- Mandatory conservation cannot be bypassed by omitted tactics.
- Tactic traces alone cannot construct trusted values.
- No reaction-family search exists yet.

### Explicitly excluded

No precipitation, neutralization, gas, or no-reaction inference.

## Slice 8 — canonical precipitation vertical slice

### Depends on

Slice 7.

### Deliverables

- supported aqueous dissociation tactic;
- precipitation candidate enumeration and rule premises;
- deterministic closure/confluence handling;
- molecular, complete ionic, and net ionic derivation;
- precipitation observations;
- complete canonical proof and bounded `auto` path;
- first privately constructed `ValidatedExperiment`.

### Acceptance

The silver nitrate/sodium chloride source completes:

```text
source
  -> typed HIR
  -> stages/opportunity
  -> goals/tactics
  -> checked derivation
  -> ValidatedExperiment
```

Additionally:

- a wrong equation is `Invalid`;
- unknown supported-looking chemistry is `Unsupported`;
- omitted tactics leave `Incomplete` rather than weakening checks;
- every empirical conclusion resolves to evidence provenance;
- a source edit or catalogue digest change stales the artifact.

### Explicitly excluded

No other reaction family.

## Slice 9 — reaction breadth and positive no-reaction proof

### Depends on

Slice 8.

### Deliverables

- strong acid/base neutralization family;
- curated acid/carbonate gas-formation family;
- rule-domain coverage declarations and no-reaction closure;
- gas/closure premise handling;
- all four independently reviewed canonical fixtures;
- multi-family confluence/ambiguity behavior.

### Acceptance

- All four canonical experiments produce their reviewed artifacts.
- Absence of candidates without coverage is `Unsupported`.
- No-reaction succeeds only with complete coverage.
- Competing non-confluent outcomes are `Unsupported` regardless of rule order.

### Explicitly excluded

Weak acid/base equilibria, redox, kinetics, organic chemistry, and arbitrary gas
patterns.

## Slice 10 — language service, diagnostics, and CLI

### Depends on

Slices 2, 4, 6, and 9.

### Deliverables

- complete compiler result envelope and diagnostic precedence;
- all stable diagnostic namespaces and structured safe fixes;
- source-version/catalogue-digest incremental API;
- semantic tokens, completion, hover, goals, and provenance mappings;
- `check`, `ast`, `hir`, `goals`, `derive`, `artifact`, and `format` CLI paths;
- stale asynchronous result rejection.

### Acceptance

- CLI output and exit codes have golden tests for every result state.
- Fix edits reject stale digests and cannot silently add assumptions.
- Editor requests never return values for the wrong source/catalogue version.
- Normal test execution consumes no network or model usage.

## Slice 11 — artifact and simulation boundary

### Depends on

Slices 8 through 10.

### Deliverables

- stable artifact schema and canonical serializer;
- checked deserializer with derivation replay;
- artifact content digest;
- renderer-independent conversion to simulation stages/frames;
- source/claim/derivation/simulation linking IDs;
- invalid/stale source gate.

### Acceptance

- Round-trip artifact replay succeeds only for unchanged canonical content.
- Mutation of any chemistry field, premise, assumption, digest, or derivation
  node is rejected.
- Simulation can be constructed only from the private checked artifact type.
- The canonical stage timeline produces deterministic simulation input.

## Slice 12 — fuzzing, hardening, and conformance closure

### Depends on

All prior slices.

### Deliverables

- requirement-to-test coverage report;
- full fuzz/property/metamorphic suites;
- resource-limit and denial-of-service hardening;
- complete licence and unsafe-code audit;
- normative conformance manifest and golden artifacts;
- implementation/spec discrepancy audit.

### Acceptance

- Every normative requirement ID has at least one conformance case.
- All suite categories report complete coverage.
- Fuzzing finds no panic or unchecked artifact construction.
- Known unsupported domains remain explicit and documented.
- The implementation claims `.chems` conformance only after this gate.

## Integration gates

| Gate | After slice | Demonstration |
| --- | ---: | --- |
| `G0` Specification executable | 0 | Requirement/grammar/conformance validation in CI |
| `G1` Source and types | 4 | Canonical source to typed HIR with exact quantities/catalogue identities |
| `G2` Experiment state | 6 | Canonical source to immutable stages and open proof goals |
| `G3` First trusted chemistry | 8 | Silver chloride source to replayable validated artifact |
| `G4` Initial domain complete | 9 | Four reaction-family fixtures and positive no-reaction proof |
| `G5` Product-ready language | 11 | Checked artifact drives deterministic simulation input |
| `G6` `.chems` conformance | 12 | Full coverage, fuzzing, and hardening evidence |

No gate is passed by compilation alone.

## Per-slice definition of done

Every slice requires:

- specification requirement IDs selected;
- failing conformance fixtures added before behavior;
- independently reviewed expected chemistry where applicable;
- implementation limited to the slice boundary;
- unit, integration, property, and golden tests appropriate to risk;
- formatting and strict linting clean;
- no unsafe Rust;
- exact pass/fail commands recorded;
- diagnostics and public schemas documented;
- no invalid/unsupported/incomplete path to a validated artifact;
- review by the owner of the next consuming boundary.

## Change control

When implementation reveals a specification problem:

1. Stop the affected slice.
2. Write the smallest concrete counterexample.
3. Identify affected requirement IDs and compatibility consequences.
4. Amend the specification and normative grammar if required.
5. Add/update conformance cases.
6. Resume implementation against the reviewed decision.

Do not make the code's current behavior normative after the fact.

## Immediate next action

Begin Slice 5 against the completed typed HIR. Add immutable initial stages and
inventory ledgers, validate prepared-material initial state, execute the closed
procedure operation set exactly, enforce vessel/location/capacity/closure and
condition invariants, and create reaction opportunities without beginning
reaction inference, claims, goals, tactics, or artifact construction.
