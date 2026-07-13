# The `.chems` language

## Purpose

`.chems` is a first-class, chemistry-native language for declaring virtual
experiments and expected outcomes. It is designed to be:

- familiar to people who read chemical equations;
- concise and comfortable for human authors;
- predictable for agent generation;
- statically checkable;
- readable in source control;
- capable of producing precise, educational diagnostics.

The design is inspired by Lean's separation of propositions and proof, without
claiming that empirical chemistry becomes a purely mathematical theorem.

The authoritative design is the
[`.chems` language specification](chems-specification.md). This document is a
short product-oriented introduction to that normative contract.

## Canonical example

```chems
chems 1
use catalog ChemSpec.Aqueous@1

experiment SilverChloridePrecipitation where
  conditions
    temperature := 25 degC
    pressure := 1 atm
    medium := aqueous

  given
    silverNitrate := 50 mL of 0.100 mol/L AgNO3(aq)
    sodiumChloride := 50 mL of 0.100 mol/L NaCl(aq)

  vessels
    reaction := open vessel 250 mL

  procedure
    place silverNitrate in reaction
    mixed: add sodiumChloride to reaction
    stir reaction

  expect at final
    class := precipitation
    produces AgCl(s)

    molecular := AgNO3(aq) + NaCl(aq) -> AgCl(s) + NaNO3(aq)

    completeIonic := ?
    netIonic := ?
    amount AgCl(s) := ?

    observe
      precipitate AgCl(s)
      colour := white

  by
    dissociate aqueous
    infer products using solubilityRules
    balance molecular
    derive completeIonic
    cancel spectators
    solve stoichiometry
    verify atoms
    verify charge
    prove observations
    close
```

## Surface-language principles

### Chemistry is syntax, not strings

Formulas, ionic charges, phases, quantities, concentrations, and units are
parsed as typed language constructs. They are never opaque strings passed to
the model or renderer.

### Claims are explicit

The `expect` block contains the author or agent's claims. These claims are not
trusted because they appear in a file.

### Proof tactics request deterministic work

The `by` block selects bounded validation tactics. A tactic such as
`infer products using solubilityRules` asks the trusted kernel to apply a rule family; it
does not allow the source author to assert that the rule succeeded.

### Empirical facts come from the catalogue

`use catalog ChemSpec.Aqueous@1` establishes the versioned empirical universe
against which the program is interpreted. A `.chems` program cannot inject new
solubility, colour, dissociation, or hazard facts into that catalogue.

### Source remains keyboard friendly

Canonical source uses forms such as `degC`, `->`, `Ag^+`, and `NO3^-`. The
application may render these typographically as `°C`, `→`, subscripts, and
superscripts without changing the saved source.

## Semantic layers

The implementation should distinguish:

1. **Source text** — bytes and source spans.
2. **Syntax tree** — language structure, including unresolved names.
3. **Resolved experiment** — names and units resolved against a catalogue.
4. **Validated experiment** — supported claims and derivation artifact.

Only the chemistry engine may construct the final layer.

## Normative contract and implementation status

The only language grammar is [`grammar/chems.ebnf`](../grammar/chems.ebnf).
There is no legacy grammar or migration surface. `chems-lang` implements the
complete source frontend against that grammar.

The executable specification and source-tooling commands are:

```text
cargo run -p chems-conformance -- validate
cargo run -p chems-conformance -- report
cargo run -p chems-lang -- parse experiment.chems
cargo run -p chems-lang -- format --check experiment.chems
cargo run -p chems-lang -- format --write experiment.chems
```

`validate` checks the requirement registry, manifest, fixture paths, grammar
reachability, reserved words, and schema documents. `report` additionally exits
non-zero until all normative requirements are covered by conformance cases.
`chems-lang` owns lossless source syntax, `CHEMS-L`/`CHEMS-P` diagnostics,
comment attachment, and formatting. Catalogue resolution and the kernel remain
separate trusted boundaries as fixed by the specification.

## Derived values

Values that the validator can derive reliably should not be duplicated in
source by default. Examples include:

- limiting reagent;
- consumed and remaining moles;
- theoretical product quantity;
- molar mass calculated from accepted elemental data.

An author may explicitly assert a derived value for teaching or checking, in
which case disagreement becomes a diagnostic.

## Diagnostics

Diagnostics attach to exact source spans and explain the chemical issue:

```chems
netIonic :=
  Ag^+(aq) + Cl^2-(aq) -> AgCl(s)
              ^^^^^
```

```text
CHEM-E023 — Charge is not conserved

Reactant charge: -1
Product charge:   0

Did you mean Cl^-(aq)?
```

Required diagnostic properties:

- stable code;
- severity;
- primary source span;
- concise summary;
- chemistry-aware explanation;
- optional related spans;
- optional safe replacement;
- machine-readable representation for the agent repair loop.

## Editing contract

`.chems` is the experiment's source of truth. The editor supports human and
agent authors equally.

Required editor states:

- unmodified and validated;
- modified, validation pending;
- validating;
- validated with or without assumptions;
- invalid;
- unsupported.

Editing validated source immediately makes the current simulation stale. The
previous result may remain visible in a paused, dimmed state, but it must be
labelled as the last validated version. Invalid source never drives the
renderer.

Baseline editing features:

- syntax highlighting;
- automatic indentation;
- completion for formulas, phases, charges, quantities, and units;
- inline diagnostics;
- hover information for catalogued substances;
- format document;
- validate;
- simulate when valid;
- open and save ordinary `.chems` files.

Agent edits are patches. During an automatic repair loop, patches may apply
without another confirmation, but they remain visible, attributable, and
undoable. An agent must never silently overwrite unrelated human edits.

## Source-to-simulation linking

Where practical, the application connects the same concept across views:

- selecting `Ag^+(aq)` highlights representative silver ions;
- selecting `AgCl(s)` highlights the precipitate;
- selecting `netIonic` isolates participating species;
- selecting `dissociate aqueous` reveals the corresponding derivation step.

This connection makes the language an educational representation rather than
an implementation detail.

## Evolution rules

The initial language should remain deliberately small. New syntax is justified
only when it represents a chemistry concept that cannot be expressed clearly
with existing constructs. Catalogue data and UI preferences do not belong in
the language grammar.

Future versions may add redox half-equations, equilibria, weak acid/base
systems, kinetics, or organic mechanisms, but those additions must be versioned
and must preserve the distinction between deterministic invariants and
empirical premises.
