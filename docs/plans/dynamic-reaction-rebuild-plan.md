# Dynamic reaction outcome rebuild plan

## Status and decision

This is the implementation plan for replacing the model-authored
electron-ledger path on catalogue misses and completing the coverage and
experience work that follows it. It governs `DYN-104` through `DYN-119` in
[the repository implementation plan](implementation-plan.md) and this plan's
Phase 2 section.

The provider decision is fixed:

- the locally installed, signed-in **Codex binary is the default and the only
  live provider implemented by this rebuild**;
- Codex runs through the user's existing subscription authentication;
- BYOK/API remains a provider-neutral backup direction. It must not displace
  the Codex-binary default, and connecting it is not required by this plan;
- ChemSpec will not ship, deploy, or depend on a hosted backend; and
- the higher-cost Fast service tier stays off. Codex invocations explicitly
  use `low` reasoning and `service_tier="default"`.

This is an end-state architecture plan. It should reuse the proven application,
language, catalogue, kernel, and presentation boundaries, but it must not keep
a weak dynamic contract merely to minimize the diff. New identity, evidence,
outcome, escalation, and presentation subsystems are in scope where they
produce the stronger final product.

## Problem statement

The current dynamic path asks Codex to research a reaction and author exact
structures, valence tables, total atom mapping, before/after electron states,
ordered graph operations, catalogue metadata, evidence links, and `.chems`
source. That division of labour is both slow and mechanically fragile.

Live rubidium/water measurements established the failure mode:

- the default frontier Codex model exceeded the 300-second total deadline;
- the faster Luna model at low reasoning and the normal service tier took
  approximately 281 seconds and remained invalid after three repairs; and
- its final failure was an arithmetic valence-state error, not uncertainty
  about the reaction outcome.

The rebuild must make the provider answer only the factual question—what
happens—while ChemSpec handles representation, balance, family selection,
validation, presentation, and caching.

## Product outcome

On a catalogue miss, ChemSpec should normally produce a useful result in this
order:

1. resolve the two requested species as far as the local product domain allows;
2. ask Codex for one small reaction claim from model knowledge, without live
   search on the initial path;
3. corroborate the claim locally against bundled outcome data and reviewed
   family applicability;
4. balance and validate the claimed outcome locally;
5. show the equation, context, observations, and static structures immediately,
   labelled with the trust tier actually earned;
6. produce the full animated presentation—either a reviewed family match or a
   kernel-validated mechanism obtained through one bounded escalation request;
   and
7. store the successful result on the user's device with **Regenerate** and
   on-demand **Verify with sources** available.

The product goal is outcome parity: once a dynamic reaction is processed, its
playback quality and abilities equal a catalogued reaction's, differing only
in trust labelling. A missing reviewed family therefore triggers mechanism
escalation, not a degraded presentation. The static result remains on screen
while the mechanism is being derived, and it is the honest terminal state when
escalation exhausts its bounded repairs—presented as an explicit
mechanism-unavailable partial result with retry available, never as a
fabricated or generic animation.

## Authority and trust

The central boundary remains:

> Codex proposes a factual outcome; evidence supports the proposal; ChemSpec
> compiles representation; the validator produces structural trust; the
> chemistry engine produces meaning; the application produces the experience.

Two independent conclusions must not be conflated:

- **Factual support:** a reviewed local outcome or a fetched evidence snapshot
  supports the claimed reactants, products, context, and observations.
- **Structural validity:** deterministic code proves balance, request binding,
  structure validity, and, when animation exists, mapping and graph operations.

The kernel cannot prove that claimed products occur in reality. Conversely, a
web source cannot prove that an authored structural animation conserves atoms
and electrons. Dynamic results remain internal review candidates even after
both gates pass; only host-pinned reviewed catalogue content has production
catalogue trust.

Factual support is a typed ladder, not a boolean:

- **Reviewed** — host-pinned catalogue and generalized family content.
- **EvidenceBacked** — a dynamic claim corroborated by bundled outcome data,
  by matching a reviewed family's applicability, or by a fetched and checked
  evidence snapshot.
- **ModelAsserted** — a dynamic claim with no local corroboration and no
  verified sources yet. It is displayable and playable once structurally
  validated, with a quiet disclosure that the outcome comes from model
  knowledge and has not been source-checked.

Display is gated by structural validation and honest labelling; evidence gates
the badge, export, and any path toward catalogue promotion. A labelled absence
of sources is honest; an unverified citation pretending to be a gate is not.
**Verify with sources** upgrades a `ModelAsserted` result in place or surfaces
a conflict, and verification always runs before a dynamic result is exported
or proposed as a family candidate.

The UI should not shout this provenance distinction. A ready catalogue path
uses **Press space to find out**; a ready Codex fallback uses the identical
prompt treatment and motion with **Press space to ask Codex**. That prompt
fades away while the dynamic modal is open and fades back in if the modal is
closed. The modal is the exclusive owner of dynamic progress, failure, and
outcome copy: no inline status, toolbar panel, drag overlay, or builder
shortcut may remain active beneath it. Local Mode may use the same line for an
immediate, non-actionable **Try using Codex mode for this reaction** notice when
the deterministic solver declines. The result should use ordinary copy and a
concise model disclosure, not large `VALIDATED` or `MADE BY AI` banners. The
internal state remains explicit and typed.

