# Product specification

## Summary

ChemSpec turns a theoretical chemistry question into a rule-supported,
structurally validated, and visually explainable reaction outcome.

Deterministic catalogue rules first identify a supported outcome. An AI agent
then supplies evidence-backed qualitative observations and concise `.chems 1`
source. The engine expands the selected rule into an atom-mapped structural
certificate, validates every graph state, and drives paired observation and
structural-change simulations.

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

ChemSpec shows a representative structural outcome of a known reviewed
reaction. It is not a laboratory instruction system, bulk solution simulator,
kinetics engine, molecular-dynamics system, or universal reaction predictor.

The authored language deliberately excludes quantities, apparatus, vessels,
timed steps, and physical preparation. Context needed to decide whether a
supported outcome applies belongs to the reviewed catalogue rule. The UI keeps
that applicability premise and the representative/explanatory model disclosure
visible.

## Chemistry scope

The architecture supports molecular covalent structures (including localized
dative single bonds with donor-to-acceptor provenance), monatomic and
polyatomic ions, ionic assemblies, metallic domains, reviewed groups, and
atom-mapped transformations. Aromatic bonding remains outside the closed
domain.

Initial shipped chemistry is closed-world and fixture-led. Broader inorganic or
A-Level organic coverage is added only through explicitly AI-reviewed,
digest-pinned
structures and rules. Missing coverage is Unsupported; the agent does not fill
trusted-model gaps.

The first complete reaction is lithium with water because it exercises
covalent, ionic, metallic, electron-transfer, observation, and mapping
boundaries in one educational example.

Every registered element can be checked against elemental oxygen. The check
may select a representative oxide-family product, report no direct reaction,
or stop as ambiguous/unsupported. Compound oxidation is initially restricted
to compounds already defined by the structural catalogue. These checks do not
unlock animation until a reviewed structural transformation also exists.

## Canonical journey

1. **Ask.** The learner asks what happens when lithium reacts with water.
2. **Resolve.** The engine resolves identities and uniquely selects the AI-reviewed
   `AlkaliMetalWithWater` rule.
3. **Research.** The agent obtains typed qualitative observation claims and
   claim-level evidence.
4. **Author.** The agent writes concise `.chems 1` using catalogue identities
   and binds the reviewed rule.
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
- **Unsupported** — the catalogue lacks a reviewed identity, rule, state, or
  applicability premise.
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

1. ask about a supported reaction outcome;
2. distinguish pinned catalogue applicability from runtime AI-supplied observations;
3. inspect concise authored source and exact expanded structure;
4. follow stable atoms from reactants to products;
5. distinguish covalent, ionic, and metallic representations;
6. understand the structural conservation proof; and
7. receive an evidence-linked explanation.

No invalid, unsupported, incomplete, or stale value may reach either
simulation.
