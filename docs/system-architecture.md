# System architecture

## Architectural rule

ChemSpec has four distinct authorities:

> The agent produces text; the validator produces trust; the
> chemistry engine produces meaning; the application produces the experience.

No layer may bypass the layer immediately downstream of it. In particular, raw
or stale provider output never reaches the simulation.

## Runtime flow

```text
structured reaction request
  -> host-pinned catalogue fast path
     -> hit: selected production rule
     -> miss: stable identity resolution + generated structures
        -> algorithmic reaction solver (families, confident no-reactions)
           -> miss: cache-v3 lookup, then provider ReactionClaim
        -> exact local balance + checked ReactionDeclaration
        -> private ValidatedStaticOutcome (no frames)
        -> algorithmic graph-diff mechanism derivation
           -> miss: reviewed-family match or bounded model proposal
  -> chem-kernel validates every animated structural derivation
  -> ValidatedStructuralReaction or ValidatedDynamicFrames
  -> paired structural and observation frames
  -> chem-presentation guided and macroscopic plans
  -> Iced Canvas/wgpu presentation
```

The simulation does not parse `.chems`; the agent does not construct validated
domain values; the renderer does not infer bonds; and the application cannot
mark a reaction valid. Dynamic frames retain `review_candidate` provenance even
after deterministic validation makes them renderer-readable.

## Workspace boundaries

```text
                         chemspec-app
                    /        |          \
            chems-lang   chem-presentation  agent
                              |
                         chem-kernel
                           /     \
                 chem-catalogue  chem-domain
```

### `chem-domain`

Pure stable types for elements, formulae, typed IDs, atom/electron state,
shared and directed dative covalent graphs, groups, ionic association, metallic
domains, reaction instances, atom mappings, structural operations, immutable
graph states, derivations, validated artifacts, and renderer-independent frames.

It has no parsing, catalogue I/O, UI, networking, provider, or GPU dependency.

### `chems-lang`

Owns `chems 1` dispatch, encoding/layout lexing, lossless CST, source AST,
comments, spans, formatting, and syntax diagnostics. It constructs unresolved
source only and cannot decide chemical support.

### `chem-catalogue`

Owns immutable reviewed structure entries, groups, valence/electron premises,
reaction applicability, product/map/operation templates, observation
compatibility, provenance, review attestations, schema versions, semantic
digests, validation, and deterministic indexes. Optional evidence-backed
macroscopic material records attach `Phase` to a structure in a standard or
exact rule-role context. They are presentation facts only and cannot authorize
a reaction or construct a product.

The implemented generalized-rules design extends this boundary with an
element registry, derived reviewed categories, checked structural traits,
structure templates, graph patterns, and inert family-rule records. G0–G6 are
complete and compile supported family members into the concrete kernel path.

### `chem-kernel`

Owns catalogue resolution, rule-role checking, deterministic expansion, typed
HIR, expanded certificates, graph-state execution, structural invariants,
conservation proofs, derivations, and private construction of
`ValidatedStructuralReaction`.

Generalized matching and rewrite instantiation remain on the elaboration side
of this crate: they compile to the existing concrete expanded reaction
before graph-state validation begins.

### `agent`

Owns the algorithmic reaction solver (classroom families, solubility and
activity tables, confident no-reactions), systematic naming in both
directions (product names and typed-name input), graph-diff mechanism
derivation, provider preflight, closed claim and mechanism wire contracts,
reviewed identity projection, exact outcome compilation, reviewed-family
matching, bounded mechanism escalation, cache v3, timeouts, and normalized
progress. Cached, solved, and fresh artefacts cross the same
identity/balance/kernel gates. It never constructs host-pinned catalogue
trust.

### `chem-presentation`

Compiles validated kernel frames into deterministic educational scenes and binds
host-selected or generic phase-driven macroscopic styling into a scene plan.
The generic compiler accepts catalogue-resolved material roles/phases and typed
observations, never reaction names. Effects require matching validated
observation predicates. It cannot parse source, expand rules, alter frames, or
construct chemical state.

### `chemspec-app`

Composes request states, provider selection, visible workflow, source editing,
expanded-certificate inspection, diagnostics, derivations, paired playback,
guided 2D and macroscopic 3D views, and overview. Only the application depends
on Iced and GPU presentation. It also owns versioned per-user preferences for
app mode and chemical label presentation. Formula/name selection changes only
the projection of already checked identities; it cannot alter source,
declarations, validation, frames, or catalogue meaning. Preference files never
contain provider credentials.