## Runtime architecture

```text
structured request
  -> exact reviewed catalogue / generalized family
     -> hit: existing validated animated path
     -> miss:
        -> validated local dynamic cache
           -> hit: revalidate and present
           -> miss:
              -> Codex compact ReactionClaim (model knowledge, no search)
              -> local corroboration: bundled outcomes / family applicability
              -> exact local outcome compiler
              -> typed ReactionDeclaration
              -> registered reviewed family match?
                 -> yes: reviewed explanatory animation
                 -> no: bounded Codex escalation
                    -> missing product graphs: structure escalation first
                       (validated in an isolated working catalogue bundle)
                    -> kernel-validated: escalated mechanism animation
                    -> repairs exhausted: static outcome, mechanism
                       unavailable, retry available
              -> cache

on demand / before export or family candidacy:
  Verify with sources
    -> source-locating claim confirmation (live search)
    -> evidence snapshot gate (fetch, excerpt check, digest)
    -> EvidenceBacked upgrade, or typed conflict/ambiguity
```

`.chems` remains the human-readable authoring, review, fixture, interchange,
and export language. It is not a required serialization round trip in the
dynamic hot path. Introduce a closed typed `ReactionDeclaration` downstream of
syntax: the `.chems` parser constructs it through checked conversion, while the
dynamic outcome compiler constructs the same type through restricted checked
constructors. Both paths then cross the same catalogue/kernel boundaries.

Dynamic results may be exported as deterministic `.chems` when every referenced
identity and operation is representable, but playback does not wait for source
serialization and reparsing. This is a contract convergence, not a validator
bypass.

## Compact provider contract

Codex returns one strict `ReactionClaim`. It does not return coefficients,
atom graphs, valence states, mappings, operations, catalogue documents,
evidence packets, `.chems`, or hidden reasoning.

Conceptual wire shape:

```json
{
  "schema_version": 1,
  "disposition": "reaction",
  "products": [
    {
      "name": "rubidium hydroxide",
      "formula": "RbOH",
      "phase": "aqueous",
      "identity_hints": []
    },
    {
      "name": "hydrogen",
      "formula": "H2",
      "phase": "gas",
      "identity_hints": []
    }
  ],
  "required_context": "Representative reaction with liquid water",
  "observations": [
    {
      "predicate": "evolves",
      "subject": "hydrogen"
    }
  ],
  "sources": [
    {
      "id": "S1",
      "title": "Rubidium",
      "publisher": "Royal Society of Chemistry",
      "url": "https://example.invalid/direct-source",
      "supporting_excerpt": "A short passage supporting the outcome",
      "supports": ["products", "observations"]
    }
  ]
}
```

`disposition` is a closed enum: `reaction`, `no_reaction`, `ambiguous`, or
`unsupported`. Ambiguity is not repaired into a guess. Source excerpts are
short claim locators, not permission to reproduce substantial copyrighted
text.

The builder exposes one claim path. The claim comes from model knowledge
without live search and `sources` is empty; the separate verification path may
later fill `sources` with direct sources. Skipping citations never skips the
`no_reaction`, `ambiguous`, or `unsupported` honesty states. This path is
unrelated to the Codex `service_tier="fast"` setting, which remains off.

The result schema and prompt must stay small enough that the model-visible
task is dominated by chemistry research, not repository contracts. Set
regression budgets for both prompt bytes and maximum accepted result bytes.

## Mechanism escalation contract

When no reviewed family matches a supported balanced declaration, ChemSpec
issues one escalation request. This is the only path where the model proposes
mechanism content, and it is deliberately narrow: ChemSpec supplies the
resolved, labelled reactant and product structures, the balanced coefficients,
and the closed operation vocabulary. The model returns only a total atom
mapping and an ordered operation sequence over those given labels.

Conceptual wire shape:

```json
{
  "schema_version": 1,
  "mapping": [
    { "reactant": "hydrogen[1].h1", "product": "hydrogenchloride[1].h" }
  ],
  "operations": [
    {
      "kind": "cleave_covalent",
      "edge": ["hydrogen[1].h1", "hydrogen[1].h2", "single"],
      "allocation": "homolytic",
      "before": { "left": [0, 0, 0], "right": [0, 0, 0] },
      "after": { "left": [0, 0, 1], "right": [0, 0, 1] }
    }
  ]
}
```

The operation vocabulary and `[q,n,u]` state shapes are the existing kernel
forms. The mechanism response cannot introduce species, structures, valence
records, coefficients, sources, or observations; unknown atom labels and
unknown fields fail closed. Escalation runs without live search at low
reasoning. Kernel rejection produces a precise operation-level diagnostic and
at most two targeted repair attempts before the reaction settles to the
mechanism-unavailable static state. A validated escalated mechanism is cached
with the claim and marked internally as model-proposed review-candidate
content—reviewable and exportable as a draft `.chems` family candidate, never
self-promoted into the production catalogue.

