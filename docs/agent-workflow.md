# Agent workflow and providers

## Role and validation boundary

The agent crate answers algorithmically first; the model supplies text only
for genuine unknowns and is never a chemistry authority. On a reviewed
catalogue miss the solver attempts the claim itself; on a solver miss, Codex
may provide two narrowly separated unvalidated chemistry artefacts:

1. a compact factual `ProviderClaim`; and
2. only when neither the graph-diff deriver nor local reviewed-family
   matching can animate the outcome, a mapping and ordered operation
   proposal over host-labelled structures.

Codex does not author `.chems`, catalogues, structures, valence states,
coefficients, internal IDs, or trusted capabilities. Stable species identity,
structure generation, exact balancing, typed declarations, family
applicability, kernel validation, frame projection, and cache revalidation
are local responsibilities. No provider result can bypass the next
downstream gate.

After chemistry validation, Codex mode also permits one presentation-only
`OxideAppearanceClaim` for an already-validated surface-oxidation product. It
uses live search, is bound to the exact product structure/formula and catalogue
digest, and selects only a closed local colour family. It cannot alter
chemistry or catalogue content. The application labels its authority
`ModelAsserted`, prefers reviewed catalogue colour, and retains a neutral-grey
fallback on every failure.

The final product record may also request a presentation-only reaction context
note. The chat presents this guide as **Prof. Codex**, with a warm,
conversational professor voice that remains concise and age-appropriate. Each
starter chip sends its own learner question on the first activation, appends it
to the transcript immediately, and gives Codex that question alongside the
validated display equation. Prof. Codex answers the selected focus directly;
condition questions stay high-level about temperature, pressure, and any
commonly used catalyst, while application questions explain practical uses or
relevance without forcing an industrial or environmental classification. The
answer is limited to two or three short
paragraphs and is explicitly prohibited from supplying a step-by-step
procedure, quantities, apparatus, optimization, procurement, purification,
collection, or safety-control bypasses. This note is not chemistry authority
and cannot modify the reaction, frames, catalogue, or validation state. After
the first answer, the learner may ask follow-up questions through a bounded,
typed transcript. Each follow-up retains the same educational safety limits,
is independently generation-scoped, and remains presentation-only. Local mode
never invokes either request.

While the first answer is pending, the conversation body stays free of a large
loading notice. The disabled input carries the active-only Prof. Codex thinking
indicator, cycling from one to three dots; its timer subscription is absent as
soon as the request stops or the learner leaves the screen.

Unlike chemistry-bearing provider contracts, reaction chat is bounded plain
text rather than JSON. It runs through an ephemeral, read-only Codex app-server
thread and streams only final-answer text deltas; commentary, reasoning, tools,
and other provider events are discarded. The completed agent-message item is
authoritative and must still pass the local non-empty, 2,400-character, and
paragraph-count checks before it enters the retained transcript. Cancelling,
leaving the owning screen, or replacing the reaction interrupts the active
generation, and late deltas are rejected by request ID.

## Progressive result path

```text
request
  -> reviewed catalogue fast path
  -> stable reactant identity + generated structures
  -> algorithmic solver claim (families, confident no-reactions)
     -> miss: cache v3 replay, then provider compact claim
  -> exact balance + checked ReactionDeclaration
  -> immediate static outcome
  -> algorithmic graph-diff mechanism
     or local reviewed-family match
     or bounded model-proposed mechanism escalation
     or labelled mechanism-unavailable static outcome
```

The application commits the first valid static result before mapping or
animation work. Static results have no frame or playback capability. Solved,
reviewed-family, and escalated animations cross the same expansion, kernel,
and frame-validation boundary. An escalated sequence is always disclosed as
model-proposed.

Every claim, verification, and presentation task carries a monotonically
changing generation ID. Late or duplicate completions are ignored. Editing the
request clears all prior dynamic frames immediately. **Regenerate** bypasses
cache but replaces the stored entry only after the new result crosses the same
gates.

## Claim policy

The builder exposes one low-latency claim path. It uses model knowledge,
returns no invented citations, and targets the first static result. **Verify
with sources** can later locate and fetch direct support. This product behavior
is unrelated to the Codex Fast service tier: release invocation always requests
low reasoning and `service_tier="default"`.

## Codex subscription provider

Preflight locates `codex`/`codex.exe`, checks `codex --version`, reads
`codex login status`, and capability-probes `codex exec --help`. ChemSpec never
reads credential files.

Each chemistry-bearing invocation is ephemeral, read-only, ignores
repository/user rules and configuration, runs in an isolated temporary
directory, and uses a strict output schema. Live search is disabled for the
initial claim and enabled only for source-location calls. Mechanism proposals
and repairs never browse. Presentation chat uses the separate app-server
boundary above because it needs incremental final-answer events.

The release path fixes:

- reasoning to `low`;
- service tier to `default`;
- initial claim deadline to 30 seconds;
- source-location deadline to 90 seconds;
- escalated mechanism deadline to 120 seconds;
- claim repair to one targeted correction; and
- operation repair to at most two kernel-diagnostic corrections.

`CHEMSPEC_CODEX_MODEL` remains a development benchmark override. Promoting a
different release-default model slug is a deliberate decision backed by
benchmark evidence, not an ambient configuration change.

Chemistry-bearing `codex exec` JSONL is normalized to closed product events:
started, working, searching sources, completed, and failed, each with elapsed
time. Model text and hidden reasoning are discarded. The presentation-only
Prof. Codex chat instead consumes app-server final-answer deltas as described
above; it cannot produce chemistry capabilities. Failure, timeout, and
authentication states never become chemistry results.

## Compact claim contract

`ProviderClaim` is an opaque capability decoded from a closed `ReactionClaim`
wire schema containing only disposition, products,
required context, qualitative observations, direct source locations, and typed
ambiguity. Disposition is one of `reaction`, `no_reaction`, `ambiguous`, or
`unsupported`. Unknown fields, missing required fields, unsafe procedural
content, oversize output, and inconsistent dispositions fail closed.

The deterministic solver instead returns `SolvedClaim`. Both capabilities feed
the same exact claim compiler, but only `SolvedClaim` can carry a typed physical
`NoReactionReason`; provider JSON and cache bytes cannot express that field.
The compiler preserves this origin as `Derived` versus `ModelAsserted` rather
than asking the application to infer provenance from its current mode.

The source-locating call receives an immutable displayed claim and may change
only its `sources` array. Any product, observation, context, disposition, or
ambiguity change is a typed conflict.

## Evidence verification

Evidence fetching treats remote bytes as hostile. The curl adapter allows only
HTTPS, same-host bounded redirects, strict time/byte/decompression limits, and
HTML, plain text, or text-extractable PDF. It forwards no credentials, executes
no scripts, and creates no persistent cookies.

Every accepted excerpt must exist after deterministic normalization of fetched
bytes. A separate non-browsing adjudicator checks that mapped product or
observation fields occur in the supporting region. It can reject a mapping but
cannot confer trust. Complete fetched claim-level coverage upgrades the static
outcome to `EvidenceBacked` and stores a digest-bound snapshot for offline
replay.

One source replacement is allowed after a local check fails. A second failure
is final. Verification failure never discards or mutates an already displayed
structural result.

## Mechanism escalation

The local compiler supplies labelled resolved structures, exact coefficients,
and a closed operation vocabulary. Codex may return only a total atom mapping
and ordered operations over those labels. It cannot introduce species,
structures, coefficients, atoms, or operation variants.

Returned proposals cross the same expansion, kernel, and frame projection as a
reviewed family. Only invalid provider output and kernel rejection enter the
bounded repair loop; at most two operation-level repairs receive the exact
bounded validation diagnostic. Cancellation, timeout, unavailable capability,
authentication, transport, cache, and other operational failures return
immediately with their original typed error and are never shown to the provider
as repair feedback. A structure proposal request is re-derived canonically from
the validated static outcome at adoption: count, ordered IDs, names, formulas,
and reactant-before-product position must match exactly before any proposed
graph is validated or attached. Repair exhaustion preserves the static outcome
and exposes a retry affordance. Formula-only products never enter escalation
because ChemSpec does not fabricate unknown graphs.

## Cache v3

The cache key binds canonical request identities and context, claim/mode
contracts, identity snapshot, reference catalogue digest, compiler contract, and
mechanism contract. Its envelope stores untrusted claim bytes, provider/model
provenance, and an optional presentation recipe. It never serializes a
trusted capability.

Every load recompiles request binding and exact balance and revalidates
reviewed-family or escalated presentation through the kernel. Corrupt and old
entries are misses and are not deleted. Cache lookup precedes Codex
preflight, preserving offline replay.

The default location is the platform cache directory (`Library/Caches` on
macOS, `LOCALAPPDATA` on Windows, and `XDG_CACHE_HOME` or `.cache` on Linux).
`CHEMSPEC_CACHE_DIR` overrides it.

Oxide appearance enrichment uses a separate v1 cache envelope in the same
directory. Its key binds the exact validated product request and catalogue
digest. Loads revalidate the model claim, source records, identity echo, claim
digest, and `ModelAsserted` tier before the renderer can consume the closed
colour family. Appearance entries are never merged into cache v3 or promoted
to the reference catalogue.

## Provider-neutral boundary

The current live implementation uses the signed-in Codex binary. BYOK/API is a
reserved provider-neutral direction only; no direct OpenAI HTTP call, API-key
persistence, hosted backend, account system, billing, or deployment is part of
this rebuild.

Normal tests use fake providers and consume no subscription or network. Live
Codex runs are explicit ignored smoke tests and must record provider, model,
and provider version.
