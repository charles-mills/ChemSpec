# `.chems` implementation plan

## Authority

This document is the execution authority for the definitive structural
`.chems 1` language. The language has not been released, so the former
quantitative experiment design is replaced directly rather than retained as a
public legacy language or compatibility mode.

There are exactly seven implementation slices, numbered 0 through 6. They may
not be split, merged, renamed, reordered, or supplemented with additional
slices. Work discovered outside a slice is either required to satisfy that
slice's acceptance criteria or is deferred outside this language plan.

The normative source contract consists of:

- [`chems-specification.md`](chems-specification.md);
- [`../grammar/chems.ebnf`](../grammar/chems.ebnf);
- the requirement registry and fixtures under [`../conformance`](../conformance);
- the reviewed catalogue schemas; and
- the acceptance criteria in this plan.

## Product and trust boundary

`.chems` describes the supported outcome of a reaction and a representative
structural explanation. It is not a laboratory recipe, bulk-material model,
mechanism claim, molecular-dynamics input, or universal reaction predictor.

```text
reaction request
    -> reviewed catalogue identity and rule selection
    -> agent-authored concise .chems and evidence-backed observations
    -> deterministic rule application and structural expansion
    -> graph, mapping, step, atom, charge, and electron validation
    -> ValidatedStructuralReaction
    -> renderer-independent structural and observation frames
```

The agent may select catalogue identities, state the expected equation, bind a
reviewed rule, and reference observation claims. It may not introduce trusted
structures, rules, mappings, transformations, or validation premises at run
time. Applicability belongs to the reviewed rule. Unsupported chemistry remains
`Unsupported`, not false or guessed.

## Fixed authored and expanded forms

The normal authored form is concise and rule-oriented:

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

The selected rule deterministically expands coefficients into instances, atom
mappings, and typed structural operations. The expanded structural certificate
is human-readable and inspectable, but it is derived output rather than a
second authoring language. There is one grammar and one parser target.

## Slice completion loop

Every slice follows the same mandatory loop:

1. Select or add independently authored conformance evidence.
2. Implement the complete slice and only that slice.
3. Run formatting, focused tests, workspace tests, strict lint, documentation,
   conformance validation, and diff hygiene in proportion to the slice.
4. Send the complete slice to an independent sub-agent for review.
5. Fix every actionable finding.
6. Request re-review and repeat until the reviewer explicitly reports clean.
7. Record the exact verification boundary, then begin the next slice.

A slice is not complete merely because it compiles. No later slice begins while
an earlier slice has an unresolved review finding.

## Reuse and retirement boundary

Existing implementation is retained only where its semantics remain correct:

- keep exact formula, element, charge, identifier, canonical-serialization,
  source-span, lossless-token, diagnostic, catalogue-digest, provenance, and
  review infrastructure;
- rebuild syntax trees, HIR, catalogue records, and validation logic where they
  encode the discarded quantitative language; and
- remove units, quantities, conditions, materials, vessels, inventories,
  physical procedures, and stage ledgers from the definitive language surface.

Repository history remains available for archaeology. It is not a supported
runtime compatibility path.

## Slice 0 — definitive language and conformance contract

### Depends on

Nothing.

### Deliverables

- Rewrite the language overview, specification, architecture, chemistry-engine,
  workflow, verification, and product documents around structural `.chems 1`.
- Replace the normative grammar in `grammar/chems.ebnf`; remove alternate
  structural grammar files and compatibility framing.
- Define exact authored syntax, formatting, comments, identifiers, catalogue
  references, reaction declarations, rule bindings, typed observations, and
  evidence references.
- Define the expanded certificate independently of source syntax.
- Define atom, group, covalent, ionic, metallic, formal-charge, radical,
  non-bonding-electron, and delocalized-electron semantics.
- Define dative support consistently across structure records, operations,
  validation, and rendering; explicitly exclude aromatic bonding from the
  closed first domain.
- Define rule-owned applicability, deterministic coefficient/instance
  expansion, mapping templates, operation templates, and model assumptions.
- Define mandatory validation; source cannot select or omit kernel invariants.
- Replace requirement IDs, reserved words, manifest entries, schemas, and
  fixtures to describe the new language.