When a claimed product has no reviewed structural graph, a separate closed
**structure escalation** call precedes the mechanism call: ChemSpec names each
missing species with its exact formula, and the model returns one structural
graph per species in the closed structure wire shape (atoms, bonds, groups,
components, associations, domains). Each proposal is compiled into an isolated
`Working` catalogue bundle—the full trusted document plus the proposed
structures under one provisional premise—and must cross the identical
catalogue validation every reviewed structure crossed: graph integrity, exact
element inventory, neutral charge binding, and supported valence states. The
structure response cannot alter the requested species, ids, formulas, the
claim, or the balance; it earns at most two proposal-level repairs, shares the
mechanism call's single escalation deadline, and never touches the trusted
catalogue. A validated proposal upgrades the product to a resolved species
whose identity confidence is capped at `ExternalUnverified` with explicit
model-proposal provenance, and is cached beside the mechanism response for
offline revalidation through the same isolated-bundle path.

This differs from the retired compact-ledger design in scale, not only shape:
escalation runs rarely (family misses only), has no research or search phase,
never authors structures, and receives operation-level diagnostics against
structures it did not invent.

## Canonical species identity and resolution

The current atomic-number multiset request is not a sufficient end-state
identity: it cannot distinguish constitutional isomers, stereoisomers,
tautomers, protonation states, coordination states, or salts from their
components. Introduce a first-class identity subsystem rather than allowing
formula/name guesses to leak into dynamic chemistry.

Each resolved species record contains:

```text
SpeciesId
display name
normalized aliases
formula
charge
phase/context qualifiers where identity-relevant
canonical structural graph
canonical serialization where supported
external identifiers such as InChI/InChIKey, canonical/isomeric SMILES,
  registry/database identifiers, and source provenance
stereochemistry and tautomer/protonation policy
identity confidence and ambiguity alternatives
```

Names and formulae are search keys and presentation, never database identity.
Resolution follows a capability chain:

1. exact host-pinned species registry match;
2. normalized alias/formula/charge/phase match;
3. locally cached external identity record;
4. bounded lookup through configured public chemistry identity sources; and
5. explicit user disambiguation when multiple identities remain.

External resolvers return untrusted records. ChemSpec validates identifiers,
formula/graph agreement, charge, graph integrity, and source provenance before
constructing `ResolvedSpecies`. Resolver adapters stay separate from domain
types and are fully faked in normal tests. Mature chemistry libraries and
public identity services should be integrated at narrow adapters rather than
reimplementing nomenclature, aromaticity, stereochemistry, or canonicalization
from scratch.

Public chemistry identity/evidence lookups are data-source integrations, not an
LLM API provider or ChemSpec backend. They require no ChemSpec account or API
key, remain optional behind local caches/capability checks, and must degrade to
an honest static/unsupported state when unavailable.

The composer evolves from atom-multiset identity to selecting or constructing a
resolved species. Existing simple inorganic composition remains a convenient
input mode, but it must resolve to `SpeciesId` before dynamic reaction lookup.
Ambiguity becomes a first-class product state with alternatives, not an
unsupported dead end or silent guess.

## Outcome compiler

The local outcome compiler owns only deterministic work:

1. parse reactant and product formulae using `chem-domain`;
2. solve the element-count system using exact integer/rational arithmetic;
3. reduce a unique positive solution to smallest integer coefficients;
4. reject no solution, non-unique unconstrained solutions, zero coefficients,
   charge mismatch, or requests whose authored reactants differ from the input;
5. resolve product species and canonical structures;
6. match registered generalized reaction families using resolved identities and
   graphs;
7. construct the checked typed `ReactionDeclaration`;
8. compile a reviewed family match through existing expansion, kernel, and
   frame validation; and
9. otherwise construct the mechanism-escalation request—labelled structures,
   coefficients, and operation vocabulary—and validate the returned mapping
   and operations through the same kernel and frame projection.

The model's optional family hint, if retained at all, may only order local
matching. It cannot select a family or bypass applicability.

There is no deterministic generic atom-mapping engine in this plan. Arbitrary
graph-to-graph mapping is a research-grade problem, and a mechanically derived
net transformation would still be a mechanism-shaped animation without a
mechanistic basis. The escalation contract asks the one component that can
propose a representative sequence, and the kernel remains the sole structural
authority over the answer.

The balancer must preserve the repository's exact-number policy. Binary
floating point is not permitted. Output order and coefficient normalization
must be deterministic.

## Evidence verification (on demand)

The current evidence decoder proves only schema and reciprocal links. The
rebuild adds a bounded external check that ChemSpec actually performs—run on
demand from **Verify with sources**, automatically before export or family
candidacy. It does not block first display of a structurally validated,
honestly labelled result.

Verifying a claim starts with one source-locating model call
(live search enabled) that must return direct sources for the existing claim
without altering its products or observations; a changed claim is a typed
conflict, not a silent correction. Each material product/context claim must
then map to at least one fetched authoritative source, with two independent
sources preferred when the outcome is novel, hazardous, conditional, or
disputed. Every accepted source must:

