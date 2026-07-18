# `.chems` language specification

## Status and authority

This document defines the intended end-state of the unreleased `.chems 1`
language. The normative grammar is
[`grammar/chems.ebnf`](../grammar/chems.ebnf). There is one source language, one
grammar, and one parser target. Earlier quantitative experiment syntax has no
compatibility status and is not part of the product contract.

Normative statements use **must**, **must not**, **required**, and **only**.
Examples are normative when explicitly labelled canonical; otherwise they are
illustrative.

## Language charter

`.chems` is a chemistry-native, agent-authorable, human-readable language for
stating the supported outcome of a reaction and binding that statement to a
reviewed structural reaction rule.

A source program states reactants, products, a display equation, explicit
theoretical-model disclosures, typed observation references, and one reviewed
rule application. The trusted engine expands that rule application into atom
instances, an atom map, and an ordered structural change sequence. Only the
expanded result is eligible for validation and simulation.

## Authority boundaries

The catalogue owns trusted structures, groups, valence premises, electron
premises, reaction applicability, product patterns, mapping templates,
structural-operation templates, model assumptions, and observation
compatibility.

The agent may author source using catalogue identities and may produce an
external evidence packet. It may not create trusted catalogue facts or weaken
kernel checks. Source declarations are claims until catalogue resolution,
expansion, and validation succeed.

The renderer receives a validated artifact. It must not parse source or infer
atoms, bonds, charges, associations, products, or reaction steps.

## Non-goals

`.chems` does not encode laboratory quantities, vessels, apparatus, physical
instructions, reaction duration, kinetic laws, force fields, transition-state
geometry, molecular dynamics, bulk lattice fidelity, or universal safety.

The ordered structural sequence is explanatory unless its reviewed rule
explicitly carries mechanistic evidence. The initial language always requires
`sequence := explanatory` and does not make mechanism claims.

## Compilation and validation model

Compilation has the following one-way stages:

```text
UTF-8 source
  -> lossless CST
  -> source AST
  -> catalogue-resolved reaction claim
  -> deterministic rule expansion
  -> typed structural HIR and expanded certificate
  -> immutable graph-state derivation
  -> ValidatedStructuralReaction
  -> renderer-independent frames
```

Invalid, unsupported, incomplete, stale, or system-error results cannot be
promoted into validated reactions or frames.

## Source unit

One UTF-8 `.chems` file contains exactly one language header, one catalogue
selection, and one reaction declaration. Leading, separating, and trailing
blank lines are permitted as defined by the normative grammar.

The language header is exactly `chems 1`. Other positive major numbers are
unsupported. A missing or malformed header is invalid source; the parser must
not guess a language version.

## Catalogue selection

`use catalog Name@Version` selects exactly one immutable catalogue bundle.
`Name` is a qualified name. `Version` contains one or more decimal integer
components separated by dots. Elaboration records both the selected version and
the validated semantic content digest.

Changing either the version or digest makes previous expanded, validated, and
frame artifacts stale.

## Reaction declaration

A reaction name is a type identifier and is local presentation identity. It
does not establish chemical identity.

The following sections occur exactly once and in this order:

1. `reactants`;
2. `products`;
3. `equation`;
4. `model`;
5. `observe from`;
6. `by`.

No source-local structure or reaction-rule declaration is permitted.

## Reactants and products

Each reactant and product declaration binds a value identifier to a positive
coefficient and a qualified catalogue structure identity:

```chems
lithium := 2 of LithiumMetal
```

Names are unique across both sections. Coefficients are exact positive
integers. They determine deterministic instance expansion using one-based
indices in declaration order, such as `lithium[1]` and `lithium[2]`.

Every declared structure must resolve in the selected catalogue and must have
exactly one representation kind: `molecular`, `ion`, `ionic`, or `metallic`.

## Equation

The equation is a presentation claim checked against resolved declarations. An
equation term contains an optional positive coefficient, a formula summary,
and a representation kind.

Formula equality is not structure equality. The equation must agree exactly
with the declared side coefficients, catalogue formula summaries, and
representation kinds after canonical term matching. Reordering terms does not
change meaning; changing a coefficient, formula, side, or kind does.