- Provide one complete lithium-and-water authored fixture, independently
  reviewed expanded certificate, expected diagnostics, and observation packet.

### Acceptance

- Every normative statement has a stable requirement ID and conformance owner.
- Every grammar production is defined and reachable; every keyword is reserved.
- The canonical authored fixture parses according to the published grammar.
- Its expanded certificate accounts for every reactant and product atom exactly
  once and declares the electron state before and after every operation.
- Catalogue applicability is sufficient to choose the rule without embedding
  laboratory quantities or procedures in source.
- No document claims implemented support that the current code does not have.
- Conformance validation reports the structural plan honestly even before later
  implementation slices satisfy it.

### Explicitly excluded

No structural Rust domain implementation or parser migration.

## Slice 1 — structural domain

### Depends on

Slice 0 reviewed clean.

### Deliverables

- Stable typed IDs for structures, atoms, groups, bonds, associations, metallic
  domains, instances, rules, operations, mappings, evidence, and claims.
- Atom nodes with element, formal charge, and explicit local electron state.
- Covalent edges with the closed supported order registry and optional dative
  donor-to-acceptor electron-origin annotation.
- Deterministic group expansion to atom sets.
- Ionic components and associations without fake covalent edges.
- Metallic sites and explicit ownership of delocalized electrons.
- Immutable structural graphs and reaction-side instance collections.
- Total typed atom mappings and structural-operation values.
- Canonical ordering, serialization, graph equality, and content digests.

### Acceptance

- Invalid/self/duplicate edges, invalid groups, incompatible associations, and
  inconsistent electron ownership cannot construct valid domain values.
- Graph equality is independent of declaration order while preserving chemical
  identity and bond semantics.
- Equal formulae do not make structural isomers equal.
- Property tests cover mapping bijection primitives, graph canonicalization,
  group expansion, charge/electron accounting, and serialization stability.

### Explicitly excluded

No `.chems` parsing, catalogue loading, rule application, or graph execution.

## Slice 2 — structural frontend

### Depends on

Slice 1 reviewed clean.

### Deliverables

- Migrate `chems-lang` to the sole structural `chems 1` grammar.
- Preserve encoding validation, lossless tokens, nested comments, source spans,
  recovery diagnostics, comment attachment, and canonical formatting.
- Replace source AST nodes with authored reactions, reactants, products,
  equations, model declarations, typed observation references, and rule
  applications.
- Add exact malformed-source diagnostics and safe edits.
- Reject discarded quantitative syntax without invoking a compatibility parser.

### Acceptance

- Every normative production is exercised by conformance fixtures.
- Parse/format/parse is lossless in meaning and canonical formatting is
  idempotent.
- The canonical authored fixture has independently authored CST and AST oracles.
- Arbitrary bytes and UTF-8 input do not panic.
- Diagnostic codes and byte spans are stable.

### Explicitly excluded

No catalogue resolution, chemical rule application, or structural validation.

## Slice 3 — structural catalogue and reaction rules

### Depends on

Slice 2 reviewed clean.

### Deliverables

- Replace catalogue records and schemas with reviewed structures, groups,
  electron premises, observation compatibility facts, applicability rules,
  product patterns, atom-map templates, and operation templates.
- Validate dative structure annotations and donor-pair formation/cleavage
  operation templates without treating dative bonding as a fourth bond order.
- Preserve deterministic bundle validation, canonical serialization, digests,
  provenance, evidence eligibility, review attestations, and lookup indexes.
- Implement the closed lithium, water, lithium-hydroxide, and hydrogen
  structures and the `AlkaliMetalWithWater` rule.
- Make every proof-relevant premise resolvable by stable fact ID.

### Acceptance

- Runtime agents cannot add or mutate trusted catalogue facts.
- Corrupt structures, templates, mappings, applicability metadata, evidence, or
  review state fail as typed system errors.
- The canonical catalogue and rule are independently chemistry-reviewed.
- Semantic mutation changes the catalogue digest; record-order changes do not.
- Unsupported identities and rules remain distinct from invalid bundles.