- use HTTPS and pass the application's source-policy checks;
- return through bounded redirects within a strict byte and time limit;
- have a supported textual content type;
- preserve final URL, publisher, retrieval time, and a content digest;
- contain the claimed supporting excerpt after normalized text extraction; and
- contain the relevant reactant/product names or formulae in the supporting
  region.

The evidence engine records claim-level coverage, conflicting passages,
publisher/source class, and retrieval limitations. A small evidence-adjudication
step may compare the already-fetched passages with the compact claim, but it
cannot browse, alter the claim, or confer production trust. Deterministic checks
still require the cited excerpt to exist in the fetched bytes.

This is stronger than transcribing a plausible URL, but it is not human
scientific review or proof of semantic entailment. It upgrades the result to
`EvidenceBacked`, not to production catalogue trust. A reviewed bundled
outcome remains the stronger and faster gate. Conflicting credible evidence is
an explicit conflict state on the result, never silently averaged.

Source fetching treats remote content as hostile data: no script execution, no
credential forwarding, no local-file URLs, bounded decompression, bounded text
extraction, and no source text copied into later shell instructions. Unsupported
formats or an absent excerpt cause an evidence failure.

One targeted Codex retry may request a replacement source. There is no
three-run full-plan repair loop. On-demand verification failure never discards
the displayed structural result: it leaves the result `ModelAsserted` with a
visible verification-failed note, or marks it conflicted.

## Presentation modes

Dynamic success has three typed presentation capabilities:

```text
StaticOutcome
  equation
  resolved/static species where available
  representative context
  qualitative observations
  evidence links
  mechanism availability state

EscalatedMechanismOutcome
  StaticOutcome
  ValidatedDynamicFrames (model-proposed, kernel-validated)

ReviewedAnimationOutcome
  StaticOutcome
  ValidatedDynamicFrames
```

Both animated capabilities expose identical playback and 2D/3D timeline
abilities—outcome parity is the point. Their chapter language differs:
reviewed-family animation explains its authored representative sequence, while
an escalated mechanism is introduced as a model-proposed representative
sequence that passed structural validation. A static result uses the same
visual language but shows reactants and products side by side, either while a
mechanism is still being derived or as the labelled mechanism-unavailable
terminal state. A formula-only labelled card appears only while structure
escalation is pending or after it exhausts its bounded repairs—never as a
success tier of its own. Any displayed molecular graph crossed full catalogue
validation; none is fabricated.

The progressive UI states are:

```text
Recalling outcome
Corroborating locally
Balancing reaction
Preparing structures
Matching a reviewed reaction family
Deriving a mechanism (escalation, only when no family matches)
Verifying sources (on demand)
Ready / Mechanism unavailable / Unsupported / Invalid / Provider failure
```

These are concise events, never hidden chain-of-thought.

## Codex CLI provider policy

The runtime keeps the existing capability-checked subprocess adapter:

- `codex exec` runs ephemerally in an isolated temporary directory;
- user config and repository rules are ignored;
- sandbox is read-only;
- live search is disabled for initial claims; it is enabled only for the
  verification source-locating call, and never for mechanism
  escalation or repairs;
- reasoning is explicitly `low`;
- `service_tier="default"` is explicit and Fast is not exposed in the UI or
  release configuration;
- output is constrained by the compact schema;
- the claim path allows one targeted retry, and the escalation path allows at
  most two operation-level repairs against kernel diagnostics; and
- late completions are rejected by generation ID.

Capability/version help probes may be cached for the application process.
Authentication status is checked at startup and after an authentication
failure. This is cleanup, not a claimed latency breakthrough.

Model selection must be benchmarked on the compact claim contract. Do not pin
a nominally faster model merely from its description: select the normal-tier,
low-reasoning model with the best measured valid-result latency and factual
success across the benchmark corpus. `CHEMSPEC_CODEX_MODEL` may remain a
development override.

The BYOK/API adapter boundary remains provider-neutral, but no direct OpenAI
HTTP invocation, API-key persistence work, backend, authentication service,
rate limiter, billing system, or deployment belongs to this plan.

## Local cache and regeneration

Successful dynamic results are stored in the platform cache directory. Bump
the format and discard current compact-ledger entries rather than migrate them.

The key includes:

- normalized stable reactant identities and any selected context;
- claim schema and prompt contract version;
- outcome compiler/cache format version;
- governing species/outcome catalogue digest; and
- family/compiler and mechanism-escalation contract versions used for
  animation.

The cached envelope stores:

- provider/model provenance;
- compact `ReactionClaim`;
- the trust tier and, when verified, evidence snapshot metadata and content
  digest;
- balanced compiled outcome;
- resolved species identities; and
- the validated animation artefact (reviewed-family binding or escalated
  mapping/operations) with its provenance, when one exists.

Every load decodes and revalidates request binding, evidence snapshot integrity,
balance, catalogue dependencies, and any animation artefacts before presentation.
Cache lookup precedes Codex preflight so an existing result remains usable
offline. **Regenerate** bypasses the entry and replaces it only after the new
result crosses the same gates. A failed regeneration preserves the prior valid
entry.

