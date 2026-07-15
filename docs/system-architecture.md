# System architecture

## Architectural rule

ChemSpec has four distinct authorities:

> The catalogue supplies reviewed chemistry; the agent proposes source and
> observations; the kernel produces trust and structural meaning; the
> application produces the experience.

No downstream component may synthesize chemistry omitted by the component
upstream of its trust boundary.

## Runtime flow

```text
natural-language reaction request
  -> catalogue identity resolution
  -> reviewed-rule applicability
     -> unsupported / no reaction / ambiguous / invalid: stop honestly
     -> unique supported outcome: continue
  -> provider researches typed observations
  -> provider authors concise structural .chems 1
  -> chems-lang parses source
  -> chem-kernel resolves and expands reviewed rule
  -> chem-kernel validates immutable structural derivation
  -> ValidatedStructuralReaction
  -> paired structural and observation frames
  -> chem-presentation guided and macroscopic plans
  -> Iced Canvas/wgpu presentation
  -> provider supplies post-playback overview
```

The simulation does not parse `.chems`; the agent does not construct trusted
domain values; the renderer does not infer bonds; and the application cannot
mark a reaction valid.

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
digests, validation, and deterministic indexes.

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

Owns provider preflight, observation research, evidence packets, concise source
proposal, bounded repair, post-simulation overview, cancellation, timeouts, and
normalized workflow events. It returns claims and text, never trusted chemistry.

### `chem-presentation`

Compiles trusted kernel frames into deterministic educational scenes and binds
host-selected macroscopic styling into a scene plan. Effects require matching
validated observation predicates. It cannot parse source, expand rules, alter
frames, or construct chemical state.

### `chemspec-app`

Composes request states, provider selection, visible workflow, source editing,
expanded-certificate inspection, diagnostics, derivations, paired playback,
guided 2D and macroscopic 3D views, and overview. Only the application depends
on Iced and GPU presentation.

### Oxygen screening boundary

Oxygen screening is a closed-world catalogue layer. It maps all 118 registered
elements to representative, no-direct-reaction, ambiguous, or unsupported
outcomes and admits only compounds already present in the structural
catalogue. A screening result cannot construct frames or bypass the reviewed
structural-rule and kernel boundary.

## Shared contracts

### `ResolvedReactionClaim`

```text
source hash and catalogue version/digest
declared reactants, products, coefficients, and equation
model disclosures
evidence packet and typed observation references
selected rule and complete role binding
source origins
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

## Persistence and staleness

The `.chems` file is the human-readable authored artifact. Evidence,
certificate, derivation, and frames remain separate and bind to source,
catalogue, and evidence digests. Editing source or changing either trusted
digest invalidates every downstream value.

## Platform decision

ChemSpec remains a native Rust application using Iced and `wgpu`. Platform code
is limited to provider discovery, storage, credentials, file dialogs, process
management, and packaging. Structural meaning remains platform- and
renderer-independent.