The arrow is `->`. Formula tokens contain no horizontal whitespace. An element
or parenthesized-group count may use either ASCII digits or the corresponding
Unicode subscript digits `₀` through `₉`. A count uses one digit style
throughout, is positive, and has no leading zero. Subscript counts normalize to
ASCII before formula comparison; the lossless CST retains the authored bytes
and byte spans, while the canonical formatter writes ASCII digits. Equation
coefficients, versions, and identifiers remain ASCII-only. Formula adducts may
use either ASCII `.` or the Unicode middle dot `·` as their separator; middle
dot normalizes to ASCII `.` under the same lossless-input and canonical-output
rules.

## Structural identity

A structural identity consists of a stable catalogue ID, formula summary,
representation kind, and representation payload. Equal formulae do not imply
equal identities or graphs. Catalogue identities distinguish structural
isomers.

### Atom nodes

An atom node has a stable local label, element, formal charge, non-bonding
electron count, unpaired-electron count, and optional reviewed presentation
metadata. Isotopes and stereochemistry are outside the initial closed domain.

`unpaired_electrons` must be non-negative and no greater than
`non_bonding_electrons`. Electron values are exact integers.

### Covalent structures

A molecular structure or polyatomic ion is an atom graph. A covalent edge joins
two distinct atoms and has order `single`, `double`, or `triple`. Duplicate
unordered endpoints and self edges are invalid.

### Dative covalent bonding

A dative bond is a localized `single` covalent edge whose electron-origin
annotation records a donor atom and an acceptor atom. It is not a fourth bond
order: after formation it contributes one to each endpoint's covalent
bond-order sum and owns two electrons exactly like any other single bond. The
direction records that both forming electrons came from the donor and is
retained for structural explanation and rendering. It does not imply a
permanent physical polarity or a different final bond strength.

Aromatic bonding remains unsupported rather than being silently approximated
as alternating localized edges. Later aromatic support requires declaration,
operation, electron, validation, and renderer semantics to be defined together.

### Groups

A group is a catalogue-defined named atom set within a structure template. It
may also identify its induced internal covalent subgraph. Expansion produces a
sorted, duplicate-free atom set. Groups are readable references, not
pseudo-atoms, and cannot conceal missing atoms or electron state.

### Ionic structures

An ionic structure contains charged atomic or polyatomic components and
many-body ionic associations. An ionic association is not a covalent edge and
does not own a localized electron pair.

The catalogue defines the smallest neutral component ratio where neutrality is
expected. A representative association describes structural outcome and
rendering membership; it does not claim a permanent isolated ion pair in every
bulk environment.

### Metallic structures

A metallic structure contains positively charged site cores and one or more
delocalized electron domains. Every delocalized electron is owned exactly once
by a metallic domain; it is not simultaneously counted as a site's local
non-bonding electron.

The catalogue supplies a finite deterministic fragment for representative
animation. A neutral lithium-metal site is therefore represented by a `Li+`
site core plus one domain-owned delocalized electron, not by a neutral `Li` atom
plus an additional electron.

## Electron and formal-charge model

The kernel tracks valence electrons explicitly. For an atom outside a metallic
domain, formal charge must satisfy:

```text
formal_charge
  = neutral_valence_electrons(element)
  - non_bonding_electrons
  - covalent_bond_order_sum
```

The bond-order sum counts `single` as 1, `double` as 2, and `triple` as 3.
Every covalent order owns two electrons globally and assigns one electron per
order to each endpoint for formal-charge calculation.

Unpaired state records how many non-bonding electrons are unpaired. The
catalogue valence premise determines supported combinations of element, formal
charge, bond-order sum, non-bonding electrons, and unpaired electrons. A state
that is arithmetically consistent but absent from the reviewed premise set is
`Unsupported`.

Total explicit valence electrons are the sum of atom-local non-bonding
electrons, twice every covalent bond order, and every domain-owned delocalized
electron. Structural operations preserve this total.

Charge ledgers distinguish the sum of atom-core formal charges from closed
system charge. A domain-owned electron contributes `-1`, so:

```text
system_net_charge
  = atom_formal_charge_sum
  - delocalized_domain_electron_count
```

Both values are recorded after every operation. It is invalid to report a
neutral metallic fragment merely by labelling its positively charged site
cores neutral; neutrality comes from the separately owned domain electrons.

## Structural operations

Operations exist only in expanded certificates. Source cannot directly author
or reorder them. Each operation carries its reviewed template origin and is a
pure transition from one immutable graph state to the next.

Every electron-changing operation carries exact reviewed `before` and `after`
endpoint states. An endpoint state contains `formal_charge`,
`non_bonding_electrons`, and `unpaired_electrons`; domain-changing operations
also contain the exact domain electron counts. Allocation describes where
electrons move, but never asks the kernel to infer their paired or unpaired
post-state. The kernel checks both the allocation arithmetic and every declared
post-state against the formal-charge equation and reviewed valence premises.

The initial operation set is closed:

- `CleaveCovalent(A, B, expected_order, Homolytic)` removes the edge and gives
  one electron per removed order to each endpoint, with exact endpoint
  post-states (single-bond cleavage therefore adds one non-bonding unpaired
  electron at each endpoint unless the reviewed template declares and proves a
  different supported pairing outcome);
- `CleaveCovalent(A, B, expected_order, Heterolytic(recipient))` removes the
  edge and gives both electrons per removed order to the named endpoint;
- `FormCovalent(A, B, order)` consumes one available unpaired electron from
  each endpoint per new bond order and creates the edge atomically, with exact
  endpoint post-states;
- `FormDative(donor, acceptor)` consumes one paired non-bonding electron pair
  from the donor, consumes no acceptor-local electron, and creates a
  donor-to-acceptor annotated single covalent edge with exact endpoint
  post-states;
- `CleaveDative(donor, acceptor, allocation)` removes that exact annotated edge
  and explicitly allocates the bonding pair homolytically or heterolytically;
- `ChangeCovalent(A, B, old_order, new_order, allocation)` performs a checked
  order decrease or increase with explicit allocation and exact endpoint
  post-states;
- `AssociateIonic(left_group, right_group)` creates a checked ionic
  association without moving electrons;
- `DissociateIonic(left_group, right_group)` removes that exact association
  without moving electrons;
- `ReleaseMetallic(site, domain, RetainElectron)` removes a site core and one
  domain electron, assigning the electron locally as unpaired;
- `ReleaseMetallic(site, domain, LeaveElectron)` removes only the site core and
  is valid only when the resulting charges and domain size match the template;
- `JoinMetallic(site, domain, DonateElectron)` moves one local unpaired electron
  into the domain and adds the compatible site core;
- `TransferElectron(donor_atom, acceptor_atom, count)` transfers available
  atom-local non-bonding electrons between atom endpoints and declares exact
  donor and acceptor post-states, including pairing; and
- `AssignProduct(atom_set, product_instance)` assigns conserved atoms to final
  product identity without changing graph or electron state.

Electron-transfer endpoints are atoms, never groups. Ionic operations accept
catalogue groups because their total component charge is deterministic.

Every precondition is evaluated against the immediately preceding state. A
failed operation produces a diagnostic and no successor state.

## Atom mapping

The selected rule owns a mapping template. Expansion instantiates a total,
bijective map from every reactant atom to every product atom. Mapped atoms must
have the same element identity. No atom may be missing, duplicated, merged,
split, or element-changing.

Product assignment is derived consistently with the mapping. An atom assigned
to a product different from its mapped destination is invalid.

## Rule-owned applicability

A reviewed rule defines its reactant pattern, product pattern, role schema,
applicability boundary, mapping template, operation template, supported model
assumptions, and observation compatibility.

Applicability may contain catalogue-owned physical context required to identify
the intended outcome. Such context is part of the rule's reviewed premise and
derivation, not authored laboratory procedure syntax. If the resolved request
does not uniquely select an applicable rule, the result is `Unsupported` or the
application requests disambiguation before source authoring.

The `by` block contains exactly one `apply` command. Bindings map every required
rule role exactly once to a declared reactant or product name. Unknown, missing,
duplicated, wrong-side, or wrong-kind bindings are invalid.

