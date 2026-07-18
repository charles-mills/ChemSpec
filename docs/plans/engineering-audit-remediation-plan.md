# Engineering audit and remediation plan

- Status: active audit, awaiting slice-by-slice implementation
- Audited: 2026-07-18
- Source revision: `93b5501`

## Purpose

This document audits a third-party engineering report against the current
repository and turns the surviving findings into independently executable
remediation slices. It is an engineering plan, not a change to the chemistry
trust contract. The durable architecture and verification documents remain
authoritative.

The audit uses four verdicts:

- **Confirmed** — current source directly demonstrates the claim.
- **Partly confirmed** — the underlying concern is real, but the report's
  scope, mechanism, severity, or proposed deletion is overstated.
- **Not confirmed** — current source contradicts the claim or does not support
  it as a defect.
- **Recommendation** — a reasonable design direction, but not an observed
  correctness failure by itself.

Line references are anchors for this revision and will move during the work.
Each completed slice must update this document and any governing contract it
changes.

## Executive result

The report is directionally strong, especially on the trust boundary,
delocalized bonds, process termination, duplicated renderer topology, stale
documentation, and oversized modules. It is not 99% literally correct.
Several statements combine a real narrow problem with a broader conclusion
that the source does not justify.

The most important corrections to the report are:

- `TrustedCatalogue` is not vestigial. It is the production capability used
  by the kernel, agent, CLI, and app. Its documentation and constructor no
  longer enforce the attestation semantics promised by the crate docs, which
  is a serious contract mismatch, but deleting the type would remove a live
  boundary rather than dead code.
- The identity adapter/cache entry point is unused outside its tests, but
  `agent::identity` as a whole is not. Reviewed registries, generated species,
  formula inventory, and model-proposed species helpers are production inputs
  to outcome and structure compilation.
- Message routing in the app does not silently ignore new root variants. The
  top-level match is exhaustive and the subrouters use explicit
  `unreachable!` guards. The enum and file are still too large.
- `into_validated_dynamic` does not bypass structural validation. Its input is
  created from a review-candidate derivation that crossed the kernel. A
  separate inspection witness may improve the capability model, but the
  absence of one is a product-review policy question rather than the same
  exploit as mutable public HIR metadata.
- The NUL-delimited dynamic species digest is not shown to be non-injective in
  the accepted domain: the formula parser rejects NUL, making the final
  delimiter recoverable even if a name contains NUL. Length-prefixed or
  structured hashing is still clearer.

## Priority 0 — trust and correctness boundary

### AUD-001 — Make expanded claim consistency structural

Verdict: **Confirmed; critical.**

Evidence:

- `ResolvedReactionClaim` and `ExpandedStructuralReaction` expose ordinary
  public fields in `crates/chem-kernel/src/hir.rs:244` and `:260`.
- `validate` in `crates/chem-kernel/src/validate.rs:467` validates identity,
  operation sequence, mapping, valence, conservation, and final products, but
  does not re-establish that `claim.equation`, the claim-side formula maps, the
  declaration, or resolved observations agree with the expanded instances.
- `compare_final_bonds` and related checks establish graph equality, not the
  independent display metadata retained in the claim.

Impact: another crate can clone or construct structurally valid HIR, alter
claim-level learner-facing metadata, and still receive a successful
derivation. This violates the documented promise that the kernel checks the
equation and observations and makes “validated structure, fictional display”
representable.

Required outcome:

- Make proof-relevant HIR fields private or construction-restricted.
- Establish claim/equation/formula/observation consistency in the constructor
  that creates the expanded value, with no public mutation path afterward.
- Add adversarial tests that mutate every learner-facing claim surface and
  prove it cannot reach validated frames.
- Update `docs/system-architecture.md` and `docs/chemistry-engine.md` if the
  ownership boundary changes.

### AUD-002 — Separate provider and solver no-reaction claims

Verdict: **Confirmed; high.**

Evidence:

- `ReactionClaim::no_reaction_reason` is deserializable with
  `#[serde(default)]` in `crates/agent/src/claim.rs:41` despite its comment
  saying it is solver-only.
- `ReactionClaim::from_json` accepts it for a `NoReaction` disposition, and
  `no_reaction_reason_only_rides_a_no_reaction` explicitly tests acceptance.
- `crates/chemspec-app/src/main.rs:3928` renders
  `NoReactionReason::learner_explanation` as trusted explanatory copy.
- `compile_claim_outcome` assigns `TrustTier::ModelAsserted` to every dynamic
  compiled claim at `crates/agent/src/outcome.rs:272`, including deterministic
  solver output. The UI locally relabels that tier as “DERIVED,” but provenance
  is still not structural in the underlying capability.

Required outcome:

- Reject this field at every provider/cache decoding boundary, or split the
  wire type from the algorithmic solver type so the provider cannot express
  it.
- Give deterministic solver outcomes a distinct typed provenance instead of
  routing them through a model-asserted value and correcting the label in the
  app.
- Preserve cache versioning and add hostile JSON tests.
- Prefer the broader `ProviderClaim` / `SolvedClaim` split when it can be done
  without combining unrelated work.

### AUD-003 — Make dynamic frame promotion explicit

Verdict: **Partly confirmed; medium design hardening.**

Evidence:

- `CandidateDynamicFrames::into_validated_dynamic` at
  `crates/chem-kernel/src/frames.rs:471` is an unconditional capability
  conversion.
- Its only production caller, `crates/agent/src/mechanism.rs:625`, first calls
  `validate_review_candidate` and `inspect_review_candidate_frames`; therefore
  structural validation is not skipped.
