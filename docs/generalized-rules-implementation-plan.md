# Generalized rules implementation plan

> **Status:** fixed implementation queue for the locked
> [generalized chemistry design](generalized-chemistry-rules.md). G0 through G2
> are implemented and reviewed. G3 through G6 remain queued in the order below.

## Purpose

This plan migrates the reviewed chemistry catalogue from duplicated concrete
reaction rules to element-backed classifications, parameterized structures,
typed graph matching, and generalized graph-rewrite families.

It is separate from the completed `.chems 1` Slices 0–6. The authored grammar
does not change, the language major remains `chems 1`, and the existing concrete
kernel remains the validation authority.

There are exactly seven generalized-rules slices, numbered G0 through G6. They
are implemented in order from this fixed plan. No implementation agent may
split, merge, reorder, or invent slices.

## Mandatory implementation loop

For every slice:

1. Codex takes the next exact slice from the locked design and slice scope.
2. Codex implements only that slice and its prescribed fixtures/tests.
3. Codex runs the focused and repository-wide gates required by the slice.
4. Codex reviews the complete diff against the design.
5. An independent reviewer reports findings against semantics, trust, and
   regression boundaries.
6. All findings are fixed and re-reviewed until clean.
7. The slice is committed before the next slice begins.

If implementation exposes a design contradiction, work stops. Codex amends the
design and this fixed plan explicitly before Luna resumes. Luna must not resolve
the contradiction by inventing semantics.

The original G0 handoff remains as implementation history at
[`handoffs/generalized-rules-g0-luna.md`](handoffs/generalized-rules-g0-luna.md).

## Global constraints

- Do not change `grammar/chems.ebnf`, source AST, CST, formatter, or authored
  `.chems 1` syntax.
- Do not add a second validation kernel or a generic trusted-artifact type.
- Do not weaken private construction or trusted catalogue promotion.
- Do not add arbitrary expressions, executable predicates, user-authored atom
  selectors, or runtime model decisions.
- Do not infer chemical categories or structures from names.
- Do not represent unsupported fractional, aromatic, multicentre,
  stereochemical, or coordination chemistry approximately.
- Preserve concrete catalogue records until G5 completes migration.
- Preserve stable distinction among Invalid, Unsupported, Ambiguous, and
  CorruptTrustedData.
- Every new proof-relevant record requires premise provenance and digest
  participation in the same slice that introduces it.

## Slice G0 — element registry and category derivation

### Deliverables

- Add strict catalogue wire records for:
  - element identity and proof-relevant intrinsic fields;
  - element-category definitions;
  - closed typed category predicates;
  - explicit membership; and
  - include/exclude overrides.
- Add domain-safe values for period, group, block, and atomic number without
  introducing general periodic-property logic into `chem-domain`.
- Reuse `chem_domain::Element`, `ElementId`, `ElementSymbol`, and
  `StaticElementRegistry` as the element identity boundary; do not create a
  parallel catalogue element ID type.
- Validate:
  - unique element IDs, names, symbols, and atomic numbers;
  - valid field ranges and block values;
  - typed predicate operands;
  - nonempty logical nodes;
  - resolved explicit members and overrides;
  - disjoint include/exclude sets; and
  - every premise dependency.
- Derive canonical category membership indexes deterministically.
- Retain element and category premise provenance for every derived membership.
- Include all new records in canonical catalogue semantics and digesting.
- Add the minimal reviewed test registry required to exercise H, Li, Na, K, Ca,
  B, Si, and one transition element. Population of all 118 elements belongs to
  G6, not this slice.

### Acceptance

- `Categories.AlkaliMetal` derives Li, Na, and K and excludes H and Ca.
- An explicit conventional category resolves only its reviewed members.
- Reordering elements, categories, predicate children in canonical set-like
  nodes, and overrides cannot change derived membership or digest.
- Changing any intrinsic field, predicate, override, member, or premise changes
  the digest.
- Unknown fields, type-invalid comparisons, duplicate atomic numbers,
  unresolved members, and conflicting overrides fail as catalogue errors.
- Existing concrete catalogue JSON remains valid without element/category
  records during the migration window.

