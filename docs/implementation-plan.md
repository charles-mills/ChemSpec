# Build Week implementation plan

## Purpose

This is the issue-level execution plan for ChemSpec. The
[delivery plan](delivery-plan.md) defines milestones and product priorities;
this document turns them into bounded tasks with dependencies, acceptance
criteria, integration gates, and fallbacks.

Detailed `.chems` language work is governed separately by the
[language specification](chems-specification.md) and
[language implementation plan](chems-implementation-plan.md). The Build Week task
table should reference those slices rather than inventing language semantics.

The submission deadline is **Tuesday, 21 July 2026 at 17:00 PDT**. The plan
therefore optimizes for one complete, trustworthy demonstration rather than
independent subsystem completeness.

## Target outcome

The release candidate must complete this loop from a packaged desktop build:

```text
request
  -> visible research and evidence
  -> generated .chems
  -> deterministic validation
  -> derivation and assumptions
  -> explanatory particle simulation
```

The canonical experiment is mixing 50 mL of 0.100 M silver nitrate with 50 mL
of 0.100 M sodium chloride to form silver chloride precipitate. Every layer is
integrated against this example before reaction breadth is added.

## Ownership

The four team members each own a workstream:

| Code | Workstream | Primary authority |
| --- | --- | --- |
| `L` | Language and kernel | `.chems`, parsing, diagnostics, validation plumbing |
| `C` | Chemistry and catalogue | Facts, rules, stoichiometry, scientific review |
| `U` | Application and simulation | Iced UX, simulation model, rendering, packaging |
| `A` | Agent and providers | Codex, Responses API, research, provenance, repair |

Ownership means resolving decisions and keeping the workstream integrable. It
does not prevent pairing. Changes to a shared contract require the owner of the
affected producer and consumer to review them.

## Priority and cut line

### P0 — required for submission

- canonical request-to-simulation journey;
- visible `.chems` source and validation diagnostics;
- deterministic derivation, assumptions, and unsupported states;
- visible research workflow and source provenance;
- startup choice between Codex subscription and API key;
- functional Codex CLI and direct Responses API providers;
- bounded validation-repair loop;
- local safety scope gate and transparent redirects;
- packaged macOS build and build evidence for Windows and Linux;
- repeatable demo and submission materials.

### P1 — complete after the canonical slice

- one reviewed fixture for neutralization, gas formation, and no reaction;
- stronger source editing and diagnostic navigation;
- educational explanations for all four supported reaction families;
- deterministic agent evaluation corpus;
- platform packages beyond the primary recording machine.

### Cut order

If the schedule slips, cut work in this order:

1. custom wgpu shaders in favour of Iced canvas or mesh primitives;
2. advanced editor behaviour in favour of a plain editable source view;
3. secondary-platform installers while retaining CI build evidence;
4. decorative animation and theme polish;
5. catalogue or reaction breadth beyond the four reviewed fixtures.

Never cut the validation boundary, visible source, provenance, provider
selector, canonical simulation, or honest invalid/unsupported states.

## Gate 0 decisions

These contracts must be frozen before parser and integration work begins. Each
decision is recorded in the relevant design document and represented by a
fixture or shared type.

### `.chems` surface

Lock:

- complete EBNF for declarations and experiment blocks;
- whether indentation is syntactic and how tabs are handled;
- comments, identifiers, aliases, formulae, charge, and phase notation;
- numeric syntax and the supported unit set;
- mandatory and optional experiment sections;
- the exact `expect` claims that the validator checks independently;
- the bounded set of `by` tactics;
- stable diagnostic codes, severities, source spans, and related spans;
- canonical formatting and parse/format/parse expectations.

The locked decisions are defined only by the
[language specification](chems-specification.md),
[normative grammar](../grammar/chems.ebnf), and executable conformance registry.
The canonical silver-chloride fixture uses two inputs, but the language permits
one or more material declarations and the complete closed unit registry from the
specification. Source keywords are ASCII while UTF-8 comments and retained
source text remain valid. Parser work begins only after the corresponding
fixture set is reviewed by the language and chemistry owners.

### Shared data contracts

Define exact Rust types and serializable fixture schemas for:

```text
ParsedExperiment
ValidatedExperiment
Diagnostic
Derivation
ResearchResult
EvidenceClaim
AgentEvent
SimulationFrame
```

