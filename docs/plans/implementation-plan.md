# Build Week implementation plan

## Purpose

This plan coordinates the product around the definitive structural `.chems 1`
language. Detailed language execution is governed by the fixed seven-slice
[`.chems` implementation plan](../archive/plans/chems-implementation-plan.md).
Product tasks may
consume those contracts but must not invent alternate language semantics.

The submission deadline is **Tuesday, 21 July 2026 at 17:00 PDT**. The canonical
reaction remains the integration gate, not the product ceiling. The dynamic
outcome rebuild, canonical identity system, structural capability ladder, and
breadth corpus target the strongest end-state architecture rather than a
throwaway demo path.

## Target outcome

The release candidate completes this loop from a packaged desktop build:

```text
reaction request
  -> reviewed-rule applicability
  -> visible observation research and evidence
  -> checked typed reaction declaration
  -> optional deterministic .chems 1 authoring/export
  -> deterministic structural expansion
  -> validation derivation
  -> paired observation and structural-change simulation
  -> evidence-linked overview
```

The canonical reaction is lithium with water. Its exact fixture is bound to a
host-selected AI review attestation and exercises molecular covalent, ionic,
metallic, atom-mapping, electron-transfer, observation, and provider boundaries
in one closed integration target. Broad dynamic coverage then proceeds through
`DYN-104`–`DYN-112` without weakening that gate.

## Workstreams

| Code | Workstream | Authority |
| --- | --- | --- |
| `L` | Language and kernel | `.chems`, expansion, diagnostics, structural validation |
| `C` | Chemistry and catalogue | Structures, rules, electron premises, scientific review |
| `U` | Application and simulation | Iced UX, paired rendering, packaging |
| `A` | Agent and providers | Codex/API, research, evidence, repair, overview |

Shared contracts require producer and consumer review. Runtime agent output is
never allowed to promote itself; host trust is an explicit digest-pinned AI
review decision committed with the application.

## P0 submission boundary

- canonical request-to-simulation journey;
- visible concise source where authored, plus checked declaration and expanded
  certificate;
- deterministic derivation and explicit model disclosure;
- visible observation evidence and provenance;
- capability-checked Codex CLI as the default live provider;
- a provider-neutral boundary that may admit a future BYOK/API backup without
  requiring it for the release;
- bounded visible repair loop;
- safe refusal of real-world harmful procedures;
- native packaged demo on the primary platform; and
- honest Invalid, Unsupported, provider-failure, and stale states.

Never cut the trusted validation boundary, atom identity, evidence visibility,
source visibility, model disclosure, or invalid/unsupported distinction.

## Language dependency

Product integration consumes the exact slices below without adding language
slices:

| Slice | Stable product output |
| --- | --- |
| 0 | definitive grammar, specification, requirements, and canonical oracles |
| 1 | structural domain and preliminary frame value shapes |
| 2 | parsed/formatted authored source and syntax diagnostics |
| 3 | reviewed catalogue bundle and lithium/water rule |
| 4 | resolved HIR and inspectable expanded certificate |
| 5 | privately constructed `ValidatedStructuralReaction` and derivation |
| 6 | deterministic paired frame contract and complete conformance |

Mocks may target a frozen output shape before its producing slice completes,
but production integration waits for the reviewed slice.

## Shared contracts

```text
ResolvedRequest
RuleApplicability
EvidencePacket
AgentEvent
ParsedReaction
ExpandedStructuralReaction
Diagnostic
Derivation
ValidatedStructuralReaction
SimulationFrame
```

Every persisted or transmitted contract carries its schema version and relevant
source, catalogue, evidence, certificate, and artifact digests.

## Canonical integration artifacts

```text
fixtures/lithium-water.chems
fixtures/lithium-water.evidence.json
fixtures/lithium-water.expanded.json
fixtures/lithium-water.derivation.json
fixtures/lithium-water.frames.json
```

Expected structural chemistry is authored independently from the implementation
under test, checked by the deterministic conformance suite, reviewed by AI, and
bound to the exact host-pinned attestation digest.

## Agent and provider tasks

### Dynamic coverage (`DYN`)

The end-state dynamic rebuild is governed by the
[dynamic reaction outcome rebuild plan](dynamic-reaction-rebuild-plan.md).

