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

Slices 0–2 provide the executable specification boundary, exact domain
foundation, and lossless source frontend. `chems-lang` implements `chems 1`
dispatch, encoding/layout validation, nested comments, the complete normative
grammar, lossless CST and source AST output, recovery diagnostics, comment
attachment, and canonical formatting.

```sh
cargo run -p chems-conformance -- validate
cargo run -p chems-conformance -- report
cargo run -p chems-lang -- parse experiment.chems
cargo run -p chems-lang -- format --check experiment.chems
cargo run -p chems-lang -- format --write experiment.chems
cargo run -p chemspec-app
```

The normative grammar is [`grammar/chems.ebnf`](grammar/chems.ebnf). There is no
legacy grammar or compatibility path. Formatting refuses incomplete source;
plain `chems format <path>` writes canonical source to standard output.

## Current status

ChemSpec is in active implementation. The language design and Slices 0–2 are
complete, and the static Iced application shell is available. Slice 3 is the
catalogue foundation; chemistry validation, agent integration, and live
simulation follow it.

## License

MIT. See `LICENSE`.
