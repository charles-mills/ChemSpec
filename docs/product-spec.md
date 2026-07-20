# Product specification

## Summary

ChemSpec turns a theoretical chemistry question into a rule-supported,
structurally validated, and visually explainable reaction outcome.

Deterministic catalogue rules first provide a fast path for known outcomes. On
a catalogue miss, an AI agent returns a compact factual claim. ChemSpec resolves
stable species identities, balances the outcome exactly, and displays an
honestly labelled static result. Evidence verification and structural
presentation are separate optional stages: reviewed families or bounded
model-proposed operations must cross the same deterministic kernel before any
simulation can run.

## Audience

The primary audience is secondary-school chemistry students aged roughly
14–18. Educators and introductory undergraduate students are secondary
audiences.

Progressive disclosure provides:

- paired simulations and a short explanation by default;
- reaction-rule and structural-change reasoning under **Why did this happen?**;
- authored source and expanded certificate under **Inspect `.chems`**;
- claim-level evidence under **Sources**; and
- mapping, graph, charge, and electron proofs under **Validation**.

## Learning outcome

After a supported reaction, a learner should be able to explain:

- the outcome selected by the supported rule;
- the expected visible changes;
- which atoms persist into which products;
- which localized covalent bonds change;
- how ionic association and metallic electron domains differ from covalent
  bonding;
- how atoms, charge, and valence electrons remain conserved; and
- why the displayed order is explanatory rather than automatically a proven
  mechanism.

## Product boundary

ChemSpec shows a representative structural outcome that its deterministic
engine can validate. It is not a laboratory instruction system, bulk solution
simulator, kinetics engine, molecular-dynamics system, or automatic mechanism
proof.

The authored language deliberately excludes quantities, apparatus, vessels,
timed steps, and physical preparation. Context needed to decide whether a
supported outcome applies belongs to the reviewed rule or the checked factual
claim. A runtime claim never becomes a catalogue rule. The UI keeps the
representative/explanatory model disclosure visible without turning provenance
tiers into the main product message.

## Chemistry scope

The architecture supports molecular covalent structures (including localized
dative single bonds with donor-to-acceptor provenance), monatomic and
polyatomic ions, ionic assemblies, metallic domains, reviewed groups, and
atom-mapped transformations. Aromatic bonding remains outside the closed
domain.

The language and deterministic chemistry engine remain closed over explicit
representations and invariants, but the shipped catalogue is not the product
ceiling. Missing catalogue coverage invokes Codex for a factual claim, never a
trusted graph or rule. Formula-only products remain static. When all structures
are locally resolved, a reviewed-family match or model-proposed mechanism may
become displayable only after identical deterministic structural validation.
Digest-pinned catalogue promotion remains a deliberate source-controlled
optimization.

The first complete reaction is lithium with water because it exercises
covalent, ionic, metallic, electron-transfer, observation, and mapping
boundaries in one educational example.

Every registered element can be checked against elemental oxygen. The check
may select a representative oxide-family product, report no direct reaction,
or stop as ambiguous/unsupported. Compound oxidation is initially restricted
to compounds already defined by the structural catalogue. These checks do not
unlock animation until a reviewed structural transformation also exists.

## Canonical journey

1. **Ask.** The learner asks what happens when two reactants interact.
2. **Resolve.** The application uses the host-pinned catalogue when an exact
   supported request is available.
3. **Reuse on repeat.** A cache-v3 envelope for the same stable identities,
   context, and governing contract digests is decoded and fully revalidated;
   it never bypasses the chemistry boundary. The learner can explicitly
   regenerate it.
4. **Build on miss.** The deterministic solver first returns a typed
   `SolvedClaim`; only a solver miss asks Codex for a closed `ProviderClaim`.
5. **Compile static meaning.** ChemSpec resolves identities, proves request
   binding, balances exactly, and constructs a checked `ReactionDeclaration`.
6. **Display early.** The first valid static outcome appears without playback.
7. **Verify when requested.** Hostile source retrieval can upgrade the same
   result from `ModelAsserted` to `EvidenceBacked` without replacing it.
8. **Enrich presentation.** A local reviewed family or bounded model-proposed
   mapping/operation sequence crosses expansion, kernel, and frame validation.
9. **Simulate and explain.** Observation and representative structural views
   play together with accurate reviewed/model-proposed disclosure.

## Product states

- **Validated** — every required premise and structural invariant passes with
  no model assumption; unreachable in the initial language.
- **Validated with assumptions** — validation passes with visible theoretical
  model disclosures attached; this is the initial language's successful
  result.
- **Unsupported** — the request is unsafe, materially ambiguous, or cannot be
  represented or validated by the current language and chemistry engine after
  bounded construction/repair.
- **Invalid** — source or derived structure contradicts the language or its
  declared premises.
- **System error** — reference data or runtime infrastructure is corrupt.

Provider failure is a workflow state, never a chemistry state.

## Simulation claim

The simulation remains a representative explanatory model. Structural
identities, mapping, graph operations, charge, and electron conservation are
validated; layout, motion, timing, finite metallic fragments, and intermediate
positions are illustrative. The player does not repeat this distinction as an
overlay on the model.

## Success criteria

The product succeeds when a learner can:

1. ask about any sufficiently specified reaction outcome;
2. receive a fast catalogue result or a Codex-built result through the same
   validation boundary;
3. inspect concise authored source and exact expanded structure;
4. follow stable atoms from reactants to products;
5. distinguish covalent, ionic, and metallic representations;
6. understand the structural conservation proof; and
7. receive an evidence-linked explanation.

No invalid, unsupported, incomplete, or stale value may reach either
simulation.