- The durable architecture explicitly allows deterministically validated
  review-candidate frames to remain renderer-readable while retaining
  review-candidate provenance.

Required decision:

- Decide whether “inspection” means deterministic projection only or an
  additional host/product approval event.
- If approval is required, introduce an unforgeable inspection/approval
  witness consumed by the conversion. If not, rename the types and method so
  they describe the actual validated-but-unreviewed capability without
  implying a manual inspection.

### AUD-004 — Terminate Unix process groups without `/bin/kill`

Verdict: **Confirmed; critical on NixOS.**

Status: **Completed 2026-07-18 in the working tree (uncommitted).**

Evidence:

- `terminate_child_tree` in `crates/agent/src/codex.rs:879` invokes the literal
  path `/bin/kill` and discards both failures.
- `/bin/kill` is absent in the audited NixOS environment.
- After the failed signal, the function joins pipe-reader threads. A surviving
  descendant can retain stdout/stderr and make the 30/180-second caller-owned
  deadline unbounded.

Required outcome:

- Signal the process group with a direct, portable Unix API at the narrow
  platform adapter boundary; do not add `unsafe` to workspace crates.
- Preserve Windows tree termination.
- Add a test process that forks a descendant holding both pipes and prove
  cancellation and timeout return within a bounded allowance.

### AUD-005 — Compare and preserve covalent delocalization

Verdict: **Confirmed; high.**

Status: **Completed 2026-07-18 in the working tree (uncommitted).**

Evidence:

- `compare_final_bonds` at `crates/chem-kernel/src/validate.rs:1421` compares
  endpoints, order, and dative origin, but omits `CovalentDelocalization`.
- `WorkingState::apply` handles an ordinary `ChangeCovalent` by rebuilding the
  bond with `CovalentBond::new` at `crates/chem-kernel/src/validate.rs:825`,
  which drops any prior delocalization metadata.
- A separate `ChangeCovalentDelocalization` operation exists, so
  delocalization is part of the semantic graph, not renderer decoration.
- Execution found two additional loss points: `StructureInstance::instantiate`
  rebuilt shared edges without their annotations, and the algorithmic
  graph-diff mechanism neither represented nor emitted delocalization changes.

Required outcome:

- Include delocalization identity in final graph equality.
- Make every operation either preserve it deliberately or change it through
  the typed delocalization operation.
- Add negative validation tests and frame regression cases for resonance
  systems.

### AUD-006 — Do not feed provider failures into validation repair

Verdict: **Confirmed; high.**

Status: **Completed 2026-07-18 in the working tree (uncommitted).**

Evidence: `propose_with_provider` in
`crates/agent/src/mechanism.rs:428` converts every provider error to a string,
stores it as the next diagnostic, and continues the bounded repair loop.
Cancellation, timeout, authentication, and transport failures are therefore
treated like model output rejected by local validation.

Required outcome:

- Retry only typed, repairable provider-output or kernel-validation failures.
- Return cancellation, timeout, unavailable-provider, and other operational
  failures immediately with their original typed kind.
- Test call counts and diagnostic contents for every error class.

### AUD-007 — Bind structure proposal requests to exact missing species

Verdict: **Confirmed; high.**

Evidence:

- `adopt_proposed_structures` at `crates/agent/src/structure.rs:197` checks only
  that the number of missing outcome species equals the number requested.
- Response IDs and formulas are checked against the supplied request, but the
  request's ordered IDs, names, and formulas are not checked against the
  outcome's actual missing species before the two lists are zipped at `:263`.
- A same-length substituted request can therefore validate a graph against
  one formula and attach it to an outcome species carrying another formula.

Required outcome:

- Derive a canonical expected request from the outcome and compare the full
  ordered content, or make the request an opaque capability created only from
  that outcome.
- Add same-count/wrong-content tests for ID, name, formula, side, and order.

### AUD-008 — Enforce text limits in Rust, not only JSON Schema

Verdict: **Confirmed with scope correction; medium.**

Evidence:

- The Codex output schema in `crates/agent/src/codex.rs` contains useful
  per-field maximum lengths.
- `ReactionClaim::from_json` enforces a 64 KiB document limit, but
  `require_text` at `crates/agent/src/claim.rs:392` checks only non-emptiness.
- Cache bytes, tests, alternate providers, and ordinary callers of
  `from_json` do not receive JSON Schema enforcement.

Required outcome: mirror collection and scalar bounds in the Rust wire
validator and test boundary values. Keep the whole-document byte cap as a
second defense.

### AUD-009 — Use structured inputs for generated identity digests

Verdict: **Not confirmed as a collision bug; low hardening.**

Evidence: `crates/agent/src/outcome.rs:231`, `:612`, and `:688` hash
`name + NUL + formula`. The accepted formula grammar excludes NUL, so the last
delimiter uniquely separates the two fields even if a display name contains
NUL. No collision in the accepted domain was demonstrated.

Recommendation: hash canonical serialization or length-prefixed fields to
make injectivity obvious and future-proof, with a cache/version migration if
the resulting IDs are persisted.

## Priority 1 — typed interior surfaces

### AUD-010 — Replace CST node-kind strings

Verdict: **Confirmed.**

`SyntaxNode.kind` and diagnostic `node_kind` are `String` in
`crates/chems-lang/src/syntax.rs:65` and `:97`; parser classification uses
`ends_with("-section")` at `crates/chems-lang/src/parser.rs:1458`. Introduce a
closed `NodeKind` enum while preserving stable serialized/diagnostic spelling
where it is a compatibility surface.

### AUD-011 — Parse catalogue site references once

