# Generalized chemistry rules and structural templates

> **Status:** locked design, implemented through typed graph patterns and
> reactant automorphism support (G2). Generalized families, elaboration,
> migration, and authoring support remain queued as G3 through G6.
>
> This document defines the intended catalogue and elaboration architecture for
> generalized chemistry. The implemented `.chems 1` source grammar and concrete
> validation kernel remain authoritative until the implementation plan is
> completed and conformance fixtures are promoted.

## Decision

ChemSpec will represent reusable chemistry as reviewed, typed graph templates
and graph-rewrite rule families. It will not duplicate one complete mapping and
operation list for every member of a chemical family, and it will not ask a
runtime model to infer family membership, structures, or rewrites.

The generalized layer compiles a uniquely matched family rule into the existing
concrete values:

```text
concrete authored structures in .chems 1
  -> reviewed element, category, trait, and template resolution
  -> unique family parameter and case binding
  -> typed graph-pattern matching
  -> deterministic concrete graph and operation instantiation
  -> existing ExpandedStructuralReaction
  -> existing concrete validation kernel
  -> existing derivation and frames
```

The kernel never validates variables, predicates, or generic graph rewrites. It
continues to validate exact atom IDs, exact bonds, exact electron states, exact
operations, a total atom map, conservation, and exact product graphs.

## Goals

- Define every chemical element once using reviewed, proof-relevant facts.
- Derive objective element classifications instead of duplicating member lists.
- Retain explicit reviewed membership for classifications that are conventional
  or disputed.
- Define parameterized structural graphs once for genuinely isomorphic species.
- Define reaction families as typed graph matches and rewrites.
- Support explicit, disjoint cases where members of a family behave differently.
- Collapse symmetry-equivalent matches deterministically and reject chemically
  distinct ambiguous matches.
- Preserve the current trust, provenance, digest, validation, and renderer
  boundaries.
- Keep authored `.chems 1` free of generic parameters and proof-script syntax.

## Non-goals

- Runtime prediction from element names, periodic trends, or LLM knowledge.
- An unrestricted expression language inside catalogue JSON.
- A complete SMARTS or cheminformatics query implementation.
- Automatic mechanism discovery.
- Fractional, aromatic, multicentre, stereochemical, orbital, or coordination
  semantics not already admitted by the closed structural domain.
- Treating contextual reaction outcomes as intrinsic element attributes.
- Automatically trusting generated catalogue content.

## Four distinct kinds of catalogue knowledge

### Elements

An element record is the normalized source of truth for stable identity and the
small set of intrinsic fields used by reviewed predicates.

```jsonc
{
  "symbol": "Na",
  "name": "Sodium",
  "atomic_number": 11,
  "period": 3,
  "group": 1,
  "block": "s",
  "premise_ids": ["premise.element.sodium.identity"]
}
```

The record reuses the implemented `chem_domain::Element`, `ElementId`,
`ElementSymbol`, and `StaticElementRegistry` identities. The catalogue owns the
additional reviewed name, period, group, block, category, and premise data; it
does not introduce a second element identity system into `chem-domain`.

During G0–G4 the generalized element registry is optional so the existing
concrete catalogue remains executable. G5 migration makes registry resolution
mandatory for every element referenced by a promoted structure, formula,
template, pattern, or rule. G6 expands the registry from the reviewed migration
subset to all 118 named elements.

The initial registry contains all 118 named elements. It does not become a
general-purpose periodic-table database. Atomic mass, density, melting point,
electronegativity, electron configuration, and similar fields are added only
if a specified deterministic rule consumes them.

There is no single intrinsic `valence`, `oxidation_state`, or
`preferred_charge` field. Those are model- and context-sensitive reviewed
facts and remain in valence premises, structural templates, traits, or rule
cases.

### Element categories

An element category classifies element records. Membership uses one of two
closed forms:

1. a reviewed predicate over intrinsic element fields, with explicit includes
   and excludes; or
2. an explicit reviewed member set when the classification is conventional,
   disputed, or not expressible using the closed predicate language.

```jsonc
{
  "id": "Categories.AlkaliMetal",
  "subject": "element",
  "membership": {
    "kind": "predicate",
    "predicate": {
      "kind": "all",
      "predicates": [
        {"kind": "equals", "field": "group", "value": 1},
        {
          "kind": "not",
          "predicate": {"kind": "equals", "field": "symbol", "value": "H"}
        }
      ]
    },
    "include": [],
    "exclude": []
  },
  "premise_ids": ["premise.category.alkali-metal"]
}
```

The predicate language is a closed typed AST, not strings or executable code:

- `all`, `any`, and `not`;
- `equals` over enumerated scalar fields;
- inclusive integer `range`;
- `in_set` over one scalar field; and
- `present` for optional fields.

Every predicate node uses an explicit `kind`. Predicate fields are the closed
set `symbol`, `name`, `atomic_number`, `period`, `group`, and `block`. Scalar values
are strings or integers and must match the selected field type. A comparison
against an absent optional field evaluates false; `present` makes absence an
explicit constraint. Children of `all` and `any`, and values of `in_set`, are
semantically unordered and reject duplicates.

Unknown fields, ill-typed comparisons, empty logical nodes, conflicting
include/exclude overrides, and references to absent elements invalidate the
catalogue. A category whose final derived member set is empty is also invalid.
Derived membership is canonical and insensitive to record order.

An explicitly reviewed category remains possible:

```jsonc
{
  "id": "Categories.Metalloid",
  "subject": "element",
  "membership": {
    "kind": "explicit",
    "members": ["B", "Si", "Ge", "As", "Sb", "Te"]
  },
  "premise_ids": ["premise.category.metalloid.reviewed-convention"]
}
```

### Structural traits

A structural trait is a reviewed capability of a concrete structure or a
structure-template application. It is not an element category.

Each trait definition declares a nonempty map of named sites to the closed site
kinds `atom`, `covalent_bond`, `group`, `ionic_association`, and
`metallic_domain`. It may also declare named, typed graph projections. The
initial projection set is closed over atom element, formal charge, local and
unpaired electrons, covalent bond-order sum, exact covalent order and electron
origin between two atom sites, group size, ionic component count, metallic
site count, and metallic-domain electron count. Definitions and assertions
both carry premise IDs.

Projection scalars are closed by the projection kind: elements and bond orders
use their existing string spellings; covalent electron origin is `shared`,
`dative_left_to_right`, or `dative_right_to_left` relative to the definition's
two named atom sites; all counts and charges are integers.

Examples include:

- `Traits.ElementalMonovalentMetalDomain`;
- `Traits.ProticOH`;
- `Traits.DonorLonePair`;
- `Traits.CarbonylCarbon`; and
- `Traits.MonatomicCation(+1)`.

Traits may expose typed values and named structural sites used by patterns:

```jsonc
{
  "trait": "Traits.DonorLonePair",
  "sites": {"donor": "n"},
  "values": {"paired_electrons": 2},
  "premise_ids": ["premise.trait.ammonia.donor-pair"]
}
```

Trait validation resolves every named site against the complete graph and
checks the asserted value against its exact atom, bond, group, association, or
metallic-domain state. A free-form tag that is not structurally checked cannot
participate in rule applicability.

An assertion must provide exactly the sites and projected values declared by
its definition. Two assertions of the same trait on one structure are invalid.
The assertion values are exposed facts, not executable expressions.

### Contextual reaction facts

Facts such as “forms a peroxide in excess oxygen” are not element categories or
structural traits. They belong to a reaction case and its applicability premise.

This separation prevents permanent element tags from smuggling contextual
outcomes into supposedly intrinsic data.

## Parameterized structure templates

A structure template defines an exact structural graph containing typed
parameters. Initial parameter kinds are closed:

- `element`, constrained by element categories;
- `structure`, constrained by structural traits; and
- closed catalogue enums explicitly declared by the template.

No unbounded integer, float, text, formula, or arbitrary JSON parameter is
admitted.

The initial substitution forms are deliberately narrower than the parameter
kinds. An `element` parameter may replace an atom's element. A closed `enum`
parameter may replace a covalent bond order, and every declared enum value must
be a valid closed bond-order value at that site. A `structure` parameter names
an already validated concrete structure and checks its required trait set; it
is retained as argument provenance but is never graph-spliced into the
template. Labels, charges, electron counts, group membership, association
membership, domain membership, and domain electron counts remain literal.
Conditional template fields are not supported.

```jsonc
{
  "id": "Templates.ElementalAlkaliMetal",
  "parameters": {
    "M": {"kind": "element", "category": "Categories.AlkaliMetal"}
  },
  "representation": "metallic",
  "sites": [
    {
      "label": "metal",
      "element": {"parameter": "M"},
      "formal_charge": 1,
      "non_bonding_electrons": 0,
      "unpaired_electrons": 0
    }
  ],
  "domains": [
    {
      "label": "metallic",
      "sites": ["metal"],
      "delocalized_electrons": 1
    }
  ],
  "traits": [
    {
      "trait": "Traits.ElementalMonovalentMetalDomain",
      "sites": {"metal": "metal", "domain": "metallic"},
      "premise_ids": ["premise.template.elemental-alkali-metal"]
    }
  ],
  "premise_ids": ["premise.template.elemental-alkali-metal"]
}
```

