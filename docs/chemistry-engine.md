# Chemistry engine and validator

## Trust model

The chemistry engine is a deterministic trusted kernel. The agent proposes a
catalogue-bound reaction statement and evidence-backed observations; it cannot
declare its own output valid.

A successful result means that one reviewed rule applies, source agrees with
that rule, deterministic expansion succeeds, every structural operation is
legal, every atom is mapped exactly once, system charge and explicit electrons
are conserved, and the final graphs equal the declared products. Because the
initial language requires representative/explanatory disclosures, that result
is `ValidatedWithAssumptions`.

It does not mean that ChemSpec proves every real-world outcome, a unique
mechanism, rate, energy, trajectory, bulk solution structure, or universal
safety.

## Two-stage decision

### Applicability

Before observation research and source validation, the engine resolves the
requested identities and searches reviewed reaction rules. The result is
`Likely`, `NoReaction`, `Unsupported`, or `Invalid/Ambiguous`.

Applicability belongs to the rule and may contain reviewed contextual premises
needed to identify the intended outcome. `.chems` itself contains no laboratory
recipe. Only a unique supported outcome proceeds.

### Structural validation

The validator resolves concise source and expands its selected rule. It then
checks language identity, catalogue digest, structures, coefficients, equation,
roles, evidence references, mapping, graph steps, electron state,
conservation, and final products.

## Trusted catalogue

The immutable catalogue contains:

```text
StructuralEntry
  stable identity and aliases
  formula summary and representation kind
  atom nodes, formal charge, non-bonding and unpaired electrons
  localized covalent edges
  ionic components and associations
  metallic site cores and delocalized electron domains
  reusable groups
  reviewed valence/electron premises

ReactionRule
  role schema
  reactant and product patterns
  applicability premises
  coefficient and instance expansion
  total atom-map template
  ordered structural-operation template
  model assumptions
  compatible observation predicates
  evidence and review metadata
```

Runtime agents may read these records but cannot modify them.

## Expansion boundary

`by apply RuleName` does not execute an agent-written proof script. It binds a
reviewed rule to declared source names. The engine deterministically expands:

- coefficients into stable instances;
- catalogue structures into stable atom IDs;
- the mapping template into a total atom map; and
- the operation template into exact typed operations with electron allocation.

Every derived value records its source or catalogue origin. Equivalent source
declaration order produces equivalent canonical HIR after semantic ordering.

## Structural operation semantics

Every operation is a pure transition from one immutable graph state to the
next. Failed preconditions return a diagnostic and no state.

- Covalent cleavage removes the exact expected edge and allocates its electrons
  homolytically or to one named endpoint.
- Covalent formation consumes exact available unpaired electrons.
- Bond-order change has explicit electron allocation.
- Ionic association changes membership without inventing electron sharing.
- Metallic release and join transfer ownership between site-local and
  domain-delocalized electrons explicitly.
- Electron transfer uses atom endpoints only and declares exact donor and
  acceptor post-states, including unpaired-electron counts.
- Product assignment changes final ownership without changing connectivity.

After every operation, the engine validates local arithmetic and reviewed
state support.

## Conservation

The engine proves:

- every mapped atom preserves element identity;
- atom mapping is total and bijective;
- system net charge is conserved, where atom-core formal-charge sum is reduced
  by one for every domain-owned delocalized electron;
- explicit valence electrons are conserved across atom-local, covalent, and
  metallic-domain ownership; and
- final transformed graphs equal catalogue product instances.

Charge conservation and electron conservation are recorded separately even
when one can be derived from other closed-system facts, because local electron
ownership is educational and operation-critical.

## Observation boundary

The agent supplies a typed evidence packet only after applicability succeeds.
Source observation statements reference packet claim IDs. Validation checks
subject, predicate, provenance, and rule compatibility, but observations never
participate in graph proof or mutate the catalogue.

## Derivation artifact

The engine returns a structured derivation rather than a Boolean:

```text
LithiumAndWater
  catalogue and evidence digests resolved
  AlkaliMetalWithWater applicability established
  source declarations and equation agree
  instances expanded deterministically
  atom map total, bijective, and element preserving
  every structural step replayed
  local valence, charge, radical, and electron states supported
  atoms conserved
  charge conserved
  explicit valence electrons conserved
  final graphs equal declared products
```

The derivation drives explanations, diagnostics, repair inputs, regression
tests, expanded-certificate inspection, and renderer frames.

## Result states

- **Validated** — every required premise and invariant is established without
  model assumptions; unreachable in the initial language.
- **Validated with assumptions** — structural validation succeeds with visible
  representative/explanatory model disclosures attached; this is the initial
  language's successful result.
- **Unsupported** — potentially legitimate chemistry lies outside reviewed
  structures, rules, or state premises.
- **Invalid** — source or derived structure contradicts the language or trusted
  premises.
- **System error** — the trusted bundle or runtime boundary is corrupt.

Unsupported chemistry is never presented as chemically false.
