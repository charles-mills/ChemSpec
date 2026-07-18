# Luna handoff — Generalized Rules Slice G0

## Objective

Implement exactly Slice G0: the reviewed element registry, closed element
category definitions, deterministic category derivation, lookup indexes,
provenance dependencies, canonical semantics, schema, and tests.

Do not implement any part of G1 or later.

## Authoritative sources

Read completely before editing:

1. [`docs/generalized-chemistry-rules.md`](../../generalized-chemistry-rules.md),
   through **Element categories** and **Architectural invariants**.
2. [`docs/archive/plans/generalized-rules-implementation-plan.md`](../plans/generalized-rules-implementation-plan.md),
   especially global constraints and **Slice G0**.
3. The existing catalogue model, validation, normalization, digest, schema, and
   Slice 3 tests.

The design and plan outrank implementation convenience. If they contradict the
current code or each other, stop and report the contradiction. Do not invent a
resolution.

## Allowed files

Luna may modify only:

```text
crates/chem-catalogue/src/model.rs
crates/chem-catalogue/src/lib.rs
crates/chem-catalogue/tests/generalized_g0.rs
crates/chem-catalogue/README.md
schemas/chem-catalogue-1.schema.json
```

The test file may be created. No other file may be changed.

In particular, do not edit:

- `chem-domain`;
- `chems-lang`;
- `chem-kernel`;
- `.chems` grammar or fixtures;
- the canonical lithium catalogue or its pinned digest;
- conformance manifests;
- review attestations; or
- either authoritative design document.

## Exact wire contract

Add two optional fields to catalogue bundle schema 1 and
`CatalogueDocument`. Both default to empty and are omitted on serialization
when empty so the canonical lithium catalogue remains byte- and digest-stable:

```jsonc
{
  "elements": [],
  "element_categories": []
}
```

### Element record

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

Required fields:

- `symbol`: existing `chem_domain::ElementSymbol`;
- `name`: nonempty, equal to its trimmed value;
- `atomic_number`: `1..=118`, constructible as existing
  `chem_domain::ElementId`;
- `period`: integer `1..=7`;
- `block`: exactly `s | p | d | f`;
- `premise_ids`: nonempty unique set of existing premise IDs.

Optional field:

- `group`: integer `1..=18`; omitted for records where the reviewed source does
  not assign one in this model.

The pair `(atomic_number, symbol)` constructs the existing
`chem_domain::Element`. Use `StaticElementRegistry` as the identity consistency
boundary. Do not add another element ID type.

Element symbols, atomic numbers, and names are independently unique. Names use
exact case-sensitive equality after the trimmed-value check.

### Element category record

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
          "predicate": {
            "kind": "equals",
            "field": "symbol",
            "value": "H"
          }
        }
      ]
    },
    "include": [],
    "exclude": []
  },
  "premise_ids": ["premise.category.alkali-metal"]
}
```

Required fields:

- `id`: a typed declared ID whose wire value obeys the existing catalogue ID
  shape;
- `subject`: exactly `element` in G0;
- `membership`: one of the two variants below; and
- `premise_ids`: nonempty unique set of existing premise IDs.

Predicate membership:

```jsonc
{
  "kind": "predicate",
  "predicate": { /* ElementPredicateRecord */ },
  "include": ["optional", "element", "symbols"],
  "exclude": ["optional", "element", "symbols"]
}
```

`include` and `exclude` default to empty, serialize only when nonempty, are
semantically unordered, and must be disjoint.

Explicit membership:

```jsonc
{
  "kind": "explicit",
  "members": ["B", "Si", "Ge", "As", "Sb", "Te"]
}
```

`members` is nonempty, unique, semantically unordered, and fully resolved.

### Closed element predicate AST

Every node is internally tagged by `kind` and denies unknown fields.

```text
all       { predicates: nonempty unique list<ElementPredicateRecord> }
any       { predicates: nonempty unique list<ElementPredicateRecord> }
not       { predicate: ElementPredicateRecord }
equals    { field: ElementFieldRecord, value: ElementScalarRecord }
range     { field: ElementFieldRecord, min: integer, max: integer }
in_set    { field: ElementFieldRecord, values: nonempty unique scalar set }
present   { field: ElementFieldRecord }
```

Closed fields and types:

```text
symbol          string (ElementSymbol syntax)
name            string
atomic_number   integer
period          integer
group           integer, optional
block           string enum s|p|d|f
```

Rules:

- `equals` and `in_set` scalar types must match their field.
- `range` is legal only for `atomic_number`, `period`, and `group`.
- `min <= max` is mandatory.
- Comparison against an absent optional field returns false.
- `present(group)` reports whether a group is present; other fields are always
  present but remain legal.
- `all`/`any` predicate children and `in_set` values are semantically unordered
  and reject canonical duplicates.
- No regex, arithmetic, field-to-field comparison, user function, arbitrary
  JSON, or string expression is permitted.

## Derivation semantics

For each category:

1. Evaluate its predicate over every validated element, or take its explicit
   member set.
2. Add resolved `include` elements.
3. Remove resolved `exclude` elements.
4. Store the result as a canonical `BTreeSet<ElementSymbol>`.

The final member set must be nonempty.

Every derived membership must retain access to:

- the element record’s premise IDs; and
- the category record’s premise IDs.

It is sufficient in G0 to expose both through a small immutable provenance
value returned by lookup. Do not reuse kernel source provenance types and do
not create trusted chemistry capabilities.

## Validated catalogue API

Add immutable lookup APIs with these observable capabilities, using idiomatic
exact names if surrounding code demands a small naming adjustment:

```text
element(symbol) -> Option<&ElementRecord>
element_category(id) -> Option<&ElementCategoryRecord>
element_category_members(id) -> Option<&BTreeSet<ElementSymbol>>
element_is_member(symbol, category_id) -> Option<bool>
element_membership_provenance(symbol, category_id)
  -> Option<&ElementMembershipProvenance>