Verdict: **Partly confirmed.**

References such as `role[1].atom` are raw strings across catalogue records,
and `parse_instance_site` splits them at runtime in
`crates/chem-catalogue/src/generalized_elaboration.rs:1353`. This is a latent
correctness and maintenance risk. The report's specific “substring surgery”
claim is inaccurate at the audited revision: canonicalization rewrites exact
keys through `transform_reference` at `:1068`, not substrings.

Required outcome: introduce a parsed `SiteRef`/`InstanceRef` type at the
validated model boundary and use typed transformations thereafter.

### AUD-012 — Replace semantic JSON key sniffing

Verdict: **Confirmed.**

`strip_physical_source_identity` at `crates/chem-kernel/src/hir.rs:540`
recursively changes digest semantics based on generic JSON object shape and
field names. Any future object containing both digest keys, or the selected
top-level key pattern, can be silently stripped. Replace it with explicit
serializable semantic-view structs and golden tests proving which fields do
and do not affect the digest. Apply the same principle to the AST semantic
digest pass in `elaborate.rs:2020`.

### AUD-013 — Remove avoidable lexer and parser copying

Verdict: **Confirmed; performance impact not yet measured.**

- Every real token owns `source[start..end].to_owned()` in
  `crates/chems-lang/src/lexer.rs:369`.
- Every completed syntax node is deeply cloned into its parent in
  `crates/chems-lang/src/parser.rs:1022`.

The clone is structurally capable of quadratic copying; calling it the one
real performance bug requires a benchmark. First add representative large
source benchmarks, then use source spans/interned text and arena/indexed or
single-ownership CST construction without changing losslessness.

## Priority 2 — one source of truth

### AUD-014 — Extract shared ring topology

Verdict: **Confirmed; high maintenance risk.**

2D and 3D contain parallel implementations of 2-core pruning, shortest-cycle
selection, fused rings, spiro systems, and bridged bicyclics around
`crates/chemspec-app/src/structural_2d.rs:1431-1815` and
`structural_3d.rs:2090-2490`. Extract renderer-independent topology before
splitting either renderer, with shared conformance cases consumed by both.

### AUD-015 — Consolidate periodic facts and element presentation policy

Verdict: **Confirmed with ownership caveat.**

- `chem_domain::periodic` already owns 118 symbols, names, and valence
  electrons.
- `crates/chemspec-app/src/elements.rs` repeats them with title casing,
  categories, and string atomic masses, then reparses mass to `f32` for 2D
  physics at `structural_2d.rs:393`.
- Element colors are independently selected in 3D, 2D, particle cards, and
  product summary. Sodium is Jmol purple in 3D and theme sodium in the other
  views.
- Standard-state multiplicities are repeated in app naming/input logic and
  agent naming, while domain generation separately encodes P4/As4 and S8
  topology.

Required decision: put stable physical facts and exact masses in a domain
periodic-data type; keep theme policy outside the domain but expose one app
palette service used by every renderer. Define one standard-state API consumed
by app, agent, and generator. Do not put Iced `Color` in `chem-domain`.

### AUD-016 — Unify presentation compilation deliberately

Verdict: **Partly confirmed.**

`chem-presentation::compile_phase_driven_profile` and
`chemspec-app::chemistry::presentation_profile` duplicate object/effect/camera
construction and several literals. However, current durable docs explicitly
permit host-selected exact experience styling in addition to the generic
phase-driven compiler. The defect is duplicated assembly, not necessarily the
existence of two policies.

Required outcome: move shared primitives and assembly to
`chem-presentation`; represent exact experience differences as typed data or
small overrides, and preserve the host-selected/generic distinction.

### AUD-017 — Derive every displayed validated equation from the declaration

Verdict: **Partly confirmed; high wherever validated voice is used.**

The app stores static equation strings and generated `.chems` templates in
`crates/chemspec-app/src/chemistry.rs`, while a checked
`ReactionDeclaration` also exists. Structural playback already formats the
validated declaration in `main.rs:3340`, so the report overstates the current
drift across all views. Builder, profile, and summary paths still use
`ReactionRequest::equation` in several places.

Required outcome: classify pre-validation preview copy separately; after
validation, derive all equation display and profile metadata from the current
declaration. Add deliberate mismatch tests.

### AUD-018 — Merge kernel expansion assembly

Verdict: **Confirmed.**

`expand_typed_declaration` and source `expand` independently assemble
instances, mapping provenance, operation provenance, premises, claim, and the
final HIR in `crates/chem-kernel/src/elaborate.rs:327-437` and `:550-666`.
They have diverged: the source path enriches each atom's provenance with an
“expanded atom” catalogue origin, while the typed path copies only instance
provenance. Equivalent chemistry can therefore produce different semantic
certificates depending on entry path.

Required outcome: one shared assembly function after the two frontends have
resolved their distinct inputs; differential tests must compare semantic HIR,
digests, validation, and frames.

### AUD-019 — Reconcile solver families with catalogue rules

Verdict: **Confirmed as duplicated knowledge; recommendation on mechanism.**

`crates/agent/src/solve.rs` implements many reaction-family decisions in Rust
while reviewed generalized and concrete catalogue rules encode overlapping
families. No systematic differential gate was found. Decide per family whether
the solver is an applicability algorithm feeding catalogue compilation or a
second rule source. Prefer data where expressive enough; otherwise add a
catalogue-wide agreement suite for overlapping closed-world cases and honest
declines.

### AUD-020 — Generate or mechanically diff the public catalogue schema

Verdict: **Confirmed with scope correction.**