### Explicitly excluded

No structure templates, traits, graph patterns, generalized rules, matching, or
elaboration changes.

## Slice G1 — structural traits and template applications

### Depends on

G0 reviewed clean.

### Deliverables

- Add strict records for:
  - structural-trait definitions;
  - checked trait assertions with named sites and typed values;
  - structure-template parameters and constraints;
  - parameterized atoms, groups, covalent edges, ionic associations, metallic
    domains, and representation kind; and
  - stable concrete structure-template applications.
- Support only the parameter kinds and substitution forms locked in the design.
- Instantiate templates through existing private structural constructors.
- Check every trait site and value against the instantiated concrete graph.
- Resolve stable application IDs, aliases, formulae, arguments, and premises.
- Preserve template, argument, application, trait, and premise provenance.
- Include templates, applications, traits, and assertions in catalogue digests.
- Add initial templates and applications for elemental alkali metals and alkali
  hydroxides for Li, Na, and K.

### Acceptance

- `LithiumMetal`, `SodiumMetal`, and `PotassiumMetal` instantiate one metallic
  template into distinct exact graphs.
- Their hydroxides instantiate one ionic template with exact cation, hydroxide,
  shared O-H bond, charges, and component membership.
- Formula inventories equal instantiated graphs.
- Unknown parameters, missing arguments, wrong category arguments, self-bonds,
  invalid dative sites, broken trait sites, aliases colliding with concrete
  structures, and unsupported conditional template fields fail validation.
- A hand-authored concrete graph equal to a template application is structurally
  equal but remains a distinct catalogue identity unless explicitly migrated.
- Existing concrete structure lookup remains compatible.

### Explicitly excluded

No graph-pattern search, generalized reaction rule, case selection, operation
instantiation, or kernel change.

## Slice G2 — typed graph patterns and canonical matching

### Depends on

G1 reviewed clean.

### Deliverables

- Add the closed typed pattern record set defined by the design for atoms,
  shared and dative covalent edges, groups, ionic associations, metallic
  membership, domain electrons, and checked traits.
- Validate pattern variable uniqueness, reference resolution, endpoint kinds,
  constraint typing, premise dependencies, and unsupported constructs.
- Implement deterministic injective matching over one or more role-bound
  concrete graphs.
- Implement canonical raw-match enumeration.
- Define and expose a provisional match binding containing no trusted chemistry
  capability.
- Implement reactant-graph automorphism equivalence support required for later
  certificate quotienting; product-graph and repeated-instance equivalence is
  completed when product identities are available in G4.
- Add independent match oracles for:
  - one unique site;
  - symmetric water O-H sites;
  - a directed dative donor/acceptor edge;
  - an ionic component;
  - a metallic site/domain; and
  - two genuinely non-equivalent sites.

### Acceptance

- Pattern matching is invariant under atom, bond, group, and catalogue record
  declaration order.
- Matches never bind two atom variables to one concrete atom.
- Dative direction, group membership, association membership, and metallic
  ownership are exact constraints rather than inferred labels.
- Symmetric water sites are identified as automorphism-related raw matches.
- Genuinely distinct sites remain distinct.
- Unsupported pattern syntax is rejected by schema/deserialization rather than
  ignored.
- No matcher API can construct an expanded or validated reaction.

### Explicitly excluded

No family parameters, cases, rewrites, product construction, source
elaboration, or kernel execution.

## Slice G3 — generalized families, cases, and static validation

### Depends on

G2 reviewed clean.

### Deliverables

- Add generalized rule records for:
  - typed parameters;
  - element-category and structural-trait constraints;
  - role schemas and coefficients;
  - reactant patterns;
  - exact or template-applied product identities;
  - nonempty unordered supported/unsupported cases and the closed case-predicate
    AST;
  - total atom-correspondence templates;
  - ordered rewrite templates over pattern and trait sites;
  - applicability, model assumptions, observation compatibility, and premise
    provenance.
- Statically enumerate finite element-parameter domains.
- Treat element-category members, checked-trait structure indexes, and closed
  enum values as the only finite parameter-domain sources.