A template application gives the instantiated graph a stable concrete identity
and authored aliases without duplicating its atoms and relationships:

```jsonc
{
  "id": "SodiumMetal",
  "template": "Templates.ElementalAlkaliMetal",
  "arguments": {"M": "Na"},
  "formula": "Na",
  "aliases": ["sodium"],
  "premise_ids": ["premise.structure.sodium-metal"]
}
```

The application is the concrete structure identity used by `.chems`. Template
syntax never appears in authored source. Applications are required because
they provide stable names, aliases, formulae, premises, and a unique product
identity. Their graph is derived and must equal the declared formula inventory.

Template application is total and deterministic:

1. resolve every parameter and constraint;
2. substitute typed element and enum values;
3. construct the complete graph through existing private domain constructors;
4. validate traits against that graph;
5. retain template, argument, application, and premise provenance; and
6. canonicalize and digest the concrete result.

If a family member needs a different graph, it uses another template or an
explicit concrete structure. Templates do not support conditional fields.

## Typed graph patterns

A graph pattern identifies the chemically relevant sites within a role-bound
concrete structure. Pattern variables are local to one rule case.

The initial closed pattern vocabulary supports:

- atoms with optional literal element, element-parameter, formal-charge,
  non-bonding-electron, unpaired-electron, and bond-order-sum constraints;
- shared covalent edges with exact order;
- directed dative edges with donor and acceptor variables;
- named atom-group membership;
- ionic-association component membership;
- metallic site/domain membership and exact domain electron count; and
- required checked structural traits and their exposed sites.

Patterns do not support arbitrary negation, recursive paths, ring syntax,
geometry, stereochemistry, or computed numeric expressions.

```jsonc
{
  "id": "Patterns.ProticOH",
  "variables": {
    "oxygen": {"atom": {"element": "O"}},
    "hydrogen": {"atom": {"element": "H"}}
  },
  "relationships": [
    {
      "kind": "covalent",
      "bond": "oh",
      "left": "oxygen",
      "right": "hydrogen",
      "order": "single",
      "electron_origin": "shared"
    }
  ],
  "premise_ids": ["premise.pattern.protic-oh"]
}
```

A match is an injective binding from pattern atom variables to concrete atom
IDs plus exact bindings for every referenced relationship. Every constraint is
checked against the immutable concrete graph.

Every relationship also declares a stable local binding name: `bond` for a
covalent edge, `group` for group membership, `association` for an ionic
association, or `domain` for metallic ownership. Trait-site requirements bind
their declared site kinds to atom or relationship binding names. Element
parameter references are typed inputs to matching; G2 never infers them, and
G3 supplies them only from a statically validated finite family domain.

## Match uniqueness, symmetry, and ambiguity

Naively requiring one raw graph match would make symmetric molecules unusable.
For example, the two hydrogen atoms in water produce two raw matches for one
O-H pattern even though the resulting chemistry is equivalent.

ChemSpec therefore uses this deterministic process:

1. enumerate raw injective matches in canonical atom-ID order;
2. instantiate the complete case rewrite for each match;
3. resolve the product identities and canonicalize each resulting concrete
   certificate under automorphisms of the role-bound reactant and product
   graphs, plus permutations of repeated indistinguishable instances, while
   preserving source roles and every explicitly constrained site;
4. collapse matches with byte-identical canonical instantiated certificates;
5. accept exactly one equivalence class;
6. return `Ambiguous` when two or more non-equivalent classes remain.

The implementation may use graph-isomorphism and automorphism optimizations,
but observable semantics are defined by canonical instantiated-certificate
equivalence. It may not choose the lexicographically first chemically distinct
site.

A rule may add a reviewed site constraint or require a checked trait to resolve
real regioselectivity. The runtime agent cannot choose a match by naming an atom
or editing the expanded certificate.

## Generalized reaction rules

A generalized rule owns:

- typed parameters and constraints;
- role schema and coefficients;
- reactant structure-template or trait constraints;
- product structure-template applications or exact identities;
- one or more unordered, explicitly supported or unsupported cases;
- per-case graph patterns;
- a total atom-correspondence template;
- an ordered typed rewrite template;
- applicability and model premises;
- observation compatibility; and
- complete premise provenance.

The rule is applied from ordinary `.chems 1` source:

```chems
by
  apply Rules.AlkaliMetalWithWater
    metal := sodium
    water := water
    hydroxide := sodiumHydroxide
    gasProduct := hydrogen
```

Parameters are inferred from role-bound concrete structures. Source cannot name,
override, or partially bind them.

