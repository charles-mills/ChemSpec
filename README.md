# ChemSpec

ChemSpec is an AI-assisted virtual chemistry laboratory. A learner asks what
happens when substances are mixed; an agent researches the expected reaction,
writes a chemistry-native `.chems` program, and submits it to a deterministic
validator. Only validated experiments can drive the explanatory particle
simulation.

The project is being built for the Education category of
[OpenAI Build Week](https://openai.devpost.com/).

## Product contract

ChemSpec separates proposal, trust, meaning, and presentation:

```text
User request
    -> agent research and cited evidence
    -> generated .chems source
    -> parser and deterministic chemistry validation
    -> validated experiment
    -> explanatory particle simulation
```

- The agent may research and propose chemistry.
- The `.chems` file is visible and editable by humans.
- The validator is the only component that can promote source into a validated
  experiment.
- The simulation visualizes validated chemistry; it does not discover reaction
  outcomes through particle collisions.
- Unsupported chemistry is reported as unsupported, not treated as false.

## Initial chemistry domain

The first complete domain is closed-world aqueous ionic chemistry under
ordinary classroom-laboratory conditions:

- precipitation reactions;
- strong acid/strong base neutralization;
- a small, curated set of gas-forming reactions;
- explicit no-net-reaction outcomes.

The initial domain does not attempt arbitrary materials, organic mechanisms,
general redox, combustion, quantitative kinetics, or molecular dynamics.

## Example

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

## Documentation

- [Product specification](docs/product-spec.md)
- [The `.chems` language](docs/chems-language.md)
- [`.chems` language specification](docs/chems-specification.md)
- [`.chems` implementation plan](docs/chems-implementation-plan.md)
- [`.chems` conformance contract](conformance/README.md)
- [Chemistry engine and validator](docs/chemistry-engine.md)
- [System architecture](docs/system-architecture.md)
- [Agent workflow and providers](docs/agent-workflow.md)
- [Safety policy](docs/safety.md)
- [Verification strategy](docs/verification.md)
- [Build Week delivery plan](docs/delivery-plan.md)
- [Build Week implementation plan](docs/implementation-plan.md)

## Language toolchain

Slices 0–4 provide the executable specification boundary, exact domain
foundation, lossless source frontend, immutable catalogue trust store, and
catalogue-backed typed elaboration.
`chems-lang` implements `chems 1` dispatch, encoding/layout validation, nested
comments, the complete normative grammar, lossless CST and source AST output,
recovery diagnostics, comment attachment, and canonical formatting.

`chem-catalogue` loads versioned digest-bearing bundles, canonicalizes semantic
record order, validates formula/species/condition/evidence/review consistency,
rejects conflicting or ineligible production facts, and builds deterministic
lookup indexes. The initial reviewed fixture is the exact room-condition
silver-chloride teaching domain in
[`conformance/catalogue`](conformance/catalogue).

`chem-kernel` resolves a complete source AST into typed experiment HIR with
stable experiment/material/vessel/stage/operation IDs, exact conditions and
quantities, catalogue-resolved species and media, dimension-directed initial
materials, explicit premise and assumption dependencies, typed procedure
operands, and source origins. Its procedure engine then constructs immutable
stages, exact vessel and inventory state, append-only movement/split/separation
lineage, deterministic reaction opportunities, and classified transition
diagnostics. Reaction outcomes and claim validation remain deferred.

```sh
cargo run -p chems-conformance -- validate
cargo run -p chems-conformance -- report
cargo run -p chems-lang -- parse experiment.chems
cargo run -p chems-lang -- format --check experiment.chems
cargo run -p chems-lang -- format --write experiment.chems
```

The normative grammar is [`grammar/chems.ebnf`](grammar/chems.ebnf). There is no
legacy grammar or compatibility path. Formatting refuses incomplete source;
plain `chems format <path>` writes canonical source to standard output.

## Current status

ChemSpec is in active implementation. The language design and Slices 0–5 are
complete. Slice 6 will elaborate stage-targeted claims and holes into open proof
goals without yet running chemistry rules or tactics.

## License

MIT. See `LICENSE`.
