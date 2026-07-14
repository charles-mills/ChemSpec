# Verification strategy

## Principle

ChemSpec's central invariant is:

> No invalid, unsupported, incomplete, or stale structural value can enter
> either simulation.

Expected chemistry artifacts are independently authored and must be reviewed
before promotion. The implementation under test never generates its own oracle
and accepts it as proof.

## Language verification

Fixtures cover every production in `grammar/chems.ebnf`, exact source spans,
encoding, tabs, NUL, BOM, comments, nested comments, layout, identifiers,
reserved words, formulae, equation wrapping, section order, rule bindings,
typed observations, recovery, formatting, and parse/format/parse round trips.

Discarded quantitative syntax is negative input, not a compatibility suite.

## Structural-domain verification

Property and table-driven tests cover:

- stable typed identities and canonical serialization;
- graph equality independent of declaration order;
- equal formula but unequal structural isomers;
- duplicate/self/unsupported covalent edges;
- dative donor/acceptor identity, donor-pair formation, explicit cleavage
  allocation, and stable canonical serialization;
- deterministic group expansion;
- ionic component charge and association membership;
- metallic site-core and delocalized-electron ownership;
- formal-charge arithmetic;
- non-bonding and unpaired-electron constraints; and
- atom-map primitives.

## Catalogue verification

Every catalogue build checks unique IDs and aliases, formula/graph consistency,
reviewed electron/valence states, group membership, ionic ratios, metallic
domains, reaction roles, applicability, product patterns, mapping templates,
operation templates, observation compatibility, provenance, review eligibility,
canonical ordering, schema version, and digest stability.

Semantic mutation changes the digest; record reordering does not.

## Expansion verification

Independently authored fixtures verify catalogue resolution, equation agreement,
role completeness, deterministic coefficient/instance expansion, total map
instantiation, operation instantiation, premise provenance, source origins, and
canonical certificate output.

Metamorphic tests reorder semantically unordered declarations and catalogue
records without changing the expanded result.

## Kernel verification

Every validated fixture is independently checked for:

- total bijective element-preserving mapping;
- operation preconditions at exact positions;
- covalent edge and order transitions;
- homolytic and heterolytic electron allocation;
- atom-local transfer availability;
- ionic association compatibility;
- metallic release/join electron ownership;
- supported formal charge, non-bonding, and unpaired states;
- atom, total-charge, and explicit-valence-electron conservation;
- product assignment; and
- final graph equality.

Negative fixtures mutate each independent premise. No failing transition emits
a successor state.

## Canonical chemistry fixture

Lithium and water is prepared for review as a representative explanatory event
covering:

- two lithium metallic site cores and their domain electrons;
- two water molecular graphs;
- explicit metallic release with electron retention;
- heterolytic O-H cleavage;
- electron transfer to hydrogen endpoints;
- H-H bond formation;
- lithium/hydroxide ionic association;
- complete atom mapping and product assignment; and
- gas evolution and reactant disappearance claims.

The chemist reviews the initial structures, every intermediate electron state,
the final structures, and the non-mechanism disclosure.

## Agent evaluation

A fixed corpus covers supported reactions, no reaction, unsupported chemistry,
identity ambiguity, incorrect assumptions, prompt injection, hazardous real-
world procedure requests, malformed evidence, malformed source, and bounded
repair.

Metrics include identity/rule selection, evidence completeness, first-pass parse
and validation, repair success, honest unsupported behavior, and time to first
visible event.

## Frame verification

Renderer-independent tests assert stable atom identity, exact graph snapshots,
distinct covalent/ionic/metallic frame values, correct electron and charge
labels, changed-relationship highlighting, product membership, observation
synchronization, deterministic restart, and presentation-only speed changes.

Visual tests check distinct bonding conventions and persistent explanatory
disclosure. GPU output never determines chemistry.

## Application verification

Fake providers and deterministic messages cover applicability outcomes,
provider unavailability/authentication/refusal/cancellation/timeout, malformed
evidence/source, bounded repair, stale asynchronous results, source edits,
paired playback, and post-playback overview.

Normal tests consume no subscription or API usage. Live checks are explicit
opt-in smoke tests.

## Slice and release gates

Each language slice must pass focused tests, workspace tests, formatting,
strict Clippy, warnings-as-errors documentation, conformance validation, diff
hygiene, and independent review appropriate to its boundary.

A releasable structural language additionally requires:

1. every bundled chemistry result matches its independent oracle;
2. every proof-relevant premise resolves to reviewed provenance;
3. all mapping, graph, charge, electron, and product invariants pass;
4. covalent, ionic, and metallic models remain distinct;
5. invalid, unsupported, incomplete, and stale values cannot reach frames; and
6. an independent final review reports no actionable findings.
