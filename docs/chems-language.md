# The `.chems` language

## Purpose

`.chems` is a concise chemistry-native language for stating a supported
reaction outcome and binding it to a reviewed structural explanation. It is
readable by learners, predictable for agent generation, statically checkable,
and precise enough to drive atom-mapped animation.

The definitive language identifier is `chems 1`. The language has not been
released previously and has no compatibility grammar.

## Canonical example

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

The file makes a readable claim. The reviewed rule supplies the detailed
structures, applicability, instance expansion, atom map, electron allocations,
and structural operations. The validator checks that the claim and every
derived state agree.

## Surface principles

### Formula is not structure

Formulae are equation summaries. Catalogue identity and graph equality
distinguish compounds and structural isomers.

### Bonding models remain distinct

Covalent edges, ionic association, and metallic electron domains are different
domain values and receive different visual treatment. The engine never encodes
ionic or metallic bonding as convenient fake covalent edges.

### Applicability belongs to rules

`.chems` describes the selected outcome rather than a laboratory setup. The
catalogue rule owns the reviewed applicability context. If no rule applies
uniquely, the request is Unsupported or must be clarified before authoring.

### Detailed structure is derived, not hidden

The author does not repeat an atom map and operation list already owned by the
selected rule. The engine exposes a canonical expanded certificate containing
that detail for inspection, diagnostics, education, and renderer frames.

### Validation is mandatory

`by apply` binds one reviewed rule. It does not choose a subset of checks. The
kernel always validates mapping, operation preconditions, valence, formal
charge, explicit electrons, ionic and metallic invariants, conservation, and
final product equality.

### Observations remain evidence-backed claims

Observation statements are typed references into a separate immutable evidence
packet. They cannot change structures, applicability, or validation premises.

## Semantic layers

The implementation distinguishes:

1. source text and exact spans;
2. lossless syntax and source AST;
3. catalogue-resolved reaction claim;
4. expanded structural HIR and certificate;
5. immutable validated graph-state derivation; and
6. renderer-independent paired frames.

Only the trusted chemistry kernel constructs the validated layer.

## Editing contract

Source remains visible and editable. Editing invalidates the current expansion,
validation, and frames immediately. An old result may remain visible only as
explicitly stale. Agent patches are attributable, diffable, undoable, and may
not overwrite unrelated human edits silently.

Baseline tooling includes syntax highlighting, catalogue completion, hover
information, formatting, diagnostics, expanded-certificate inspection, and
simulation only after validation.

## Diagnostics

Diagnostics identify the exact declaration, equation term, evidence claim,
rule role, expanded atom, mapping, operation, or structural invariant that
failed. Invalid source and unsupported chemistry remain distinct.

## Further authority

- [Normative specification](chems-specification.md)
- [Normative grammar](../grammar/chems.ebnf)
- [Structural architecture](structural-chems-architecture.md)
- [Implementation plan](chems-implementation-plan.md)
- [Conformance contract](../conformance/README.md)