The contract must encode the product states `Validated`,
`ValidatedWithAssumptions`, `Unsupported`, and `Invalid`. Only `chem-engine`
may construct a `ValidatedExperiment` through its public API.

### Canonical integration artifacts

Create and review these files first:

```text
fixtures/silver-chloride.chems
fixtures/silver-chloride.validated.json
fixtures/silver-chloride.research.json
fixtures/silver-chloride.frames.json
```

Expected JSON is hand-reviewed, not generated by the implementation under
test. Schema changes update all affected fixtures in the same pull request.

### Initial catalogue boundary

The first catalogue contains only the facts needed by the four product
fixtures. Every proof-relevant fact has:

- a stable identifier;
- normalized substance identity and aliases;
- formula, charge, and phase data;
- conditions under which the fact holds;
- a source reference and retrieval metadata;
- a version and deterministic catalogue digest.

Agent research cannot add or alter trusted catalogue facts during a run.

## Toolchain and delivery policy

- Pin the Rust toolchain and dependency versions in the repository.
- Use a Cargo workspace matching the crate boundaries in
  [system-architecture.md](system-architecture.md).
- CI runs formatting checks, compilation, unit/integration tests, fixture
  checks, and licence checks where practical.
- Normal tests use fake providers and never consume subscription or API usage.
- Live provider checks are explicit, opt-in smoke tests.
- macOS is the fully rehearsed packaging target. Windows and Linux remain
  supported native targets with CI evidence; packages are P1 if runners or
  signing become a bottleneck.
- Use short-lived branches named `buildweek/<task-id>-<slug>`.
- Keep pull requests small enough for one adjacent workstream owner to review
  the changed contract and fixture.
- Merge shared types and fixtures early. Do not maintain parallel incompatible
  copies in different crates.

## Feasibility spikes

Spikes are timeboxed evidence-gathering tasks. They produce a small checked-in
prototype or decision note, not production abstractions.

| ID | Owner | Question and acceptance | Cap |
| --- | --- | --- | --- |
| `SP-001` | A | Can `codex exec` emit visible JSONL progress and schema-conforming research, citations, and `.chems`? Record the exact invocation, observed event shapes, cancellation behaviour, and failure state. | 3 h |
| `SP-002` | A | Can the Responses API produce the same `ResearchResult` schema with web evidence? Store a redacted response fixture and note usage/error handling. | 2 h |
| `SP-003` | U | Can an Iced render surface consume `SimulationFrame` without creating a second device or event loop? Render the canonical initial state at interactive speed. | 3 h |

If a spike exceeds its cap, take the fallback immediately:

- normalize coarse Codex events instead of mirroring every provider event;
- preserve the common result schema even if provider progress granularity
  differs;
- use Iced canvas/mesh primitives before custom wgpu rendering.

## Task graph

Timeboxes are caps for a first integrated result, not estimates to fill. Split
or simplify a task that exceeds its cap. A task is complete only when its
acceptance statement is demonstrably true.

### Phase 1 — contracts and workspace

| ID | Owner | Depends on | Deliverable and acceptance | Cap |
| --- | --- | --- | --- | --- |
| `F-001` | L | — | Create the pinned Cargo workspace, crate boundaries, formatting/lint policy, and CI baseline. A no-op app and all crates compile in CI. | 3 h |
| `F-002` | L | `F-001` | Define the shared domain types and result-state boundaries. Compile-time visibility prevents ordinary external construction of `ValidatedExperiment`. | 4 h |
| `F-003` | L+C | `F-002` | Map the locked `.chems` grammar and diagnostic schema to valid/invalid language fixtures. Both owners approve the canonical source. | 4 h |
| `F-004` | C | `F-002` | Hand-author the canonical validated result, catalogue facts, derivation, and source references. Chemistry review signs off mole counts, equation, limiting reagent, and spectators. | 4 h |
| `F-005` | A | `F-002` | Freeze `ResearchResult`, `EvidenceClaim`, and `AgentEvent`; provide a fake-provider research fixture that maps claims to sources. | 3 h |
| `F-006` | U | `F-002` | Freeze `SimulationFrame` and hand-author canonical initial/final frame fixtures. App and simulation consume the same schema. | 3 h |

Run `SP-001`, `SP-002`, and `SP-003` in parallel with Phase 1 after `F-002`
has exposed the minimum shared shapes.