- Reject overlapping and unreachable cases; allow uncovered parameter bindings
  to remain Unsupported.
- Validate that every rewrite can instantiate only existing closed structural
  operation kinds.
- Validate total correspondence/product assignment shape at template level.
- Add one generalized `Rules.AlkaliMetalWithWater` record over Li, Na, and K.
- Add an oxygen-family design fixture whose heavy-superoxide case remains
  explicitly Unsupported because the current domain cannot represent it.
- Add a generalized donor/acceptor dative rule fixture.

### Acceptance

- Case order does not affect equality, canonical JSON, or digest.
- Two cases accepting Na invalidate the catalogue.
- Li, Na, and K each select exactly one supported water-reaction case; H and Ca
  do not satisfy the rule parameter constraint.
- A rule cannot advertise a heavy-superoxide case as supported.
- A selected heavy-superoxide case returns its typed Unsupported feature reason
  and carries no rewrite payload.
- A case matching no parameter binding invalidates the catalogue.
- Every parameter, role, pattern variable, product application, trait site,
  atom-correspondence endpoint, rewrite endpoint, and premise resolves.
- New generalized rules remain inert data: catalogue validation does not match
  source or execute rewrites.

### Explicitly excluded

No `.chems` resolution, parameter inference from source roles, concrete
operation expansion, kernel execution, or frame generation.

## Slice G4 — deterministic generalized elaboration

### Depends on

G3 reviewed clean.

### Deliverables

- Extend catalogue rule selection to consider concrete and generalized rules
  without source syntax changes.
- Infer parameters solely from concrete role-bound structure identities,
  template applications, and checked traits.
- Select the unique applicable case without record-order priority; return a
  selected unsupported case before graph matching.
- Run typed graph matching for the selected case.
- Instantiate the complete rewrite for every raw match.
- Canonicalize instantiated certificates under reactant and resolved-product
  automorphisms plus repeated indistinguishable-instance permutations, collapse
  equivalent matches, and require exactly one equivalence class.
- Resolve exact concrete product identities and applications.
- Expand the selected family into the existing concrete:
  - instances;
  - total atom mapping;
  - `StructuralOperationInput` values;
  - product assignments;
  - premise set;
  - provenance graph; and
  - `ExpandedStructuralReaction` certificate.
- Extend diagnostics for unsupported parameter bindings, missing cases,
  ambiguous cases, ambiguous product identities, no graph match, and multiple
  non-equivalent match classes.

### Acceptance

- Existing lithium `.chems 1` source expands through the generalized rule
  without syntax changes.
- Equivalent sodium and potassium source files expand through the same rule ID
  into exact member-specific atoms and products.
- Calcium with water is Unsupported before operation execution.
- Symmetric water matches collapse to one certificate-equivalence class.
- A deliberately asymmetric substrate with two matching sites returns
  Ambiguous rather than selecting the first atom ID.
- Dative trait sites instantiate exact directed `FormDative` or
  `CleaveDative` operations.
- Every derived value retains source plus element/category/trait/template/
  application/rule/case/pattern/rewrite premise provenance.
- Concrete legacy rules continue to elaborate during migration.

### Explicitly excluded

No new kernel operation, changed conservation rule, frame inference, authored
generic syntax, or trusted promotion.

## Slice G5 — concrete execution, migration, and conformance

### Depends on

G4 reviewed clean.

### Deliverables

- Execute generalized expansions through the existing concrete kernel without
  adding a generic execution path.
- Prove final graphs against exact product template applications.
- Confirm shared/dative direction, ionic membership, metallic ownership,
  electrons, charge, mapping, and product assignment remain proof-relevant.
- Migrate the canonical lithium rule from its concrete mapping/operation record
  to `Rules.AlkaliMetalWithWater`.
- Add independent sodium and potassium expansion/derivation/frame oracles.
- Replace the misleading lithium-specific family-rule fixture while retaining
  its stable authored `.chems 1` surface where possible.
- Promote generalized element/category/template/pattern/rule requirements into
  the conformance registry.
- Extend CLI inspection to show inferred parameters, selected case, match
  equivalence class, instantiated structure applications, and generalized
  premise provenance.