## Work packages

### DYN-104 — Compact claim contract

- Define closed `ReactionClaim`, `ClaimDisposition`, `ClaimProduct`,
  `ClaimObservation`, `ClaimSource`, and ambiguity wire types.
- Define the closed mechanism-escalation request/response wire types: labelled
  structures and coefficients in, total mapping and ordered operations out.
- Replace the current compact electron-ledger output schema and prompt.
- Remove provider responsibility for `.chems`, catalogue/evidence packets,
  structures, valence states, mapping, and operations.
- Keep fake-provider tests offline.

Acceptance:

- prompt and result size budgets are tested;
- unknown/missing fields and unsafe procedural content fail closed;
- conditional, no-reaction, conflicting, and ambiguous outcomes are representable;
- rubidium/water fits the contract without structural ledger fields; and
- normal tests consume no subscription usage.

### DYN-105 — Canonical species identity

- Add first-class `SpeciesId`, `SpeciesQuery`, `ResolvedSpecies`, ambiguity, and
  identity-provenance types in a renderer/provider-independent crate.
- Migrate existing reviewed structures and composer selections onto stable
  identities without losing exact catalogue bindings.
- Add narrow adapters for mature local chemistry tooling and public identity
  sources, with persistent device-local resolution caching.
- Support structural graphs, charge, isotopes, stereochemistry, tautomer and
  protonation policy, aliases, and external canonical identifiers.

Acceptance:

- formula/name synonyms converge on one identity where chemically equivalent;
- constitutional and stereochemical alternatives remain distinct;
- formula/graph/charge disagreement fails closed;
- resolver ordering and cache serialization are deterministic; and
- ambiguous input presents user-selectable alternatives instead of guessing.

### DYN-106 — Exact outcome compiler and typed declaration

- Implement exact equation balancing, charge conservation, phase/context
  binding, and request identity binding.
- Introduce the checked typed `ReactionDeclaration` consumed by expansion.
- Make `.chems` parsing and dynamic compilation converge on that declaration.
- Produce a private validated static-outcome capability that cannot construct
  simulation frames.

Acceptance:

- independent fixtures cover unique, impossible, underdetermined, ionic,
  redox, acid/base, combustion, and already-balanced systems;
- coefficient normalization is exact and deterministic;
- parser-built and dynamic declarations with equal meaning canonicalize
  identically; and
- source syntax or provider output cannot forge a validated declaration.

### DYN-107 — On-demand evidence verification

- Implement bounded HTML, plain-text, and text-extractable document retrieval
  at a hostile-data adapter.
- Implement the source-locating claim confirmation call for Fast-mode
  results, rejecting responses that alter products or observations.
- Verify model-supplied passages against fetched bytes and bind digests to exact
  claim fields.
- Classify publisher/source quality, detect conflicts, and retain retrieval
  limitations.
- Add one bounded source replacement and a non-browsing adjudication step
  for already-fetched passages where deterministic co-occurrence is
  insufficient.
- Upgrade the trust tier and cached entry transactionally, and run
  verification automatically before export or family candidacy.

Acceptance:

- unreachable, redirected-out-of-policy, oversized, decompression-bomb,
  unsupported-content, excerpt-mismatch, conflict, and claim-mismatch cases
  fail closed or become typed ambiguity;
- every `EvidenceBacked` upgrade has fetched claim-level coverage;
- a source-locating response that changes products or observations is a typed
  conflict, never a silent correction;
- verification never blocks, discards, or mutates an already-displayed
  structurally validated outcome;
- fixtures use local fake services and consume no network; and
- live retrieval is covered only by explicit ignored smoke tests.

### DYN-108 — Structural capability ladder and mechanism escalation

- Match balanced declarations against reviewed generalized families locally.
- Produce reviewed-family frames through existing expansion and kernel
  validation.
- Build the escalation request compiler: labelled resolved structures,
  balanced coefficients, and the closed operation vocabulary.
- Build structure escalation for products absent from the reviewed structure
  library: a closed graph-proposal contract validated inside an isolated
  working catalogue bundle before any mechanism work.
- Validate escalated mapping/operation responses through the existing kernel
  and frame projection, with at most two operation-level repairs.
- Settle to the labelled mechanism-unavailable static state when escalation
  exhausts, retaining the validated static outcome and a retry affordance.

Acceptance:

- family hints cannot override local applicability;
- family matching binds only reactant structures and derives products from
  the family's own reviewed selectors, so a formula-only or alias-resolved
  product can never block an applicable family;
- reviewed family, first-try escalated, repaired escalated, exhausted
  escalation, and structure-escalated cases have independent fixtures;
- escalated frames cross the identical kernel and frame validation as
  reviewed-family frames and expose identical playback capability;
- an escalated mechanism is always disclosed as model-proposed, never as a
  reviewed or experimentally established sequence;
- mechanism responses cannot introduce species, structures, coefficients, or
  unknown atom labels; structure proposals cannot alter requested species,
  ids, formulas, the claim, or the balance;
- exhausted escalation stays retryable without discarding the validated
  static outcome; and