### Phase 2 — offline vertical slice

| ID | Owner | Depends on | Deliverable and acceptance | Cap |
| --- | --- | --- | --- | --- |
| `L-101` | L | `F-003` | Lexer/parser with recovery, source spans, and typed syntax tree. Canonical and invalid fixtures produce their expected structures and diagnostics. | 8 h |
| `L-102` | L | `L-101` | Canonical formatter and stable diagnostic rendering. Parse/format/parse preserves meaning. | 5 h |
| `C-101` | C | `F-004` | Versioned catalogue loader and canonical substances/facts. Invalid, missing, or unproven facts cannot enter validation. | 5 h |
| `C-102` | C | `C-101`, `L-101` | Precipitation inference, equation normalization, conservation, stoichiometry, limiting reagent, spectators, and derivation. Output matches the independently reviewed fixture. | 8 h |
| `U-101` | U | `F-006` | Iced application shell loads static fixtures and shows request, workflow, source, result, evidence, and simulation regions. | 6 h |
| `U-106` | U | `U-101` | Make the reaction builder the primary entry point and add a responsive curated periodic table. Pointer hover, selected, and drag-preview states remain presentation-only and confer no chemistry meaning. | 6 h |
| `U-107` | U | `U-106` | Add the responsive reaction workspace. Desktop keeps the element library and reaction box visible together; direct drag/drop accepts duplicate instances, supports repositioning and removal, smoothly snaps nearby supported groupings, leaves unsupported groupings separate, and labels recognised compositions as untrusted previews pending validation. | 8 h |
| `U-108` | U | `U-107` | Recompose the builder with the reaction box above a full-width, no-horizontal-scroll periodic table. Render library drags on an application-level plane, and present recognised composition previews as single draggable compound objects while retaining member atom identities. | 5 h |
| `U-109` | U | `U-108` | Add deterministic Stage 3 Canvas diagrams: loose atoms show their shell count with slowly animated outer-shell electrons, recognised composition previews retain their member shell models inside one grouped atomic surface, and reduced motion freezes the presentation without changing composition state. | 6 h |
| `U-110` | U | `U-109` | Fit the complete builder onto one non-scrolling page and add the Stage 4 reaction trigger. A structured candidate catalogue drives ready, unsupported, queued, and disabled states; triggering cannot construct products, confer validation, or start simulation. | 5 h |
| `U-111` | U | `U-110` | Add a deterministic Stage 5 2D reaction storyboard preview. Structured candidate data defines balanced representative reactants, every product, and four explanatory stages; controls support pause, restart, skip, and return. The preview never emits `ValidatedExperiment` or `SimulationFrame`. | 7 h |
| `U-112` | U | `U-111` | Rework Stage 1 as a structured two-reactant composer. A complete 118-element, square-tile periodic table feeds one explicitly active reactant draft at a time; formulae build progressively, recognised and unresolved drafts remain untrusted previews, and continuing deterministically seeds the existing Stage 2 workspace without conferring validation. | 8 h |
| `U-113` | U | `U-112` | Consolidate the builder flow. Keep the periodic table full-width while visually separating its s, d, and p blocks; reuse the deterministic orbiting-electron and shared-pair canvas in the active Stage 1 model; and launch a supported illustrative reaction sequence directly from the composer without exposing the intermediate manipulation workspace. | 5 h |
| `U-102` | U | `U-101`, `L-101` | Editable `.chems` source and diagnostics with spans. Any edit invalidates the previously validated result until revalidation succeeds. | 5 h |
| `U-103` | U | `F-004`, `F-006` | Deterministic renderer-independent simulation model for the canonical reaction. Particle counts preserve ratios, excess, and spectator identities. | 6 h |
| `U-104` | U | `SP-003`, `U-101`, `U-103` | 2D particle presentation shows dissolved ions, mixing, precipitate formation, pause/restart, and explanatory labels. | 6 h |
| `I-101` | L+C+U | `L-102`, `C-102`, `U-102`, `U-104` | Wire source → parser → validator → simulation. The canonical fixture runs offline; one deliberate source error blocks playback and points to the correct span. | 5 h |

### Phase 3 — agent vertical slice