`schemas/chem-catalogue-1.schema.json` is 46,831 bytes and is manually
maintained beside serde models. Tests validate multiple fixtures against it,
not merely one spot check, but no test proves the schema and Rust model accept
the same domain. Add generated-schema comparison or a purpose-built contract
diff in CI. Do not accept generated output as the chemistry oracle; this gate
checks wire shape only.

## Priority 3 — capability and speculative-code audit

### AUD-021 — Narrow or remove the unused identity adapter/cache entry point

Verdict: **Partly confirmed.**

`SpeciesIdentityAdapter`, `resolve_species_identity`, and the identity-cache
load/store orchestration in `crates/agent/src/identity.rs:47` and `:393-675`
have no production callers outside the module's tests. The entire module is
not dead: outcome and structure code consume reviewed registry, generated
species, formula inventory, and model-proposed species helpers.

Required outcome: delete or explicitly reserve only the unused adapter/cache
subsystem, then split the live identity construction helpers into a focused
module. Name an intended production consumer for anything retained.

### AUD-022 — Align Codex preflight with actual search use

Verdict: **Confirmed.**

All three current invocations pass `live_search: false` to `invoke`, but
`cached_capabilities` hard-fails when top-level help lacks `--search` at
`crates/agent/src/codex.rs:944`. Remove that unconditional requirement or make
it conditional on a real request that uses search.

The broader report is partly overstated: `ClaimSource` is syntactically
validated during claim decoding, so it is not unreachable. What is absent is
the promised retrieval/corroboration flow and `EvidenceBacked` upgrade
described in `docs/agent-workflow.md`, `docs/product-spec.md`, and the dynamic
rebuild plan. Either implement that separate product slice or correct the
promises and trust labels.

### AUD-023 — Resolve unused presentation variants

Verdict: **Confirmed with camera nuance.**

- Only `WideEstablishingShot` is constructed in production. Camera behavior
  is read into presentation timeline beats, but the 3D renderer deliberately
  uses `fixed_camera_pose` and never consumes the behavior. It is semantically
  inert, not literally unread.
- `foam_amount` is never assigned a nonzero production value.
- `HeatDistortion` has downstream registry/renderer support, but compatibility
  always returns false, so no valid profile selects it.
- Only `FlamePalette::Lilac` is selected in production; Natural, Crimson, and
  YellowOrange are render-only variants.

Required outcome: remove inert variants and support tables or name and test an
imminent producer. Keep the renderer and presentation compiler exhaustive.

### AUD-024 — Repair the catalogue trust contract; do not delete it blindly

Verdict: **Partly confirmed; high contract debt.**

The crate docs say only `TrustedCatalogue::from_canonical_json` crosses the
runtime trust boundary and requires a host-pinned digest and attestation.
The type's comment at `crates/chem-catalogue/src/lib.rs:505` says the opposite,
and its constructor merely calls `ValidatedCatalogueBundle::from_json`.
Nevertheless the wrapper is widely consumed and is the type-level distinction
accepted by `expand_trusted` and `validate_trusted`.

Required decision: restore real host-pinned trust construction or rename and
redesign every producer/consumer and governing document consistently.
`CatalogueError::is_system_error` at `lib.rs:120` always returning `true` is
confirmed and should either disappear with a single error class or become a
real typed classification.

### AUD-025 — Remove genuinely unused dependencies

Verdict: **Confirmed.**

`chems-lang` is declared by both `agent` and `chemspec-app` with no Rust source
references in either crate. Remove them after a `cargo metadata`/build check.
Also make app `serde` and `serde_json` use workspace dependencies.

`glam = 0.25.0` being old and `bytemuck = 1.25.1` being patch-pinned are not
defects by age alone. Review version constraints against current Iced/wgpu and
MSRV compatibility before changing them.

### AUD-026 — Keep diagnostic edits based on product needs

Verdict: **Not confirmed as unjustified.**

`SafeEdit` has one production emitter and substantial conformance tests;
`SourcePosition` supports exact source positions. These are language and
diagnostic capabilities, not necessarily abandoned LSP code. Retain them if
the source editor or agent repair protocol consumes the contract; otherwise
reduce them only after tracing those product requirements. Sparse construction
alone is insufficient evidence for deletion.

## Priority 4 — mechanical decomposition

### AUD-027 — Replace long-function lint suppression with seams

Verdict: **Confirmed.**

The workspace contains 176 `clippy` allow attributes in Rust source and tests;
`structural_2d.rs` alone contains 31. `too_many_lines` occurs 23 times under
`chem-kernel` at this revision, so the report's count of 15 is stale or used a
different scope. Clear remaining production hotspots by extracting domain
operations, not by chasing a zero-attribute metric. Immediate examples:

- `agent/src/mechanize.rs:81`;
- `chem-kernel/src/validate.rs:704` (`WorkingState::apply`);
- compiler functions at `chem-presentation/src/lib.rs:169`, `:415`, `:635`,
  and `:1340`.

### AUD-028 — Split the app root along existing Elm seams

Verdict: **Confirmed as maintainability debt; one claim rejected.**

`crates/chemspec-app/src/main.rs` is 7,111 lines and combines bootstrap/CLI,
state, routing/update, views, and roughly 2,400 lines of inline tests. Move to
a small binary plus library/state/update/screens/harness modules while keeping
State → Message → Update → View explicit.

The report's claim that an unrouted root `Message` silently fails is not
confirmed: `update_with_task` is exhaustive and subrouters end in explicit
`unreachable!` branches. Preserve that property during the split.