1. **DYN-101 — Codex catalogue-miss vertical.** Recognized misses remain
   actionable; Codex runs with live search, an ephemeral read-only sandbox, and
   a strict compact-plan result schema. A self-contained prompt is embedded, so
   packaged apps require no repository checkout. ChemSpec compiles the returned
   plan into working catalogue, evidence, and source artifacts, which cross
   deterministic validation into `ValidatedReviewCandidateFrames` before
   presentation, without promoting their review-candidate provenance.
   Late completions are rejected by run ID. Explicit low reasoning is used.
   This ledger-heavy contract is superseded by `DYN-104` and remains only until
   the compact claim cutover crosses validation. **Implemented, superseded.**
2. **DYN-102 — Visible workflow and bounded repair.** Stream normalized events,
   expose validation diagnostics, terminate the child on cancellation/timeout,
   and allow at most three compact-plan repair attempts. Three bounded repairs, a
   five-minute total deadline, child termination on deadline, and an elapsed UI
   status are implemented. Event streaming, detailed diagnostics, repair diffs,
   and user cancellation remain. The rebuild replaces full-plan repair with one
   targeted claim/source correction. **Partial, superseded in part.**
3. **DYN-103 — Local cache and regeneration foundation.** Digest-bound
   on-device reuse and explicit regeneration are implemented. `DYN-110`
   replaces the current cache identity and envelope. **Implemented foundation.**
4. **DYN-104 — Compact claim contract.** Codex answers only the factual outcome,
   observations, ambiguity, context, and source locations. **Implemented.**
5. **DYN-105 — Canonical species identity.** Resolve stable species identities,
   graphs, aliases, external identifiers, and ambiguity. **Implemented local
   identity/cache contract and validated external-adapter seam; a concrete
   public resolver and its packaged capability evidence remain optional release
   integration work.**
6. **DYN-106 — Exact outcome compiler.** Balance exactly and converge parsed
   `.chems` and dynamic compilation on checked `ReactionDeclaration`.
   **Implemented.**
7. **DYN-107 — Evidence acquisition.** Fetch, snapshot, and adjudicate
   claim-level external evidence as hostile data. **Implemented.**
8. **DYN-108 — Structural capability ladder.** Choose reviewed-family
   animation, validated model-proposed mechanism, or honest static
   presentation. **Implemented.**
9. **DYN-109 — Progressive application.** Surface identity, research, evidence,
   compilation, mapping, presentation, cancellation, and regeneration states.
   **Implemented progressive result path, disambiguation, stale-result gates,
   child-process cancellation, and transactional regeneration.**
10. **DYN-110 — Cache v3.** Persist and revalidate identity, evidence,
    declaration, and presentation capabilities offline. **Implemented.**
11. **DYN-111 — Codex hardening.** Keep Codex binary as default, explicit low
    reasoning and normal service, one repair, progress events, and measured
    latency budgets. **Implemented hardening, cached capability probes,
    milestone instrumentation, and deterministic percentile/default-selection
    reporting; live model observations remain release evidence.**
12. **DYN-112 — Breadth proof.** Validate at least 250 diverse and adversarial
    requests across factual, identity, structural, and performance boundaries.
    **Implemented 266 unique requests, 76 adversarial mutations, separated
    metrics, and a 25-case all-category ignored live harness; independent oracle
    review, live execution, and three-platform packaging remain release
    evidence.**

The catalogue remains the fast path and cacheable trust root; it is no longer
the product's chemistry ceiling.

1. Freeze provider-neutral events, evidence, source, repair, and overview
   envelopes against the canonical fixture.
2. Implement Codex preflight and capability detection without reading auth
   files.
3. Invoke `codex exec` with read-only sandbox, live search, structured output,
   ignored user configuration/rules, ephemeral state, and cancellation.
4. Implement one targeted claim/source correction after deterministic
   diagnostics.
5. Evaluate identity selection, evidence completeness, source validity,
   unsupported behavior, and provider failures with fake providers.

## Application and simulation tasks