| ID | Owner | Depends on | Deliverable and acceptance | Cap |
| --- | --- | --- | --- | --- |
| `A-101` | A | `F-005` | Provider trait, fake provider, timeout, cancellation, and provider-neutral event stream. App tests can exercise the full lifecycle without network use. | 4 h |
| `A-102` | A | `SP-001`, `A-101` | Codex CLI discovery, preflight, invocation, event normalization, structured result, and explicit auth/process errors. A canonical live smoke test succeeds or reaches the correct failure state. | 7 h |
| `A-103` | A | `SP-002`, `A-101` | Direct API-key provider with the same result/event contracts, secret handling, web evidence, usage, cancellation, and clear API errors. | 6 h |
| `A-104` | A | `A-102`, `A-103` | Persist provenance separately from source and link every displayed empirical claim to evidence. Provider switching preserves the current experiment. | 4 h |
| `A-105` | A+L | `A-102`, `A-103`, `L-102`, `C-102` | Bounded repair protocol returns structured diagnostics to the provider and accepts at most three revisions. It cannot weaken or bypass validation. | 5 h |
| `U-105` | U | `U-101`, `A-101` | Startup detects Codex and presents “Use Codex subscription” or “Use API key”; workflow events, cancellation, retry, and failures remain visible. | 5 h |
| `I-201` | A+L+C+U | `I-101`, `A-104`, `A-105`, `U-105` | Both providers drive the canonical end-to-end journey or fail honestly at the correct boundary. Codex is the primary recorded path. | 5 h |

### Phase 4 — chemistry breadth, safety, and teaching value

| ID | Owner | Depends on | Deliverable and acceptance | Cap |
| --- | --- | --- | --- | --- |
| `C-201` | C | `C-102` | Add independently reviewed fixtures and catalogue evidence for strong acid/base neutralization, acid/carbonate gas formation, and no net reaction. | 8 h |
| `L-201` | L | `L-102`, `C-201` | Complete only the syntax and diagnostics required by the three added fixtures. All four sources round-trip. | 4 h |
| `U-201` | U | `U-104`, `C-201` | Add the minimum distinct visual behaviour and explanations for neutralization, gas, and no-reaction results. | 6 h |
| `A-201` | A | `A-101` | Implement the local scope/safety gate, provider safety instructions, transparent refusal/redirect events, and adversarial tests. | 5 h |
| `I-301` | L+C+U+A | `C-201`, `L-201`, `U-201`, `A-201` | All four reviewed fixtures reach their expected state; unsupported, invalid, and redirected requests remain visibly distinct. | 5 h |

### Phase 5 — release and submission

| ID | Owner | Depends on | Deliverable and acceptance | Cap |
| --- | --- | --- | --- | --- |
| `R-101` | All | `I-301` | Run the verification matrix, agent corpus, stale-result tests, deterministic simulation tests, and licence review. Record exact pass/fail evidence. | 6 h |
| `R-102` | U | `I-301` | Produce and smoke-test the macOS package; retain Windows/Linux CI build evidence and document any untested boundary. | 5 h |
| `R-103` | All | `R-101`, `R-102` | Rehearse from a clean install with both provider choices and no developer state. Complete the canonical path twice without manual repair. | 3 h |
| `R-104` | All | `R-103` | Finalize README setup/sample data, public repository access/licensing, submission copy, required Codex session identifier, and a public demo video under three minutes. | 5 h |

## Integration gates

### Gate 0 — contracts are usable

- workspace and CI are green;
- common types compile;
- `.chems` grammar and diagnostics are recorded;
- canonical source, research, validated-result, and frame fixtures are
  reviewed;
- all three feasibility spikes have a decision and fallback.

### Gate A — offline trust loop

- canonical `.chems` parses and validates deterministically;
- derivation and assumptions are inspectable;
- simulation consumes only `ValidatedExperiment`;
- a deliberate invalid edit immediately blocks or stales playback;
- no LLM or live network is required.

### Gate B — live authoring loop

- startup detects and explains provider availability;
- Codex and API-key modes share one product event/result contract;
- research, citations, generation, validation, and repair are visible;
- provider errors cannot masquerade as chemistry outcomes;
- the canonical request completes through Codex on the recording machine.

### Gate C — credible education demo

- all four reaction-family fixtures pass independent expected results;
- source cards resolve proof-relevant claims to provenance;
- unsafe, ambiguous, invalid, and unsupported requests have distinct outcomes;
- explanations identify reaction class, limiting reagent, products, phases,
  spectators, and assumptions.