The three validation options are a real invariant smell but are not always a
single triple: dynamic presentation has frames and declaration without a
catalogue macroscopic record. Replace them with a typed enum/capability such
as catalogue and dynamic variants, not blindly one struct requiring all three.

### AUD-029 — Split catalogue validation by its staged pipeline

Verdict: **Confirmed as maintainability debt.**

`crates/chem-catalogue/src/lib.rs` is 4,304 lines while `validation.rs` already
names a staged pipeline. Move stage bodies and supporting checks into focused
modules without changing validation order, diagnostics, or canonical bytes.

### AUD-030 — Split structural renderers after shared extraction

Verdict: **Confirmed as maintainability debt.**

`structural_2d.rs` is 4,519 lines and `structural_3d.rs` is 6,127. Extract
AUD-014 and AUD-015 first, then split around pipeline/scene/molecule/effects/
mesh seams. Meaningful layout stays GPU-independent and deterministic.

### AUD-031 — Split kernel validation and mechanism compilation

Verdict: **Confirmed as maintainability debt.**

Split `validate.rs` by operation family after AUD-005 and trust-boundary tests
land. Split `mechanism.rs` around request compilation, algorithmic derivation,
provider proposal/repair, and structure escalation after AUD-006/AUD-007.
Extract a reusable bounded-child process runner from `codex.rs` as part of
AUD-004, not as an unrelated abstraction first.

### AUD-032 — Consider error-derive consolidation pragmatically

Verdict: **Recommendation, not a defect.**

`chem-domain` has 28 hand-written `Display`/`Error` implementation headers.
The workspace already uses proc-macro derives such as serde, so a presumed
zero-proc-macro doctrine does not explain them. `thiserror` could reduce
boilerplate, but adding a dependency is not automatically more idiomatic.
Evaluate whether it preserves exact compatibility wording and source chaining;
record a no-new-error-derive policy only if that is an intentional constraint.

### AUD-033 — Consolidate small duplicated algorithms

Verdict: **Confirmed; low.**

There are four distinct GCD implementations, not three:
`chem-domain::{generate,structural,reaction}` and `agent::solve`. Their integer
types differ, so consolidate only the compatible primitive versions or use a
small generic/exact helper without obscuring BigInt behavior.

## Priority 5 — docs, CI, and regression contracts

### AUD-034 — Repair stale documentation

Verdict: **Confirmed.**

- `crates/README.md` lists five of eight crates, omits agent, presentation, and
  app, and still calls other modules “archaeology code.”
- `docs/agent-workflow.md` and older entries in
  `docs/plans/rebuild-decisions.md` state a 120-second mechanism bound while
  code uses three minutes. The active rebuild plan and its latest decision
  entry already record the 180-second supersession, so update the remaining
  stale authority rather than flattening decision history.
- `chem-domain/src/smiles.rs` says aromatic lowercase and stereochemistry are
  unsupported while its parser carries aromatic and chiral state and tests.
- `chem-presentation/Cargo.toml` lacks the package description present on the
  other workspace crates.

### AUD-035 — Strengthen CI supply-chain and build efficiency

Verdict: **Confirmed; policy selection required.**

Current CI runs format, workspace tests, and Clippy cold on Linux, plus tests
on macOS and Windows. It has no Cargo cache, `cargo-deny`/audit gate, or unused
dependency gate. Add caching and choose pinned, maintainable supply-chain
checks. Treat `cargo-udeps` as advisory or pin its nightly/toolchain behavior
so it does not destabilize the Rust 1.96.1 gate.

### AUD-036 — Add renderer smoke coverage

Verdict: **Confirmed as a missing gate; implementation feasibility pending.**

The app exposes structural smoke modes and frame dumping, but CI never invokes
them. Add a Linux software-rendered smoke lane only after proving llvmpipe and
the selected display backend are deterministic enough for CI. Hash a stable
artifact or assert invariant image properties; do not create brittle
cross-driver golden pixels accidentally.

### AUD-037 — Make frame digests a reviewed regression surface

Verdict: **Recommendation.**

Check reviewed `react --verbose` digests for representative catalogue cases,
including resonance/delocalization, and document the intentional update
workflow. A digest proves deterministic artifact identity, not chemical
correctness; expected digests must be independently reviewed and must not be
generated by the implementation under test and accepted as their own oracle.

### AUD-038 — Add adversarial property tests at untrusted boundaries

Verdict: **Recommendation with high value.**

Fuzz/property-test catalogue `from_json`, provider claim parsing, mechanism
responses, and structure proposal adoption for no panics and typed failure.
Include bounded sizes and deterministic seeds. Prioritize the concrete hostile
cases from AUD-001, AUD-002, AUD-007, and AUD-008 before broad fuzzing.

### AUD-039 — Replace wall-clock assertions and manual temp directories

Verdict: **Confirmed.**

Tests assert `<250 ms`, `<1 s`, and `<10 s` in agent/domain/app code. These are
host-load-sensitive. Keep performance expectations in benchmarks or generous
opt-in smoke tests; ordinary correctness tests should assert bounded work or
call counts. Multiple tests create process-ID temp directories and remove them
only on the success path. Use RAII temporary directories where supported.

### AUD-040 — Extract the headless chemistry path from the GUI crate

Verdict: **Recommendation.**

The `react` path is valuable but currently lives in `chemspec-app`, pulling
Iced/wgpu into headless verification. Extract request resolution, run artifact,
JSON projection, and digest to a renderer-free crate or existing non-UI
boundary; leave CLI argument parsing in a small binary. Preserve the exact
`atoms_from_name -> resolve_drafts -> run` behavior with differential tests.