Applying a rule does not let source choose validation checks. All mandatory
kernel invariants always run.

## Generalized element and category domains

A generalized catalogue defines element identities once and assigns category
membership as reviewed data. Category parameters range only over explicit
members. Removing a member invalidates every stored template application that
binds that element; it cannot leave a structurally usable orphan.

Element facts, category membership, and their premises are digest-bound
catalogue content. A parameter outside its declared domain is Invalid when
stored in an application and Unsupported when requested without reviewed
coverage.

## Structure templates, graph patterns, and family rules

Parameterized structure templates construct exact concrete graphs from typed
arguments. Graph patterns select typed sites in concrete reactant instances. A
family rule binds parameters, selects one reviewed case, proves one complete
equivalence class of matches, and elaborates its correspondence and rewrite
into the existing concrete HIR.

Templates, applications, traits, patterns, cases, mappings, and rewrites are
validated before use. Generic selectors and parameter references must be fully
eliminated by elaboration; they are forbidden in validated reactions,
derivations, and frames.

## Model disclosures

The initial language requires:

```chems
model
  event := representative
  sequence := explanatory
```

`representative` states that coefficients expand into one illustrative
atom-mapped event and finite lattice fragments. `explanatory` states that
operation order is selected for teaching and is not asserted as an observed
elementary mechanism.

These disclosures are typed model assumptions and appear in every derivation,
artifact, frame sequence, and user-facing explanation. The initial language
therefore reports every successful structural reaction as
`ValidatedWithAssumptions`. `Validated` is reserved for a future derivation
whose complete proof has no attached model assumption and is unreachable in
the initial language.

## Typed observations and evidence

The `observe from` section selects one immutable evidence packet and contains
one or more typed observation statements. Each statement refers to a declared
reactant or product and exactly one claim ID in that packet.

The initial closed observation forms are:

- a declared gas product evolves;
- a declared reactant disappears;
- a declared product forms; and
- a declared product has a named colour.

Observation validation checks packet identity and digest, claim existence,
subject identity, predicate shape, source provenance, and compatibility with
the selected reaction rule. Observations do not alter structural graphs,
applicability, mappings, or operations.

Conflicting or insufficient evidence remains visible. It cannot be converted
into trusted catalogue fact during a run.

## Mandatory structural validation

The kernel always checks:

1. catalogue version, digest, and review eligibility;
2. structure, group, evidence, rule, and role resolution;
3. equation/declaration agreement;
4. deterministic instance, mapping, and operation expansion;
5. total bijective element-preserving atom mapping;
6. every operation precondition at its exact step;
7. supported valence, formal charge, non-bonding electrons, radicals, dative
   donor-pair provenance, ionic associations, and metallic-domain ownership
   after every step;
8. atom, total charge, and explicit valence-electron conservation;
9. product assignment consistency; and
10. equality between final transformed graphs and declared catalogue products.

No source syntax can omit, reorder, disable, or replace these checks.

## Generalized rules lower to concrete kernel operations

The kernel executes no generalized operation kind. A family rule must first
elaborate to concrete structure instances, atom mapping, and the same closed
structural operations used by a concrete rule. The kernel then revalidates
mapping, electron and charge conservation, metallic ownership, covalent and
dative direction, ionic membership, product assignment, and exact final
product graphs without consulting generic values.

Mutation of a family domain, selected case, match, rewrite, or product graph
must fail at the earliest boundary able to prove the mismatch. A request
outside reviewed family coverage remains Unsupported and cannot produce
frames.

## Result-state model

Every complete request ends in exactly one state:

- `Validated` — every required premise and invariant is established with no
  attached model assumption; unreachable in the initial language;
- `ValidatedWithAssumptions` — every structural invariant is established and
  displayed theoretical assumptions remain attached;
- `Unsupported` — legitimate chemistry or state may exist outside the reviewed
  closed domain;
- `Invalid` — source, bindings, mapping, operations, conservation, or products
  contradict the language or trusted premises; or
- `SystemError` — the selected trusted bundle or runtime boundary is corrupt or
  unavailable.

Parser-specific malformed and elaboration-specific incomplete results are
internal refinements and cannot reach simulation.