- no unvalidated product graph is ever displayed.

### DYN-109 — Progressive application and composer flow

- Evolve the composer from atom multisets to resolved species selection while
  preserving the simple element-composition interaction.
- Keep the single claim path implicit in the builder; no mode selector or
  persisted claim-mode preference is exposed.
- Add typed identity, claim, evidence, balance, structure, mapping, and
  presentation stages.
- Render static, escalated-mechanism, and reviewed-family outcomes.
- Preserve generation IDs, stale completion rejection, elapsed time, back
  navigation, cancellation, and **Regenerate**.
- Use restrained result provenance/disclosure copy, including the trust-tier
  badge and the **Verify with sources** affordance.

Acceptance:

- disambiguation is usable without discarding the current request;
- the first supported outcome appears before optional mapping/animation work;
- static results expose no playback controls and clearly show whether a
  mechanism is pending, unavailable, or retryable;
- escalated and reviewed-family presentations use accurate chapter language;
- verification upgrades the badge in place without discarding playback state;
- stale and duplicate completions cannot replace current state; and
- failure states never retain a previous result's frames.

### DYN-110 — Cache v3 and offline replay

- Replace current dynamic entries with the identity/claim/evidence/declaration/
  presentation envelope.
- Bind keys to canonical identities, context, resolver snapshots, catalogue,
  escalation, and compiler contracts.
- Revalidate cached static and animated outcomes offline.
- Retain evidence and identity snapshot digests without requiring the source to
  remain online forever.

Acceptance:

- a repeated request avoids Codex and public identity/evidence lookups;
- changed identity, context, source snapshot, or governing digest misses or
  fails stale as appropriate;
- regeneration is transactional; and
- corrupt or old-format entries are ignored without deleting unrelated data.

### DYN-111 — Codex hardening and latency budget

- Reduce provider repairs to one targeted retry.
- Set low reasoning and the default service tier unconditionally in the release
  path.
- Stream normalized progress events from the Codex process without exposing
  hidden reasoning.
- Measure eligible Codex models on the same claim/evidence benchmark and choose
  the measured default without enabling Fast.
- Parallelize independent local identity/evidence preparation where it does not
  change meaning.

Acceptance:

- catalogue, generalized, identity-cache, and reaction-cache hits complete
  within 250 milliseconds on the primary development machine;
- cold runs report time to claim, evidence, static outcome, escalated
  mechanism, and reviewed animation separately;
- cold static outcomes target p50 at or below 15 seconds and p95 at or below
  30 seconds; verification-inclusive outcomes target p50 at or below 30
  seconds and p95 at or below 60 seconds, with a 90-second total deadline;
- escalated mechanism animations target p50 at or below 60 seconds and p95 at
  or below 100 seconds end to end, with a 120-second total deadline for the
  escalated path;
- optional local presentation enrichment never delays the first valid static
  result; and
- every failure, cancellation, and timeout is visible and bounded.

### DYN-112 — Breadth corpus and release proof

- Build a versioned corpus spanning inorganic, organic, ionic, molecular,
  metallic, acid/base, redox, precipitation, complexation, combustion,
  substitution, elimination, addition, biochemical, photochemical,
  electrochemical, no-reaction, conditional, and ambiguous cases.
- Include independently authored identities, outcomes, evidence expectations,
  balance oracles, and presentation-capability expectations.
- Measure factual accuracy per trust tier (`ModelAsserted` accuracy separately
  from `EvidenceBacked` coverage), identity resolution, balance, escalated
  mapping, presentation capability, latency, and failure classification
  separately.
- Use corpus failures to add generalized families and species records without
  weakening dynamic fallback.
- Keep expectations grounded with the `corpus-expectation-audit` binary: it
  resolves every scenario's reactants through the real registry and family
  matcher, so expectation data cannot silently drift from the product. No
  scenario or case may expect a plain static presentation as a success state.
- Verify every identity adapter and any native chemistry dependency in
  macOS, Windows, and Linux packaging rather than assuming development-machine
  availability.

Acceptance:

- the initial corpus contains at least 250 diverse requests and explicit
  adversarial mutations;
- every case reaches the expected reviewed, evidence-backed, ambiguous,
  unsupported, or invalid state;
- no case is counted successful solely because it is balanced;
- corpus and live-provider results are reproducible and report model/provider
  versions;
- at least 25 representative cases cross explicit live Codex/evidence smoke
  runs, while the full corpus remains deterministic and offline;
- identity functionality either packages on all three target platforms
  or has a tested capability/fallback path that preserves static outcomes; and
- the packaged primary-platform journey passes cold, cached, regenerate,
  ambiguity, mechanism-unavailable static, escalated-mechanism, and
  reviewed-family demonstrations.

## Phase 2 — coverage and delight (DYN-113 through DYN-119)

Phase 1 delivered parity: any reaction that clears identity and the reviewed
valence domain reaches the same validated player as catalogued content. Phase
2 removes the two remaining coverage walls, hardens live escalation to a
measured success rate, and makes the fast path *feel* fast. Its governing
principles:

- **Model for knowledge, machine for bookkeeping.** Live measurement showed
  the model is reliably right about outcomes and reliably sloppy about
  ledgers. Nothing in this phase asks the model for less chemistry; nothing
  relaxes the kernel's arithmetic.
- **Coverage gates are debt, not safety.** "That species is not catalogued"
  is a wall to remove, not a trust feature. The trust features are the kernel
  gate and honest labelling, both of which cost zero latency.
- **The realistic request universe is small.** A few hundred pairs cover most
  educational curiosity; pre-computing them beats optimizing the cold path.

### DYN-113 — Reactant-side structure escalation

- Let `compile_claim_outcome` accept formula-only reactants: when a composed
  reactant misses the registry, parse its formula from the composed atoms,
  bind the claim, balance, and produce the static outcome exactly as
  formula-only products do today.
- Generalize `missing_products` to missing species across both sides; the
  structure proposal request covers reactants and products in one call, with
  the identical isolated working-bundle validation and model-proposed
  labelling.
- Family matching treats a formula-only reactant as `NoMatch` (families
  require reviewed reactant structures by definition).
- Cache request binding and the composer flow accept formula-only reactants;
  the identity dialog appears only for genuine multi-identity ambiguity,
  never for a clean registry miss.

Acceptance:

- CH4 + O2 (and every corpus organic scenario) reaches a balanced static
  outcome without identity errors and becomes an escalation candidate;
- proposed reactant structures cross the same catalogue validation as
  proposed products, and the mechanism request compiles over them;
- the corpus expectations are regenerated with the expectation audit after
  this lands (organics move from `invalid` to `escalated_mechanism`); and
- a registry hit still never escalates.

### DYN-114 — Provisional valence states

- Collect every electron state used by proposed structures and by mechanism
  operation before/after transitions; states already in the reviewed
  `supported_states` remain authoritative and untouched.
- Admit an uncurated state into the isolated working bundle only when it
  passes the arithmetic identity anchored to the reviewed `neutral_valence`
  table: `formal_charge = valence_electrons − non_bonding_electrons −
  covalent_bond_order_sum`, with `unpaired_electrons ≤
  non_bonding_electrons` and metallic site-consistency checks.
- ChemSpec derives the provisional state records itself from the proposal;
  the model never authors a valence table (the exact failure mode of the
  retired design).
- Provisional states exist only inside the working bundle under the dynamic
  provisional premise; they never touch the trusted catalogue and carry the
  `ModelAsserted` framing of everything escalated.

Acceptance:

- a mechanism using a chemically ordinary but uncurated state (for example
  organic combustion intermediates) validates and animates;
- an arithmetically impossible state fails closed with a repairable
  diagnostic naming the identity violation;
- reviewed states still win where they exist, and the trusted catalogue is
  byte-identical before and after any dynamic run; and
- kernel conservation checks are unchanged.

### DYN-115 — Escalation hardening and measured reliability

- Bring the structure-proposal prompt to parity with the mechanism prompt:
  instance indexing, the electrons-not-in-bonds counting convention, and the
  supported/provisional state vocabulary in the request.
- Raise the escalated-path deadline from 120 to 180 seconds (supersedes the
  DYN-111 figure): at 40–60 seconds per call, the budget must fit one
  proposal and two repairs.
- Benchmark eligible Codex model slugs on the escalation contract (the plan
  already mandates measured selection; nobody has run it) and adopt the
  fastest slug with an acceptable kernel-valid rate.
- Run the 25-case live smoke selection end to end; fix systematic failure
  modes the way the Mg + CO2 session did (each fix is a prompt/request
  change validated live, then pinned by an offline regression test).

Acceptance:

- a measured live first-try and after-repairs success rate exists for the
  smoke selection, recorded in the decisions log with model and date;
- no live failure class remains that a request/prompt change fixes; and
- `live_probe` covers structure escalation as well as mechanism escalation.

### DYN-116 — Identity and tone polish

- Collapse label-variant alias ambiguity in the resolver with the
  isomorphism check, so the identity dialog appears only for chemically
  distinct alternatives and never shows internal ids as learner choices.
- Copy pass on trust language: one quiet "virtual model" disclosure per
  result; remove "sources unverified" from the window title; badges stay
  factual (`Reviewed`, `Model asserted`) without apologetic repetition.

Acceptance:

- composing CO2 or NaCl asks the learner nothing;
- a genuinely distinct-isomer ambiguity still asks, with human-readable
  names; and
- the result screen carries exactly one provenance disclosure.

### DYN-117 — Pre-baked dynamic cache

- Add an offline builder binary that runs a curated pair list (the corpus
  scenarios plus a few hundred likely educational pairs) through the full
  live pipeline and writes standard cache-v3 envelopes.
- Bundle the resulting entries with the app; cache lookup consults the
  bundled set after the device cache, and every bundled entry crosses the
  identical load-time revalidation (they gain no authority from bundling).
- Bundled entries are keyed to the shipped catalogue digest and contract
  versions, so a catalogue update naturally invalidates them at build time,
  not at runtime.

Acceptance:

- a fresh install answers every pre-baked pair with full animation in under
  one second, offline;