- Remove the concrete lithium-only rule after all migrated fixtures pass; do
  not retain a legacy generalized format.

### Acceptance

- Li, Na, and K water reactions validate through one family rule and yield
  exact member-specific concrete frames.
- Mutation of family membership, template graph, case, match, or rewrite causes
  a stable failure at the earliest correct boundary.
- Calcium remains Unsupported and cannot reach frames.
- A product application with the right formula but wrong graph fails final
  comparison.
- Shared and dative serialization compatibility remains intact.
- Existing non-generalized concrete chemistry still validates.
- No generic values cross into `ValidatedStructuralReaction` or
  `SimulationFrame`.

### Explicitly excluded

No bulk catalogue population, production attestation, provider work, UI work,
or chemistry outside the reviewed supported structural domain.

## Slice G6 — authoring compiler and Luna catalogue handoff

### Depends on

G5 reviewed clean.

### Deliverables

- Add a discoverable top-level catalogue-authoring command that assembles
  candidate shards into one deterministic catalogue envelope.
- Define a compact candidate package containing exactly:
  - `candidate.json` for elements, categories, traits, templates,
    applications, patterns, and generalized rules;
  - `example.chems` for one ordinary authored invocation; and
  - `evidence.json` for an explicitly untrusted example evidence packet.
- Validate schema, references, category derivation, template construction,
  traits, cases, matching, expansion, kernel execution, and frames in one
  candidate-check command.
- Generate rather than hand-author:
  - the merged catalogue envelope;
  - semantic digest;
  - expanded certificate;
  - derivation;
  - frames; and
  - pending human review request.
- Label generated certificates, derivations, and frames as candidate inspection
  outputs. They are not independent conformance oracles and cannot promote
  their own producing implementation.
- Document the exact Luna content prompt, prohibited edits, expected outputs,
  validation commands, and human chemist promotion boundary.
- Populate all 118 element identity records with only the fields required by
  the locked schema, using reviewed source provenance.
- Add the initial chemist-selected family queue without promoting unreviewed
  content.

### Acceptance

- Two candidate packages in different filesystem order produce the same merged
  canonical catalogue and digest.
- Duplicate IDs, aliases, element facts, applications, rules, or premises fail
  before merge.
- A Luna candidate cannot alter Rust, schemas, trust roots, validation options,
  or generated artifacts through the candidate package.
- Candidate checking reports Unsupported honestly for absent domain features.
- Generated review requests bind the exact digest and remain
  `pending-chemist-review`.
- Only an externally supplied exact human attestation can promote the generated
  catalogue through the existing trust API.

### Explicitly excluded

No automatic chemical review, LLM attestation, arbitrary web-derived runtime
catalogue mutation, or “best effort” generation of unsupported chemistry.

## Slice-specific Luna handoff format

Every handoff produced from this plan must contain:

```text
Objective
Authoritative design sections
Allowed files and crates
Required deliverables
Required positive fixtures
Required negative fixtures
Stable diagnostics or error classes
Explicit exclusions
Focused verification commands
Repository-wide verification commands
Stop condition
```

Luna may report a design contradiction with evidence. It may not broaden scope
or alter an exclusion.

## Repository-wide gates

Every slice runs, at minimum:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
cargo test --workspace --doc
cargo run -p chems-conformance -- validate
git diff --check
```

Schema- or conformance-owning slices additionally validate every changed JSON
fixture against its exact schema and run the relevant package-specific tests.

## Completion condition

The generalized-rules workstream is complete only when:

1. G0 through G6 are individually reviewed clean and committed;
2. one family rule validates Li, Na, and K water reactions end to end;
3. calcium and genuinely ambiguous sites are rejected at the specified
   boundaries;
4. dative direction survives generalized matching, instantiation, validation,
   and frames;
5. concrete exceptions remain supported without parallel legacy semantics;
6. candidate packages can be generated and checked without editing code;
7. catalogue content cannot promote itself;
8. all repository-wide gates pass; and
9. the external chemist can review one exact generated digest rather than a
   collection of unbound drafts.