### Cases

Cases express genuine family variation. They are unordered and must be disjoint
over the finite reviewed parameter domain.
Element categories, checked-trait structure indexes, and closed enum parameters
all produce finite domains from the validated catalogue.

Every generalized rule has at least one case. There is no implicit default. A
case predicate uses a separate closed typed AST over parameter identities:

- `always`;
- `all`, `any`, and `not`;
- `parameter_equals`; and
- `parameter_in_set`.

It cannot inspect graphs, invoke category predicates, compare arbitrary values,
or execute code. Category and trait restrictions belong to parameter
constraints; structural conditions belong to graph patterns.

A case is exactly one of:

- `supported`, with products, patterns, atom correspondence, rewrite,
  observation compatibility, and premise IDs; or
- `unsupported`, with a stable required-feature/domain-gap identifier, reviewed
  explanation, and premise IDs, but no product or rewrite payload.

For every possible binding in the finite Cartesian parameter domain, catalogue
validation checks:

- no two cases are simultaneously applicable; and
- every declared case is reachable by at least one binding.

Overlap or an unreachable case invalidates the catalogue rather than
introducing first-match order or dead reviewed data. A binding covered by no
case remains Unsupported.

```jsonc
{
  "id": "Rules.AlkaliMetalWithOxygen",
  "parameters": {
    "M": {"kind": "element", "category": "Categories.AlkaliMetal"}
  },
  "cases": [
    {
      "id": "lithium-oxide",
      "status": "supported",
      "when": {"kind": "parameter_equals", "parameter": "M", "value": "Li"},
      "rewrite": "..."
    },
    {
      "id": "sodium-peroxide",
      "status": "supported",
      "when": {"kind": "parameter_equals", "parameter": "M", "value": "Na"},
      "rewrite": "..."
    },
    {
      "id": "heavy-superoxide",
      "status": "unsupported",
      "when": {
        "kind": "parameter_in_set",
        "parameter": "M",
        "values": ["K", "Rb", "Cs"]
      },
      "required_feature": "Features.SuperoxideBonding",
      "explanation": "Superoxide is outside the current structural domain.",
      "premise_ids": ["premise.rule.heavy-alkali-superoxide"]
    }
  ]
}
```

The example does not admit superoxide chemistry to the current structural
domain. It records that a single unconditional element substitution would be
wrong and that the heavy-member case must remain Unsupported until its radical
and bonding representation is explicitly designed.

## Rewrite templates

A rewrite template is an ordered list of the existing closed structural
operations expressed over role instances, pattern variables, template-local
sites, and parameters.

It may:

- cleave or form shared covalent bonds;
- cleave or form directed dative bonds;
- change supported integral covalent order;
- associate or dissociate ionic components;
- release or join metallic-domain electrons;
- transfer exact electrons; and
- assign atoms to resolved product instances.

It may not create, delete, merge, split, or transmute atoms. It may not introduce
a relationship or electron state outside the existing domain constructors.

Each operation expands to an existing `StructuralOperationInput` with exact
atom IDs and exact before/after states. The existing constructor validates its
local ledger before the expanded reaction reaches the kernel.

## Product construction and identity

Products are not anonymous runtime graphs. Each product role resolves to one
exact concrete structure identity, either an explicit structure or a registered
template application.

The rewrite constructs a concrete final graph from conserved reactant atoms.
The existing atom map and final-product validation must prove that graph equals
the resolved product structure, including:

- element identity;
- shared versus dative electron origin and dative direction;
- integral bond order;
- ionic component membership;
- metallic-domain membership;
- formal charge and explicit electrons; and
- product assignment.

Thus a structure template reduces authored duplication but does not weaken final
graph equality.

## Deterministic elaboration

Given validated `.chems` source, a validated catalogue, and an evidence packet,
generalized elaboration performs exactly:

1. resolve concrete authored structure identities and aliases;
2. find rules whose role schema, representations, and applicability relation
   could match;
3. infer parameter values from bound structure applications and exact
   structures;
4. validate category and trait constraints with provenance;
5. select the unique applicable case, returning its reviewed Unsupported reason
   immediately when it is an unsupported case;
6. enumerate and canonicalize graph-pattern matches;
7. require one match-equivalence class;
8. resolve exact product structure identities;
9. instantiate coefficients, instances, atom correspondence, and ordered
   operations;
10. construct the existing concrete expanded certificate;
11. preserve every source, element, category, trait, template, application,
    rule, case, match, and premise origin; and
12. pass the concrete result to the unchanged validation boundary.

No step performs web access, model inference, floating-point ranking, random
selection, or mutation of the catalogue.