- a corrupted bundled entry is a cache miss, never a trusted artifact; and
- the builder reports which pairs failed live so the list stays honest.

### DYN-118 — The wait as part of the show

- While a mechanism derives, the result area presents the already-validated
  content as theatre: equation reveal, observation chips, the context
  sentence, and static structures with idle motion — not a spinner.
- Progress copy stays product-events-only, but phrased for learners
  ("working out where the electrons go…") rather than infrastructure
  ("Codex invocation").
- Raw provider diagnostics move to a details affordance; the headline failure
  states stay short and human.

Acceptance:

- during a cold escalation the screen is never static text-only;
- no provider jargon appears outside the details view; and
- cancellation and retry remain one tap throughout.

### DYN-119 — Single-reactant and context-driven requests

- Extend the request shape to allow one reactant plus a required context
  (thermal decomposition, photochemical decomposition), keeping the
  two-reactant contact form as the default.
- Energy inputs (heat, light, electricity) are context strings with closed
  vocabulary, never pseudo-species; the claim contract already carries
  `required_context`.
- The corpus photochemical/electrochemical scenarios move from `invalid` to
  honest claim-backed outcomes where the model supports them.

Acceptance:

- 2 AgCl under a light context reaches a balanced outcome without a
  "photon" species;
- two-reactant behavior is byte-identical to today; and
- the composer exposes the context choice without cluttering the default
  flow.

### Phase 2 order and dependencies

```text
DYN-113 reactant escalation ─┬─> DYN-114 provisional states ─> DYN-115 hardening ─> DYN-117 pre-baked cache
DYN-116 identity/tone polish ┘                                                      (117 needs 113–115 live-reliable)
DYN-118 wait-as-show: anytime after DYN-116 copy pass
DYN-119 request shapes: independent; after DYN-115 so its live cases are measurable
```

DYN-113 and DYN-116 can start immediately and in parallel. DYN-117's pair
list should only be baked after DYN-115's measured success rate exists,
otherwise the builder bakes today's failure modes into the bundle.

## Implementation order and dependencies

```text
DYN-104 compact claim
  -> DYN-105 species identity
  -> DYN-106 outcome compiler and typed declaration
  -> DYN-108 structural capability ladder

DYN-105 + DYN-106 + DYN-108
  -> DYN-109 progressive application
  -> DYN-110 cache v3
  -> DYN-111 Codex latency hardening

DYN-104 compact claim
  -> DYN-107 on-demand evidence verification (off the display critical path)

DYN-107 + DYN-111
  -> DYN-112 breadth and release proof
```

Identity, outcome, and evidence work may proceed concurrently after the compact
claim freezes. No live provider result may display until the identity and
outcome gates are connected and trust-tier labelling is in place; evidence
verification gates the `EvidenceBacked` badge, export, and family candidacy
rather than first display. Mechanism escalation work cannot begin until
canonical species graphs and `ReactionDeclaration` are stable.

## Explicit non-goals

- changing Codex binary from the default provider;
- implementing or requiring a direct OpenAI Responses API/BYOK adapter for
  completion; the provider-neutral seam remains available for later use;
- hosted services, accounts, billing, rate limiting, or deployment;
- a deterministic generic atom-mapping or graph-difference engine;
- presenting an escalated model-proposed mechanism as reviewed or
  experimentally established;
- allowing generated content to self-promote into the production catalogue;
- laboratory procedures or actionable hazardous instructions; and
- hiding unsupported or ambiguous outcomes behind a plausible animation.

## Verification

Iterate with the narrowest affected tests, then finish each integrated work
package with:

```sh
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p chems-conformance -- validate
```

Provider contract, balancing, evidence, cache, and application transition tests
must use fakes. Live Codex and external-source tests are ignored and explicitly
opt-in.

For the packaged macOS journey:

```sh
just agent-smoke builder
```

Target **ChemSpec Agent Smoke** and verify the title is **ChemSpec Agent Smoke
— Builder** before judging the UI. Exercise identity ambiguity, one cold
escalated mechanism, one mechanism-unavailable static outcome, one
reviewed-family animation, one cached repeat, and one regeneration, then run:

```sh
just agent-smoke stop
```

## Completion boundary

The rebuild is complete when a packaged app can use the signed-in Codex binary
to resolve unambiguous reactant/product identities, obtain an uncatalogued
reaction claim from model knowledge in seconds, corroborate it locally where
bundled data or a reviewed family agrees, verify claim-level evidence on
demand, construct an exactly balanced
typed declaration quickly, and reach full animated playback for supported
outcomes—reviewed families through their authored representative sequences,
unfamiliar reactions through escalated model-proposed mechanisms that crossed
the identical kernel and frame validation—then reuse the result offline and
regenerate it on demand. Once processed, a dynamic reaction's playback quality
and abilities equal a catalogued reaction's, differing only in trust
labelling. Only a reaction whose escalation exhausts its bounded repairs
remains a labelled mechanism-unavailable static result with retry. None of
these paths requires the model to author internal chemistry ledgers, an API
key, or a hosted backend.
