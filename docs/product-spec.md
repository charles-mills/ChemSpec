# Product specification

## Summary

ChemSpec turns a theoretical chemistry question into a rule-supported,
structurally validated, and visually explainable reaction outcome.

Deterministic catalogue rules first provide a fast path for known outcomes. On
a catalogue miss, an AI agent researches and constructs a self-contained
working catalogue, evidence packet, and concise `.chems 1` source. The engine
expands the selected rule into an atom-mapped structural certificate, validates
every graph state, and drives paired observation and structural-change
simulations.

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
supported outcome applies belongs to the validated rule, whether the rule came
from the host-pinned fast path or a per-run working catalogue. The UI keeps the
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
ceiling. Missing catalogue coverage invokes Codex. Its generated structures,
premises, rule, evidence, and source remain per-run working data and become
displayable only after the same deterministic structural validation succeeds.
Digest-pinned promotion is an optional latency/token optimization for future
requests.

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
3. **Build on miss.** Otherwise Codex researches qualitative claims and authors
   a self-contained working catalogue, evidence packet, and `.chems 1` source.
4. **Bind.** The language/kernel resolve source identities and the exact rule
   from the selected pinned or working catalogue.
5. **Expand.** The engine deterministically creates instances, atom mapping,
   structural operations, and an inspectable certificate.
6. **Validate.** Every mapping, graph step, valence, charge, electron,
   association, metallic domain, conservation, and product invariant passes.
7. **Simulate.** Observation and representative structural views play together.
8. **Explain.** The agent connects visible observations to validated changes
   without presenting a real-world method.

## Product states

- **Validated** — every required premise and structural invariant passes with
  no model assumption; unreachable in the initial language.
- **Validated with assumptions** — validation passes with visible theoretical
  model disclosures attached; this is the initial language's successful
  result.
- **Unsupported** — the request is unsafe, materially ambiguous, or cannot be
  represented or validated by the current language and chemistry engine after
  bounded construction/repair.
- **Invalid** — source or derived structure contradicts the language or trusted
  premises.
- **System error** — trusted data or runtime infrastructure is corrupt.

Provider failure is a workflow state, never a chemistry state.

## Simulation claim

Persistent disclosure:

> Representative explanatory model. Structural identities, mapping, graph
> operations, charge, and electron conservation are validated. Layout, motion,
> timing, finite metallic fragments, and intermediate positions are
> illustrative.

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