1. Implement startup provider selector and preflight states.
2. Render the visible workflow and claim-level evidence.
3. Provide editable source plus expanded-certificate and derivation inspectors.
4. Consume only `SimulationFrame`; never parse source or infer chemistry.
5. Render covalent, ionic, and metallic relationships distinctly.
6. Synchronize qualitative observations with validated structural steps.
7. Preserve stale-result labeling, cancellation, replay, and deterministic
   restart.

## Chemistry tasks

1. Review atom/electron state and metallic-domain semantics.
2. Author lithium metal, water, hydroxide, lithium hydroxide, and hydrogen
   structures.
3. Author `AlkaliMetalWithWater` applicability, role, product, mapping, and
   operation templates.
4. Review every canonical intermediate state and final graph.
5. Author observation-compatibility premises and evidence review.
6. Add broader reactions only after the canonical vertical path is complete.

Catalogue breadth uses the implemented
[generalized chemistry design](../generalized-chemistry-rules.md): reviewed element
facts derive classifications, parameterized structure applications construct
exact graphs, and typed family rewrites compile into the existing concrete
kernel operations. Its fixed
[G0–G6 implementation queue](../archive/plans/generalized-rules-implementation-plan.md)
is complete. The first generated catalogue now has a host-selected AI review
attestation and both exact digests committed to the trust root.

## Current integration status

- The complete language, catalogue, generalized-rule, kernel, and frame crates
  are integrated with the Iced workspace.
- The promoted catalogue crosses the pinned AI trust attestation and contains
  205 finite experiences across the established generalized families,
  elemental oxygen, fixed-charge main-group ion pairs, and finite covalent
  combinations. The 118-element registry still provides identity metadata,
  not universal reaction coverage.
- `chem-presentation` compiles deterministic guided scenes and a macroscopic
  scene plan from the same trusted `SimulationFrames` generation.
- The guided Canvas renderer consumes exact atoms, covalent/dative edges, ionic
  associations, metallic domains, operations, observations, and product
  membership. The depth-tested wgpu renderer consumes only reusable visual
  profiles and observation-gated effects, never an atom graph.
- Startup capability-probes Codex version, login, live-search support, and every
  non-interactive flag used by the runtime. Recognized catalogue misses show
  **Press space to ask Codex** in the builder's ordinary animated prompt,
  launch a generation-scoped Iced task, and compile a closed claim into an
  exactly balanced static outcome before optional evidence and 2D/3D
  presentation enrichment. The prompt fades while the dynamic modal is open
  and returns if it is dismissed. Dynamic modal visibility is the single
  builder-overlay authority: it suppresses inline status, toolbar and drag
  overlays, and background builder shortcuts until dismissal.
- UI-local hydrogen/oxygen, carbon/oxygen, and silver-chloride outcomes have
  been removed; known outcomes still come through the catalogue fast path.
- Editable source invalidates downstream frames and can be revalidated through
  the trusted boundary. The claim/evidence/mechanism rebuild is implemented;
  live model benchmarking, child-aware user cancellation, external identity
  integration, independent corpus review, and cross-platform release evidence
  remain.

## Catalogue breadth status

The original generalized-family breadth queue is implemented, reviewed, and
pinned. Current finite coverage and its machine-readable authorities are
summarized in [`../catalogue-coverage.md`](../catalogue-coverage.md). Further
content ideas are explicitly deferred under `../backlog/` and are not release
commitments until independently reviewed and promoted through the same
digest-bound trust process.

## Integration gates

### Gate A — contracts

Language Slice 0 is reviewed clean; shared provider and frame schemas consume
the same identities and result states.

### Gate B — structural vertical

Slices 1–5 validate the canonical authored source to the independent derivation
with every negative invariant covered.

### Gate C — paired experience

Slice 6 frames drive the application with fake providers; source editing,
staleness, diagnostics, and synchronized playback behave correctly.

### Gate D — release candidate

One real provider completes the packaged canonical journey; normal tests remain
offline; final formatting, tests, lint, docs, conformance, package smoke checks,
and demo rehearsal pass.

## Fallbacks

- Reduce catalogue breadth before weakening validation.
- Use simple deterministic 2D layout before custom GPU effects.
- Use a plain source editor before advanced language-service UI.
- Retain primary-platform packaging plus CI build evidence if secondary
  installers are delayed.
- Stop honestly at Unsupported when bounded dynamic construction cannot produce
  a valid, safe, representable result.