### Gate D — releasable submission

- a packaged clean install completes the canonical path;
- the demo is repeatable and fits under three minutes;
- setup, test instructions, sample data, licensing, and known limits are clear;
- submission fields and required Codex session identifier are ready;
- final upload is complete before the event deadline.

## Dated sequence

This is the default sequence from 13 July. Move work only by preserving the gate
order.

| Date | Primary target | Required result by end of day |
| --- | --- | --- |
| Mon 13 Jul | Contracts and spikes | `F-001`–`F-006` underway; spike commands and fixture shapes agreed |
| Tue 14 Jul | Parser, chemistry, shell, model | Gate 0 complete; every workstream consumes the canonical fixtures |
| Wed 15 Jul | Offline integration | Gate A complete before visual polish |
| Thu 16 Jul | Providers and workflow | Codex and API modes integrated; Gate B candidate |
| Fri 17 Jul | Repair, provenance, safety | Gate B complete; breadth fixtures reviewed |
| Sat 18 Jul | Breadth and teaching UX | Gate C candidate; freeze new features at end of day |
| Sun 19 Jul | Verification and packaging | Gate C complete; release candidate and known-limits list |
| Mon 20 Jul | Clean-install rehearsal | Gate D candidate; record final demo and prepare submission |
| Tue 21 Jul | Buffer and submission | Fix release blockers only; submit well before 17:00 PDT |

## Daily operating rhythm

1. Ten-minute start-of-day gate review: identify the first broken boundary in
   the canonical journey.
2. Each owner posts one concrete deliverable and its task ID.
3. Integrate contract changes as soon as their producer and first consumer
   agree; do not wait for a whole subsystem.
4. Run the offline canonical journey at least twice daily.
5. Run a live-provider smoke test once daily after Gate B without putting it in
   the normal test suite.
6. End each day with a packaged or runnable demo, a short blocker list, and the
   first task for the next morning.

No task may remain “almost done” across two daily reviews without being split,
simplified, or moved below the cut line.

## Definition of done

Every task must satisfy all applicable points:

- its table acceptance statement is demonstrated;
- formatting, compilation, and relevant tests pass;
- fixtures cover success and at least one meaningful failure;
- diagnostics or user-visible failures are explicit;
- changed contracts and design documents agree;
- no unvalidated or stale result can reach simulation;
- an adjacent workstream owner has reviewed boundary changes;
- no secret, API response containing credentials, or paid live call is embedded
  in normal tests;
- limitations discovered during the task are written down rather than hidden.

## Risk register

| Risk or trigger | Response | Owner |
| --- | --- | --- |
| Codex events or structured output differ from assumptions | Normalize the smallest stable event set; keep raw provider details out of app state; preserve explicit failures. | A |
| Codex-hosted research cannot return adequate evidence | Keep Codex as the author/repair provider, use only reviewed catalogue facts for validation, disclose the research limitation, and retain full evidence support in API-key mode. | A+C |
| Provider generation remains unreliable after three repairs | Stop with `Invalid` or `Unsupported`; demo a reviewed request and never synthesize a trusted result. | A+L |
| Custom renderer cannot share Iced's graphics context cleanly | Use Iced canvas or mesh primitives with the renderer-independent model. | U |
| Catalogue review becomes the bottleneck | Freeze at the four canonical fixtures and reject all unreviewed chemistry as unsupported. | C |
| Cross-platform packaging consumes release time | Fully rehearse macOS; retain compile/test evidence and document the exact untested boundaries for Windows/Linux. | U |
| Source or result becomes stale during asynchronous work | Attach generation IDs and catalogue digests; ignore late events and invalidate on every source edit. | L+U+A |
| Demo depends on network variability | Keep the offline Gate A path and reviewed fixtures available; rehearse live Codex immediately before recording. | All |

## Kickoff checklist

- [ ] Assign names to `L`, `C`, `U`, and `A`.
- [ ] Create the task board using the IDs in this plan.
- [ ] Confirm the locked `.chems` decisions and fixtures in `F-003`.
- [ ] Review the four canonical fixture outcomes and their evidence sources.
- [ ] Run all three feasibility spikes.
- [ ] Pin the toolchain and create the workspace.
- [ ] Merge common types and canonical fixture schemas.
- [ ] Pass Gate 0 before widening implementation.