```

`None` means the element or category identity is absent. `Some(false)` means
both identities exist but the element is not a member.

Do not add Unsupported runtime variants in G0; no rule or template consumes
categories yet.

## Validation order and errors

Add stable catalogue error codes after the existing range:

```text
InvalidElement          CHEMS-C016
InvalidElementCategory  CHEMS-C017
```

Use existing errors where they already express the failure:

- duplicate global/category/element identity: `DuplicateId` (`CHEMS-C005`);
- missing element or premise reference: `UnknownReference` (`CHEMS-C006`);
- malformed element facts/ranges: `InvalidElement`;
- malformed predicate, type mismatch, empty logical node, duplicate canonical
  child/value, invalid range, or conflicting overrides:
  `InvalidElementCategory`.

Validation occurs after premise indexing and before valence premises,
structures, and rules. Existing concrete structures are not required to resolve
against the optional registry until G5.

## Canonical semantics and compatibility

- Normalize elements by `symbol`.
- Normalize categories by category ID.
- Normalize include/exclude/member sets by element symbol.
- Recursively canonicalize `all`/`any` children and `in_set` values.
- Reject duplicate canonical children instead of silently deduplicating them.
- Category derivation must be independent of element and category declaration
  order.
- Existing catalogues with both new arrays absent must deserialize, validate,
  serialize canonically, and retain their current digest.
- An explicitly present empty array and an omitted array have identical
  catalogue semantics.

## Required positive tests

Create `crates/chem-catalogue/tests/generalized_g0.rs`. Build test envelopes by
mutating or constructing values in the test; do not modify the canonical
lithium fixture.

Cover at least:

1. The legacy lithium catalogue validates with no generalized records and keeps
   its published digest.
2. A registry containing H, Li, Na, K, Ca, B, Si, and Fe validates.
3. Predicate `group == 1 && symbol != H` derives exactly Li, Na, K.
4. Ca returns `Some(false)` for alkali-metal membership.
5. An explicit category derives exactly B and Si in the minimal test data.
6. Every positive membership exposes the exact union of element and category
   premise dependencies without merging their identity.
7. Reordering all semantically unordered records and predicate children keeps
   canonical JSON and digest identical.
8. Mutating each intrinsic element field, category predicate, override,
   explicit member set, or premise dependency changes the digest.
9. Omitted versus explicitly empty generalized arrays have identical canonical
   semantics.

## Required negative tests

Cover at least:

- zero and greater-than-118 atomic numbers;
- period outside `1..=7`;
- group outside `1..=18`;
- invalid block;
- invalid symbol;
- blank, padded, and duplicate name;
- duplicate symbol or atomic number;
- missing and empty premise sets;
- unresolved premise IDs;
- unknown explicit, include, or exclude element;
- one element in both include and exclude;
- a predicate category whose final derived member set is empty;
- empty `all`, `any`, and `in_set`;
- duplicate canonical logical child and duplicate set value;
- scalar/field type mismatch;
- `range` over a string field;
- `min > max`;
- unknown predicate kind, field, membership kind, or extra property; and
- duplicate category ID.

Each test must assert the exact public `CatalogueErrorCode` where decoding
reaches semantic validation. Schema-only rejection is not sufficient for cases
representable by the Rust wire types.

## Documentation

Update only `crates/chem-catalogue/README.md` to describe the optional migration
registry and derived category index. State explicitly that templates, graph
patterns, and generalized rule application remain unimplemented after G0.

## Focused verification

```sh
cargo fmt --all -- --check
cargo test -p chem-catalogue --test generalized_g0
cargo test -p chem-catalogue --test slice3
cargo clippy -p chem-catalogue --all-targets -- -D warnings
```

## Repository-wide verification

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
cargo test --workspace --doc
cargo run -p chems-conformance -- validate
git diff --check
```

## Stop condition

Stop when all G0 deliverables and tests pass and only allowed files are changed.
Return:

- a concise implementation summary;
- the exact files changed;
- every command run and its result;
- any remaining uncertainty; and
- the complete diff ready for Codex and independent review.

Do not begin G1.