## Result classification

- `UnsupportedChemistry`: no reviewed parameter/case binding, a selected
  unsupported case identifies a required domain feature, or no pattern match.
- `AmbiguousChemistry`: multiple applicable rules, cases, parameter bindings,
  product identities, or non-equivalent pattern-match classes.
- `InvalidSource`: source role/product claims contradict the unique reviewed
  instantiation.
- `CorruptTrustedData`: a validated catalogue template, membership derivation,
  rewrite, or supposedly concrete instantiation violates its own invariants.

Ambiguity is not silently downgraded to Unsupported and never resolved by
catalogue record order.

## Trust, review, provenance, and digests

Every proof-relevant authored record carries premise IDs:

- element identity and intrinsic fields;
- category definition and explicit overrides;
- structural trait assertions;
- structure templates;
- template applications;
- graph patterns;
- family rules and cases;
- applicability; and
- observation compatibility.

Derived membership provenance contains both the element premise and category
premise. Template instantiation provenance contains the template, arguments,
application, trait, and all contributing premises. A concrete operation retains
the rule, selected case, matched sites, and rewrite premise dependencies.

The catalogue semantic digest covers all authored generalized records. Derived
indexes and caches are recomputable and do not create a second source of truth.
Changing a predicate, exception, member fact, template, trait, case, pattern, or
rewrite changes the digest. Reordering semantically unordered records does not.

The existing exact external review attestation remains the only promotion path.
An LLM may draft generalized records but cannot attest to them.

## Worked family: alkali metal with water

One family rule covers lithium, sodium, and potassium only when each concrete
role resolves through the reviewed category and structure applications.

```text
parameter M: Categories.AlkaliMetal

reactants
  2 Templates.ElementalAlkaliMetal<M> as metal
  2 Water as water

products
  2 Templates.AlkaliHydroxide<M> as hydroxide
  1 Hydrogen as gasProduct

case common
  status supported
  when always

  patterns
    metalSite := Traits.ElementalMonovalentMetalDomain
    proticBond := Patterns.ProticOH

  rewrite
    release one domain electron from each metalSite
    cleave one matched O-H edge per water toward oxygen
    transfer one released electron to each detached hydrogen
    form one shared H-H single edge
    associate each M+ component with its OH- component
    assign both hydroxides and hydrogen
```

Binding `SodiumMetal` infers `M = Na`; the declared `SodiumHydroxide` product
must resolve to the unique application of `Templates.AlkaliHydroxide` with the
same argument. Binding calcium cannot satisfy the category or metal-template
constraint and is Unsupported.

The two symmetric O-H choices in each water collapse only if their fully
instantiated certificates are equivalent under water graph automorphism.

## Worked dative rewrite

A donor/acceptor family may match checked traits instead of exact species:

```text
parameters
  D: structure with Traits.DonorLonePair
  A: structure with Traits.EmptyAcceptorSite

pattern
  donor := D.trait_site("donor")
  acceptor := A.trait_site("acceptor")

case supported-pair
  status supported
  when always

  rewrite
    FormDative(donor, acceptor)
```

The trait sites resolve to concrete atom IDs and exact electron states. The
rewrite expands to the existing `FormDative`; donor-pair accounting, product
direction, and frame direction remain concrete and proof-relevant.

## Architectural invariants

1. Authored `.chems 1` never carries generic parameters or atom selectors.
2. Every generic binding is induced by reviewed concrete role identities.
3. Category membership is deterministic and premise-backed.
4. Traits participating in applicability are structurally checked.
5. Template applications construct ordinary validated structure definitions.
6. Cases are unordered and non-overlapping over their advertised finite domain.
7. Pattern matching never selects among chemically distinct sites by order.
8. Rewrites compile only to the closed existing operation set.
9. The concrete kernel remains the sole chemistry-validation authority.
10. Runtime agents cannot author or mutate trusted generalized facts.
11. Any unsupported structural concept remains Unsupported.
12. Every semantic mutation invalidates the exact catalogue review digest.

## Migration boundary

Concrete structures and concrete rules remain valid catalogue records during
implementation so each slice can be verified against the existing canonical
journey. At final migration:

- the concrete lithium-only rule is replaced by the generalized family rule;
- repeated isomorphic graphs are replaced by template applications;
- exact concrete structures remain available for exceptions and unique species;
- authored `.chems 1` fixtures retain their surface syntax;
- expansion fixtures change to show inferred parameters, selected case, match
  equivalence, and generalized provenance; and
- no legacy generalized-rule format is retained because none has been released.

Implementation is governed by
[the generalized rules implementation plan](generalized-rules-implementation-plan.md).