### Oxygen screening boundary

Oxygen screening is a closed-world catalogue layer. It maps all 118 registered
elements to representative, no-direct-reaction, ambiguous, or unsupported
outcomes and admits only compounds already present in the structural
catalogue. A screening result cannot construct frames or bypass the reviewed
structural-rule and kernel boundary.

## Shared contracts

### `ReactionClaim`

```text
closed disposition
factual product names, formulae, and phases
required context and typed qualitative observations
direct source locations and claim-field mappings
typed ambiguity alternatives
no structures, coefficients, mapping, operations, or internal trust
```

### `ExpandedStructuralReaction`

```text
resolved reaction claim
stable expanded reactant/product instances
total atom map
ordered typed operations
all proof-relevant premise IDs
canonical expanded certificate and digest
```

### `ValidatedStructuralReaction`

```text
expanded reaction
immutable graph state before and after every operation
atom/map/charge/electron/final-product derivation
validated observations and model disclosures
private construction token
```

### `SimulationFrame`

```text
observation stage
structural state
  stable atoms and charge/electron presentation
  shared and directed dative covalent edges
  ionic associations
  metallic domains
  changed relationships
  product membership
active operation
explanatory disclosure
```

### Structure-derived acid capability

Acid identity is not a formula or name whitelist. Any reviewed, cached, or
model-proposed structure that has crossed the normal graph validator may be
inspected for Brønsted-Lowry proton-donor sites. A site exists only when a
shared single X-H bond can be heterolytically cleaved to X through the exact
structural-operation electron ledger. The result identifies a possible proton
donor; acid strength, solvent behaviour, equilibrium, and applicability of a
complete-dissociation reaction family remain separate premise-bound facts.

Formula-only species receive no acid classification. This preserves isomer and
protonation ambiguity while allowing inorganic, organic, weak, and polyprotic
acids to remain valid compounds once their structures validate.

## Persistence and staleness

The `.chems` file is the human-readable authored artifact. Certificate,
derivation, and frames remain separate and bind to source and catalogue
digests. Editing source or changing a trusted digest invalidates every
downstream value.

Dynamic cache v3 is separate from authored `.chems`. It binds stable request
identities, context, identity/catalogue snapshots, and claim/compiler/mechanism
contract versions. It stores only untrusted claim/presentation recipes;
offline load reconstructs every capability through current validators.

Builder overlays have one typed presentation authority. A dynamic identity,
progress, failure, or result modal outranks toolbar panels and drag feedback;
while it is open, the composer prompt and background builder input are inert.
Closing it restores the prompt derived from the current reactants and provider,
and a generation-scoped completion reopens it while clearing any intervening
toolbar panel. Dynamic workflow copy is never duplicated in the composer.

All runtime navigation crosses one screen-entry transition. That transition
clears screen-owned transient state and recomputes the builder prompt from the
destination screen, current reactants, provider, and modal state before the
next view or subscription observes it. Returning to the builder preserves the
current reaction and restarts the normal prompt entrance motion. Starting
another reaction is a separate typed intent that clears the completed question,
result surfaces, conditions, and sketch state. Prompt intent has one typed
value; opacity is animation progress only and never decides whether the prompt
should exist. Direct screen assignment outside initialization and tests is
forbidden.

Typed reactant identity owns its canonical display formula until a manual atom
edit invalidates that identity. Conditions are reaction-request source, not
decorative builder state: a selected condition makes either non-empty slot a
valid request, empty slots are removed at handoff, and the conditioned request
always crosses the dynamic solver/provider and validation path rather than
falling through to an unconditioned catalogue result.

Screen-level playback shortcuts are distinct from pointer playback intents.
Structural entry uses a typed `Inactive -> Settling -> Ready` input state. The
settling phase advances with structural animation time, preventing queued
submit/continue keys from mutating the newly entered screen without depending
on wall-clock scheduling. Pointer playback explicitly arms immediately.
Smoke-window titles are derived from the live screen at the same navigation
boundary, not from the launch argument.

## Platform decision

ChemSpec remains a native Rust application using Iced and `wgpu`. Platform code
is limited to provider discovery, storage, credentials, file dialogs, process
management, and packaging. Structural meaning remains platform- and
renderer-independent.