### Explicitly excluded

No source elaboration or graph execution.

## Slice 4 — elaboration and deterministic expansion

### Depends on

Slice 3 reviewed clean.

### Deliverables

- Resolve catalogue versions, structures, evidence packets, rules, bindings,
  equation terms, reactant/product counts, and model declarations.
- Validate rule applicability against resolved reaction identities.
- Deterministically expand coefficients to stable labelled instances.
- Instantiate the reviewed atom-map and structural-operation templates.
- Produce typed structural HIR, a declaration-order-invariant semantic
  certificate with exact premise dependencies, and a separate physical
  provenance report with filenames and byte spans.
- Distinguish invalid source, unsupported chemistry, and corrupt trusted data.

### Acceptance

- Equivalent declaration order produces equivalent typed HIR and certificate.
- Equation coefficients, bound instance counts, rule patterns, mappings, and
  products agree exactly.
- Every expanded atom, operation, model assumption, observation, and premise is
  traceable to source or catalogue provenance.
- The canonical source expands to exact independently checked semantic JSON,
  certificate, and physical-provenance oracles.

### Explicitly excluded

No execution of structural operations and no construction of a validated
reaction.

## Slice 5 — structural validation kernel

### Depends on

Slice 4 reviewed clean.

### Deliverables

- Execute every typed structural operation as an immutable graph transition.
- Enforce exact step preconditions for shared covalent, dative covalent, ionic,
  metallic, and electron operations.
- Validate supported valence, local electron availability, formal charge,
  radicals, association compatibility, and metallic electron ownership after
  every step.
- Validate total element-preserving atom mapping.
- Prove atom, total charge, and electron conservation.
- Compare final transformed graphs with declared product graphs.
- Produce a structured derivation and privately construct
  `ValidatedStructuralReaction` only after every invariant passes.

### Acceptance

- The canonical fixture validates to its independently authored derivation.
- Negative fixtures cover every operation precondition and conservation class.
- Removing, duplicating, remapping, or changing any atom/electron/bond premise
  cannot reach validated output.
- Source edits or catalogue-digest changes make previous output stale.
- No renderer or application API can construct validated chemistry directly.

### Explicitly excluded

No UI layout, motion, kinetics, trajectory, or mechanism inference.

## Slice 6 — structural frames and conformance closure

### Depends on

Slice 5 reviewed clean.

### Deliverables

- Convert validated immutable graph states into deterministic,
  renderer-independent structural frames.
- Preserve stable atom identity, charge/electron labels, covalent edges and
  dative donor direction, ionic associations, metallic membership, changed
  relationships, active operation, model disclosure, and product membership.
- Define typed observation stages and deterministic synchronization with the
  structural sequence.
- Add authored-source and expanded-certificate CLI inspection.
- Complete structural language, domain, catalogue, elaboration, kernel, frame,
  diagnostic, and artifact conformance coverage.
- Remove remaining definitive-product references to the discarded quantitative
  language while preserving unrelated application work.

### Acceptance

- Restarting frame generation yields byte-identical semantic frames.
- Presentation speed or layout cannot change chemistry.
- Every frame is traceable to a validated graph state and active operation.
- No invalid, unsupported, incomplete, or stale value reaches frame generation.
- Full workspace formatting, tests, strict Clippy, warnings-as-errors
  documentation, conformance validation, and diff hygiene pass.
- Independent final review reports no actionable findings.

### Explicitly excluded

No Iced/wgpu renderer implementation, provider implementation, catalogue breadth
beyond reviewed closed-world fixtures, molecular dynamics, or real-world
laboratory simulation.

## Completion condition

The structural `.chems 1` language is complete when Slice 6 is reviewed clean
and all final gates pass. Broader chemistry, application rendering, provider
integration, and additional catalogue content are separate product work, not
additional language slices.

Reusable element categories, parameterized structure templates, typed graph
patterns, and generalized reaction-family rewrites are governed by the
separate [generalized rules implementation plan](generalized-rules-implementation-plan.md).
That forward work compiles to this plan's concrete expansion and kernel values
and does not reopen the authored `.chems 1` grammar.