## Explicitly retained design choices

The audit found no evidence to reverse these choices:

- the custom lossless lexer/parser is justified by layout tokens, stable
  diagnostics, source spans, comments, and formatting;
- deterministic `BTreeMap`/`BTreeSet` use is intentional;
- bounded graph/isomorphism search in `pattern.rs` supports real ambiguity
  detection;
- the kernel's re-execution of domain constructors is valid defense in depth;
- Iced remains confined to the application boundary;
- reviewed and dynamic paths must continue through identical structural
  validation before presentation.

## Slice order and integration gates

The order below is exhaustive: every audited item has one position. It is a
dependency order, not permission to batch adjacent IDs into one large change.
Each item should remain a small vertical slice with its own acceptance tests
and completion record. A slice may move earlier only when newly recorded
evidence proves that its prerequisites are already satisfied; otherwise update
this plan before reordering it.

The sequencing follows four rules:

1. Contain exploitable or hanging behavior before architecture work.
2. Establish the trusted interfaces and canonical artifacts before changing
   their callers.
3. Extract duplicated knowledge before splitting large files, so the resulting
   modules are deep and callers cross one small interface.
4. Characterize a stable seam before mechanically reorganizing its
   implementation, then run a final lint/decomposition sweep.

### Phase A — close live correctness and trust seams

These slices are deliberately first. They are either independently exploitable,
can turn bounded work into an unbounded hang, or define which values may speak
with validated authority.

| Order | Slice | Why it is here | Prerequisites |
| ---: | --- | --- | --- |
| 1 | AUD-004 | Restore real process deadlines on Unix/NixOS before relying on provider tests or CI timeouts. | None |
| 2 | AUD-005 | Make the kernel reject and preserve wrong delocalized products before broadening regression artifacts. | None |
| 3 | AUD-006 | Stop operational provider failures from consuming repair attempts before changing claim/provider types. | AUD-004 |
| 4 | AUD-007 | Close the same-count/wrong-species structure-adoption seam before refactoring dynamic claims or HIR. | None |
| 5 | AUD-008 | Put wire limits in the Rust validator so every provider and cache adapter shares one enforced contract. | None |
| 6 | AUD-002 | Split solver and provider provenance, using the bounded wire contract from AUD-008. | AUD-006, AUD-008 |
| 7 | AUD-024 | Decide and restore the catalogue trust capability before making it an input to a new private HIR constructor. | None |
| 8 | AUD-001 | Make expanded claim consistency unrepresentable behind a small kernel interface. | AUD-002, AUD-005, AUD-007, AUD-024 |
| 9 | AUD-003 | Clarify validated-dynamic promotion only after the derivation and provenance capabilities it consumes are sound. | AUD-001, AUD-002 |

Phase A exit gate:

- hostile claim, HIR, structure-response, and delocalization cases fail closed;
- cancellation and timeout kill descendant-held pipes on Unix and Windows;
- solver, provider, reviewed catalogue, review-candidate derivation, and
  renderer-readable dynamic frames have distinct typed provenance;
- no learner-facing claim field can be changed after the consistency-owning
  constructor succeeds.

### Phase B — make the gate reliable and cheap to use

These changes reduce false failures and cold-build cost before the longer
interface work. They must not relax the Phase A gate.

| Order | Slice | Why it is here | Prerequisites |
| ---: | --- | --- | --- |
| 10 | AUD-034 | Correct stale crate, timeout, SMILES, and package docs so subsequent work starts from current authority. | Phase A |
| 11 | AUD-039 | Remove flaky wall-clock assertions and leaking temp directories before adding more process/property tests. | AUD-004 |
| 12 | AUD-025 | Remove confirmed unused dependencies and normalize workspace dependency declarations before enabling an unused-dependency gate. | None |
| 13 | AUD-035 | Add dependency caching and pinned supply-chain/unused-dependency checks after the normal gate and dependency set are clean. | AUD-025, AUD-039 |

Phase B exit gate: the ordinary workspace gate remains deterministic under
parallel CI load, temporary resources clean up on panic, and CI reports
dependency/supply-chain failures without rebuilding every job cold.

### Phase C — canonicalize trusted artifacts and typed interior surfaces

This phase deepens the kernel, catalogue, and language modules. Tests should
exercise their public interfaces rather than preserving tests of duplicated
assembly internals.

| Order | Slice | Why it is here | Prerequisites |
| ---: | --- | --- | --- |
| 14 | AUD-018 | Merge the two expansion tails now that private HIR construction exists; equivalent inputs must produce equivalent provenance. | AUD-001 |
| 15 | AUD-012 | Replace JSON key sniffing after expansion has one assembly path, preventing a second digest rewrite later. | AUD-001, AUD-018 |
| 16 | AUD-009 | Move generated identity hashes to structured inputs while canonical serialization conventions are already under review. | AUD-012 |
| 17 | AUD-011 | Parse site/instance references once at the validated catalogue seam before schema and catalogue module work. | AUD-024 |
| 18 | AUD-020 | Generate or mechanically diff the public schema after the typed reference model is settled. | AUD-011 |
| 19 | AUD-010 | Replace CST node-kind strings with a closed enum before optimizing ownership of the CST. | None |
| 20 | AUD-013 | Benchmark, then remove token/subtree copying without mixing representation and ownership changes. | AUD-010 |
| 21 | AUD-038 | Add broad adversarial/property coverage only after the untrusted interfaces and canonical representations have stabilized. | AUD-001, AUD-002, AUD-007, AUD-008, AUD-011, AUD-012, AUD-020 |

Phase C exit gate:

- source and typed expansion paths have differential certificate, digest,
  validation, and frame tests;
- digest inclusion/exclusion is expressed by types rather than JSON names;
- validated catalogue references and CST node kinds are closed typed domains;
- schema/model drift and untrusted parser panics are continuous-test failures.

### Phase D — establish one authority for chemistry and presentation knowledge

This phase removes knowledge duplication before any god-file split. The shared
module in each slice must earn its interface through leverage across at least
two real callers; do not create pass-through modules.

| Order | Slice | Why it is here | Prerequisites |
| ---: | --- | --- | --- |
| 22 | AUD-019 | Reconcile solver and catalogue family authority before presentation derives more metadata from their declarations. | AUD-002, AUD-018 |
| 23 | AUD-017 | Make the checked declaration the sole post-validation equation authority once solver/catalogue agreement is explicit. | AUD-001, AUD-019 |
| 24 | AUD-016 | Move shared profile assembly behind one presentation interface after equation authority is singular. | AUD-017 |
| 25 | AUD-023 | Remove inert effects, palettes, foam, and camera behavior after the surviving presentation interface is known. | AUD-016 |
| 26 | AUD-036 | Add a software-rendered characterization lane before changing periodic palettes or ring geometry. | AUD-016, AUD-023, AUD-035 |
| 27 | AUD-015 | Consolidate periodic facts and the app palette with renderer characterization already protecting visible behavior. | AUD-036 |
| 28 | AUD-014 | Extract one renderer-independent ring-topology module before either renderer is split. | AUD-036 |
| 29 | AUD-037 | Establish reviewed frame/digest regression artifacts after semantic digests and solver/declaration authority are stable. | AUD-012, AUD-017, AUD-019 |

Phase D exit gate:

- solver and catalogue overlap has an explicit owner or differential gate;
- all validated equation copy comes from the current declaration;
- presentation assembly, periodic facts, palette selection, and ring topology
  each have one authority and shared tests;
- 2D and 3D smoke results are captured before their implementations move.

### Phase E — remove unsupported capability promises

These are intentionally after the relevant trust, identity, language, and
presentation interfaces are stable. Delete shallow or unused seams; do not
delete live implementation merely because it shares a file with dead code.

| Order | Slice | Why it is here | Prerequisites |
| ---: | --- | --- | --- |
| 30 | AUD-022 | Align Codex search preflight and either implement or retract evidence-backed promises against the new claim provenance model. | AUD-002, AUD-006 |
| 31 | AUD-021 | Remove only the unused identity adapter/cache orchestration after structured identity hashing and dependency gates settle. | AUD-009, AUD-025 |
| 32 | AUD-026 | Decide SafeEdit/SourcePosition retention with the final CST and diagnostic interface visible. | AUD-010, AUD-013 |
| 33 | AUD-033 | Consolidate compatible GCD helpers as a small locality cleanup before files move. | None |
| 34 | AUD-032 | Decide error-derive policy before module splits would replicate the current boilerplate across more files. | AUD-024 |

Phase E exit gate: every retained seam has at least one named production
consumer and, where behavior varies, justified production/test adapters. Docs
make no release promise for an unreachable trust tier or effect.

### Phase F — extract deep modules and shrink the roots

Only now should the large files move. Each split must preserve the established
interface and replace internal tests with tests at that interface where
possible. The order follows dependency direction: inner chemistry modules,
then renderers/headless orchestration, then the app shell.

| Order | Slice | Why it is here | Prerequisites |
| ---: | --- | --- | --- |
| 35 | AUD-031 | Split kernel validation, mechanism compilation, and the bounded-child runner after their correctness interfaces are fixed. | Phase A, AUD-018, AUD-022, AUD-032 |
| 36 | AUD-029 | Split catalogue validation after trust construction, typed references, schema parity, family authority, and error policy are settled. | AUD-011, AUD-019, AUD-020, AUD-024, AUD-032 |
| 37 | AUD-030 | Split 2D/3D renderers after shared topology, periodic/palette policy, and smoke characterization exist. | AUD-014, AUD-015, AUD-023, AUD-036 |
| 38 | AUD-040 | Extract the renderer-free headless reaction module after chemistry/presentation authorities are stable and before splitting the app shell. | AUD-017, AUD-019, AUD-029 |
| 39 | AUD-028 | Split the app root after headless chemistry and renderer implementations have moved behind their final interfaces. | AUD-016, AUD-030, AUD-040 |
| 40 | AUD-027 | Perform the final long-function/allow-attribute sweep across the new module layout; remove only suppressions whose underlying design issue is gone. | AUD-028, AUD-029, AUD-030, AUD-031 |

Phase F exit gate: the kernel, catalogue, presentation, renderers, headless
reaction path, and app shell are deep modules with small interfaces; no split
merely mirrors a former section of a god-file while requiring callers to know
the same implementation detail.

Every correctness slice must run its narrow package tests while iterating and
finish with:

```sh
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Renderer, provider, process-tree, and cross-platform claims require their
explicit smoke gates; unit tests alone must not be reported as proving them.

## Completion record

For each completed slice, append:

- completion date and commit;
- exact contract and code surfaces changed;
- tests and smoke commands run with results;
- any remaining live/GPU/network/cross-platform boundary;
- follow-up IDs created because the slice uncovered new work.

### 2026-07-18 — AUD-004 Unix process-tree termination

- Completion: working tree, uncommitted.
- Contract and code: the agent's Unix process adapter now sends `TERM` and
  `KILL` directly to the child process group through safe `rustix` APIs,
  retains the final direct-child kill/reap fallback, and no longer depends on
  `/bin/kill`. The Windows adapter is unchanged. `rustix 1.1.4` with its
  `process` feature is now a Unix-only dependency of `agent`.
- Regression coverage: a shell descendant that ignores `HUP` and `TERM` and
  retains captured pipes is exercised directly, through provider
  cancellation, and through the preflight timeout path. Each path must close
  its pipes and return in under one second. Before the fix, the focused
  cancellation regression exceeded an eight-second outer timeout (exit 124);
  after the fix it completed in 0.13 seconds.
- Verification passed:
  `cargo test -p agent --lib provider_cancellation_has_a_closed_error_kind -- --nocapture`;
  `cargo test -p agent --lib 'codex::tests::' -- --nocapture` (10 passed);
  `cargo test -p agent --lib` (132 passed);
  `cargo fmt --all --check`;
  `cargo test --workspace --all-targets`;
  `cargo clippy --workspace --all-targets -- -D warnings`; and
  `git diff --check`. Cargo verification used the isolated target directory
  `/tmp/chemspec-aud004`.
- Remaining boundary: the corrected Unix behavior was exercised on Linux/NixOS.
  Windows process-tree termination remains compile-gated and unchanged, but
  was not live-tested on Windows. No live provider, credential, network, GPU,
  or macOS behavior was required or claimed.
- Follow-ups: none; the planned reusable bounded-child extraction remains
  AUD-031 and was deliberately not pulled into this correctness slice.

### 2026-07-18 — AUD-005 Covalent delocalization identity

- Completion: working tree, uncommitted.
- Contract and code: structure instantiation and ordinary covalent-order
  changes now preserve typed delocalization; final bond equality compares the
  full resonance-domain/effective-order annotation; frame projection has an
  explicit resonance regression. The mechanism wire contract, strict JSON
  Schema, prompt, compiler, and algorithmic graph diff now expose and emit
  `change_covalent_delocalization` instead of relying on annotation loss.
  Governing operation and kernel documentation was updated with the same
  invariant.
- Red evidence: the focused structure-instantiation and bond-change tests each
  returned `None` instead of the expected annotation. After those loss points
  were fixed, the full workspace correctly rejected four previously accepted
  algorithmic mechanisms (neutralization, carbonate decomposition, metal-oxide
  neutralization, and nitrate decomposition) until their graph-diff producer
  emitted explicit resonance operations.
- Regression coverage: final comparison rejects a missing annotation, wrong
  domain identity, and wrong effective order with `CHEMS-K053`; superoxide
  instantiation retains its 3/2 annotation; ordinary order change retains its
  annotation; frame projection retains it; and the sulfate graph-diff case
  asserts four explicit localized-to-delocalized changes while remaining
  model-free.
- Verification passed:
  `cargo test -p chem-domain`;
  `cargo test -p chem-kernel --all-targets`;
  `cargo test -p agent --lib` (132 passed);
  `cargo fmt --all --check`;
  `cargo test --workspace --all-targets`;
  `cargo clippy --workspace --all-targets -- -D warnings`; and
  `git diff --check`. Cargo verification used the isolated target directory
  `/tmp/chemspec-aud005`.
- Remaining boundary: strict-schema compatibility is unit-tested, but the new
  operation variant was not exercised against a live model subscription. The
  attempted potassium/rubidium plus oxygen headless checks stopped honestly at
  the existing multiple-outcome selection boundary and are not counted as
  kernel proof. No GPU or network behavior is claimed.
- Follow-ups: none. No reviewed digest fixture required alteration; the four
  newly exposed producer failures were resolved within this slice.

### 2026-07-18 — AUD-006 Typed provider repair boundary

- Completion: working tree, uncommitted.
- Contract and code: mechanism and structure proposal loops now admit only
  `InvalidProviderOutput` and `KernelRejection` to bounded repair. Every other
  `AgentErrorKind` returns immediately through the new typed `Failed` outcome;
  presentation enrichment propagates that original `AgentError` rather than
  converting it into model-facing validation text. Repairable failures still
  receive their exact local diagnostic on the next call.
- Red evidence: before the change, a preclassified cancellation was called
  three times rather than once and its text was passed back as a repair
  diagnostic. The same unconditional retry branch existed in structure
  proposal escalation.
- Regression coverage: both proposal loops check every non-repairable closed
  error kind for one call, no repair diagnostic, and unchanged kind/context/
  message. Separate tests prove invalid structured output and kernel
  rejection make exactly one targeted repair with the expected diagnostic,
  and unsupported structure capability returns without repair.
- Verification passed:
  `cargo test -p agent --lib provider_errors -- --nocapture`;
  `cargo test -p agent --lib invalid_provider_output_error_is_repaired_with_its_diagnostic -- --nocapture`;
  `cargo test -p agent --lib kernel_rejection_is_repaired_with_its_diagnostic -- --nocapture`;
  `cargo test -p agent --all-targets` (136 passed, 2 live probes ignored);
  `cargo fmt --all --check`;
  `cargo test --workspace --all-targets`;
  `cargo clippy --workspace --all-targets -- -D warnings`; and
  `git diff --check`. Cargo verification used the isolated target directory
  `/tmp/chemspec-aud006`.
- Remaining boundary: error classification and call counts use deterministic
  fake providers. No live provider, credential, network, GPU, macOS, or Windows
  behavior is required or claimed. The existing Unix cancellation regression
  failed once during a parallel agent run with `ProviderUnavailable`, then
  passed focused and in the repeated full agent run; its strict assertion was
  retained unchanged.
- Follow-ups: none.
