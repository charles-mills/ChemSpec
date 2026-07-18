# Structural `.chems` architecture

## Status

This document defines the product architecture implemented by the definitive,
unreleased `.chems 1` language. It does not introduce another language version
or compatibility path. Normative source details live in the
[language specification](chems-specification.md) and
[`grammar/chems.ebnf`](../grammar/chems.ebnf).

## Design objective

A valid reaction must contain enough reviewed information to drive two
synchronized educational views:

1. a qualitative observation view; and
2. a representative atom-mapped structural-change view.

The structural view preserves atoms, bonds, charge, explicit valence electrons,
ionic association, metallic-domain membership, operation order, and final
product membership. It does not claim molecular dynamics, bulk solution
structure, transition-state geometry, reaction rate, or a uniquely established
mechanism.

## Two representations, one language

Humans and agents author a concise rule-bound reaction:

```chems
reaction LithiumAndWater where
  reactants
    lithium := 2 of LithiumMetal
    water := 2 of Water
  products
    lithiumHydroxide := 2 of LithiumHydroxide
    hydrogen := 1 of Hydrogen
  equation
    2 Li[metallic] + 2 H2O[molecular]
    -> 2 LiOH[ionic] + H2[molecular]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.LithiumAndWater@1
    gas hydrogen evolves claim R1
    reactant lithium disappears claim R2
  by
    apply Rules.AlkaliMetalWithWater
      metal := lithium
      water := water
      hydroxide := lithiumHydroxide
      gasProduct := hydrogen
```

The selected reviewed rule expands this source into a complete certificate:

```text
resolved structures and coefficients
expanded labelled instances
total reactant-to-product atom map
ordered typed structural operations
immutable graph state before and after every operation
catalogue and evidence premises
model disclosures
source origins
```

The certificate is inspectable but not independently parseable source. This
keeps normal authoring concise while retaining exact atom-level transparency.

## Trust boundary

```text
request
  -> identity resolution and reviewed-rule applicability
  -> evidence-backed observations and concise .chems proposal
  -> parsing and catalogue resolution
  -> deterministic rule expansion
  -> structural validation
  -> ValidatedStructuralReaction
  -> paired renderer-independent frames
```

The catalogue owns structure and reaction truth. The agent cannot create
trusted structures, rules, mappings, transformations, valence states, or
applicability premises at run time. The validator cannot be configured by
source to omit checks. The renderer cannot invent chemistry.

## Structural identity

Formulae summarize composition but do not determine structure. Every resolved
structure has a stable catalogue identity and exactly one representation kind.

### Molecular and ionic graph structures

Molecules and polyatomic ions contain labelled atoms and localized single,
double, or triple covalent edges. Atom state includes formal charge,
non-bonding electrons, and unpaired electrons. Groups expand to reviewed atom
sets and never behave as pseudo-atoms.

### Ionic assemblies

Ionic assemblies contain charged atomic or polyatomic components and explicit
many-body association membership. Association is visually and semantically
different from a localized covalent edge. A representative formula unit does
not claim a permanent isolated pair in every physical environment.

### Metallic domains

Metallic structures contain positively charged site cores and explicitly owned
delocalized electrons. A finite reviewed fragment supports deterministic
explanation. Site-local and domain-owned electrons are mutually exclusive, so
release, join, and electron-transfer operations have exact accounting.
System charge is the sum of atom-core formal charges minus domain-owned
electron count; both that sum and the resulting net charge are retained in
every state ledger.

### Initial closed bond domain

The closed domain supports localized single, double, and triple covalent bond
orders. A dative bond uses the single order plus a directional donor-to-acceptor
electron-origin annotation. Formation consumes a paired electron pair from the
donor and none from the acceptor; cleavage names the pair allocation explicitly.
The annotation is preserved through canonical graph identity and frames for
explanation, while final valence and bond-order arithmetic remains that of a
single covalent bond.

Aromatic models remain Unsupported until their declaration, electron,
operation, validation, and rendering semantics are specified together. They
are not approximated silently.

## Rule architecture

A reaction rule is a reviewed, digest-bound template containing:

- reactant and product role schemas;
- structural identity and coefficient patterns;
- applicability premises owned by the catalogue;
- deterministic instance expansion;
- total atom-map template;
- ordered typed structural-operation template;
- model assumptions;
- compatible observation subjects and predicates;
- proof-relevant fact IDs and evidence; and
- review and version metadata.

The rule may encode the physical applicability needed to identify a supported
outcome without exposing laboratory quantities or instructions in `.chems`.
Ambiguous or uncovered requests stop before source validation.

## Structural operations

Expanded operations cover shared and dative covalent cleavage and formation,
covalent order change; ionic association and dissociation; metallic release and
join; atom-to-atom electron transfer; and product assignment. Each operation declares all
electron allocation and exact endpoint before/after electron states needed to
make its successor state deterministic. Allocation alone never determines
radical pairing.

Operations are applied in certificate order. The order is an explanatory
sequence unless the catalogue later supplies reviewed mechanistic evidence and
the language explicitly grows a mechanism model.

## Required validation

Before either simulation, the engine verifies:

- exact catalogue and evidence bundle identity;
- structure, group, rule, role, and claim resolution;
- declaration/equation agreement;
- deterministic expansion and source provenance;
- total bijective element-preserving atom mapping;
- every operation precondition against the preceding graph;
- supported valence, formal charge, non-bonding and unpaired electrons;
- ionic association and metallic electron-domain invariants;
- atom, charge, and valence-electron conservation;
- product assignment consistency; and
- final graph equality with every declared product.

Failure blocks both simulations. Missing reviewed coverage is Unsupported.

## Renderer contract

The renderer receives validated frames containing stable atom IDs, charge and
electron presentation state, covalent edges, ionic associations, metallic
domains, changed relationships, product membership, active operation, typed
observation stage, and explanatory disclosure.

Visual conventions remain distinct:

- one, two, or three solid lines for localized covalent order;
- a donor-to-acceptor arrow on a dative single bond while that provenance is
  educationally relevant;
- charge labels and dashed membership links for ionic association;
- lattice sites plus a delocalized field for metallic domains; and
- stable atom colour and identity across every frame.

Layout, interpolation, and playback speed are illustrative and cannot change
structural truth.

## Implementation sequence

The complete, fixed implementation sequence is defined in the
[archived `.chems` implementation plan](archive/plans/chems-implementation-plan.md):

0. definitive language and conformance contract;
1. structural domain;
2. structural frontend;
3. structural catalogue and reaction rules;
4. elaboration and deterministic expansion;
5. structural validation kernel; and
6. structural frames and conformance closure.

No additional language slices are implied by this architecture.