## Expanded structural certificate

The certificate is deterministic derived output containing:

- source hash and catalogue version/digest;
- resolved reactant and product structures;
- stable expanded instances and atom IDs;
- total atom mapping;
- ordered typed structural operations;
- immutable graph state before and after every operation;
- rule, structure, electron, valence, and evidence premise IDs;
- model disclosures;
- typed observation references; and
- exact source origins for every authored claim and binding.

The certificate has canonical serialization and a semantic digest. It is
inspectable through the application and CLI but is not accepted as an alternate
source grammar.

## Deterministic generalized-rule elaboration

A generalized certificate additionally records inferred parameter bindings,
the selected case, the number of equivalent complete matches, instantiated
structure applications, matched sites, and parameter/role premise provenance.
Canonicalization includes reactant/product graph automorphisms and repeated
coefficient-instance permutations. Equivalent matches produce one canonical
certificate; non-equivalent complete matches are Ambiguous.

## Validated artifact and frames

`ValidatedStructuralReaction` has private construction restricted to the
trusted kernel. It binds the source hash, catalogue digest, certificate digest,
derivation, and observation packet digest.

Frame generation is a pure projection of validated immutable states. Frames
preserve atom identity, element, formal charge, non-bonding and unpaired
electron labels where displayed, covalent edges, ionic associations, metallic
membership, changed relationships, product membership, active operation, and
model disclosure. Layout and time interpolation are presentation data and must
not affect chemistry.

Each typed observation has exactly one deterministic trigger operation and a
stage at every immutable state: `pending` when the state ordinal is less than
the trigger ordinal, `active` when they are equal, and `established` when it is
greater. A product observation (`evolves`, `forms`, or `colour`) triggers on
the final validated `AssignProduct` operation needed to assign every product
instance of its subject binding. A reactant `disappears` observation triggers
on the final validated `AssignProduct` operation needed to cover every atom in
every reactant instance of its subject binding. Incomplete or ambiguous trigger
coverage is a corrupt validated artifact and cannot produce frames.

Observation stages are explanatory synchronization markers. They do not
encode duration, kinetics, mechanism timing, concentration, perceptual delay,
or a claim that the corresponding physical observation is instantaneous.
Runtime evidence keeps its distinct `external_untrusted` trust label even when
the structural frame artifact was produced through the trusted catalogue and
kernel boundary.

## Lexical structure and normative grammar

The normative EBNF owns accepted token order. The lexer owns encoding,
comments, whitespace, and indentation tokens described here.

### File representation

Source is UTF-8. An optional leading UTF-8 BOM is accepted but omitted by the
formatter. NUL bytes, invalid UTF-8, non-ASCII source whitespace, and tabs are
lexical errors. Outside comments, non-ASCII source is limited to Unicode
subscript digits and the middle dot in formula notation; identifiers and
keywords are ASCII.

### Horizontal whitespace

ASCII space separates tokens and is otherwise insignificant outside comments.
Canonical indentation is two spaces per nesting level. Empty and comment-only
lines do not affect indentation.

### Comments

`--` begins a line comment ending before the logical newline. `/-` begins a
nested block comment and `-/` ends it. Block comments may nest and may contain
UTF-8 and newlines. Unterminated block comments are lexical errors.

Comments are retained losslessly and attached deterministically for formatting.

### Newlines and layout

CRLF and CR input normalize to logical `NEWLINE`; formatting emits LF. After a
newline, increased indentation emits `INDENT`, decreased indentation emits one
or more `DEDENT`, and inconsistent dedentation is invalid. EOF emits remaining
dedents followed by `EOF`.

### Identifier classes

Type identifiers begin with ASCII uppercase. Value identifiers begin with ASCII
lowercase. Claim identifiers begin with ASCII uppercase and continue with
uppercase letters, digits, or underscore. Qualified names contain dot-separated
identifier segments.

Identifiers are case-sensitive. Unicode confusables are not accepted.

### Reserved words

The following case-sensitive words cannot be used as identifier segments:

```text
apply
by
catalog
chems
claim
colour
disappears
equation
event
evolves
explanatory
forms
from
gas
has
ion
ionic
metallic
model
molecular
observe
of
product
products
reactant
reactants
reaction
representative
sequence
use
where
```

## Canonical formatting

Formatting uses LF, two-space indentation, one space between ordinary tokens,
no trailing whitespace, and one final newline. Sections remain in normative
order. Equation arrows may wrap only before `->` at the equation content
indentation level. Formatter output must parse and formatting must be
idempotent.

Incomplete or recovery-bearing source is not rewritten automatically.

## Diagnostics

Every diagnostic has a stable code, severity, primary byte span, concise
summary, chemistry-aware explanation, zero or more related spans, and optional
non-overlapping safe edits.

Namespaces are:

- `CHEMS-Lxxx` — encoding, lexing, comments, and layout;
- `CHEMS-Pxxx` — parsing and source shape;
- `CHEMS-Txxx` — names, roles, equations, and typed elaboration;
- `CHEMS-Cxxx` — catalogue, provenance, evidence, and rule bundles;
- `CHEMS-Xxxx` — deterministic expansion and mapping templates;
- `CHEMS-Kxxx` — graph operations and structural invariants;
- `CHEMS-Fxxx` — artifact and frame boundaries; and
- `CHEMS-Ixxx` — internal system failures.

Diagnostics sort by primary start byte, then severity, code, and stable emission
index. Invalid and Unsupported diagnostics must remain distinguishable.

## Stable identities and canonical serialization

All semantic identities are typed and deterministic. Source-local reaction and
declaration IDs derive from source identity and declaration path. Expanded
instance, atom, operation, mapping, and frame IDs derive from stable parent
identity and deterministic ordinal or catalogue-local labels.

Canonical JSON sorts object keys, emits deterministic array order defined by
each type, rejects binary floating point, and contains no insignificant
whitespace. Digests use lowercase hexadecimal SHA-256 over canonical UTF-8
bytes.

## Conformance contract

The executable conformance registry assigns every normative section to one
component and records independently authored positive, negative, metamorphic,
and property evidence.

At minimum, final conformance covers:

- encoding, comments, layout, parsing, and formatting;
- formula and structural identity;
- covalent, ionic, and metallic domain invariants;
- electron, radical, charge, and metallic-domain ownership;
- catalogue bundle, rule, evidence, and review validation;
- coefficient/instance, map, and operation expansion;
- every operation precondition;
- atom, charge, electron, mapping, and final-product validation;
- canonical certificate and derivation output;
- deterministic structural and observation frames;
- stale-artifact rejection; and
- honest Unsupported and Invalid results.

Expected chemistry artifacts must be independently authored, explicitly
AI-reviewed under the host trust policy, and must not be
generated by the implementation under test and accepted as their own oracle.

## Family-rule concrete outcomes

One reviewed family rule may cover several concrete member reactions only when
each member has reviewed category membership, required valence states, and
exact structure applications. Every invocation must independently reach its
member-specific final concrete graph. Sharing a rule never permits formula-only
product acceptance or inferred structures.

The initial conformance family covers lithium, sodium, and potassium reacting
with water through `Rules.AlkaliMetalWithWater`. Calcium is an out-of-domain
probe and remains Unsupported. Existing non-generalized concrete rules remain
executable as a compatibility path, but the migrated family fixture contains
no concrete lithium fallback.

## Generalized member frame projection

Frame projection remains private to validated chemistry in production. Tests
may project review-candidate derivations inside the kernel crate to compare them
with independently authored frame oracles. The catalogue-authoring compiler may
request the same internal projection only as a serialized-only
`ReviewCandidateFrameInspection`; that type cannot convert or dereference to
the trusted `SimulationFrames` renderer input. Every supported family member
must preserve its exact element labels, electron states, edges, ionic
components, product membership, operation sequence, and change sequence. No
generic parameter, selector, template, or match value may appear in a frame.

## Canonical complete source

```chems
chems 1
use catalog ChemSpec.Theoretical@1

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

The canonical source is a claim bound to reviewed identities and one reviewed
rule. It becomes trusted only after deterministic expansion and every mandatory
kernel check succeeds.
