# ChemSpec

ChemSpec is an AI-assisted virtual chemistry laboratory. A learner proposes a
reaction, and ChemSpec first uses chemistry rules and algorithms to determine,
where supported, whether that reaction is likely to occur. If it is possible,
an AI agent supplies the expected observations. ChemSpec then presents a
macroscopic simulation of those observations beside a molecular simulation of
the bond changes involved, before providing an AI-generated overview of the
reaction and the conditions it requires.

The project is being built for the Education category of
[OpenAI Build Week](https://openai.devpost.com/).

## Product contract

ChemSpec separates feasibility, observed behaviour, molecular meaning, and
presentation:

```text
User request
    -> rule-based reaction feasibility check
    -> AI request for expected observations
    -> validated experiment data
    -> side-by-side observation and molecular-change simulations
    -> AI overview, notable points, and required conditions
```

- Chemistry rules and algorithms make the first determination of whether a
  supported reaction is likely to occur.
- The AI agent is asked for observations only after the viability check finds
  a possible reaction.
- If chemistry rules cannot confidently determine reaction viability, 
  the AI agent estimates the likelihood of the reaction before proceeding.
- The `.chems` file is visible and editable by humans.
- The validator is the only component that can promote source into a validated
  experiment.
- The observation simulation shows the visible, macroscopic changes in the
  reaction.
- Alongside it, the molecular simulation follows one representative molecule
  or reaction event and shows bonds being formed, broken, or rearranged. For
  example, alcohol dehydration shows removal of an `OH` group and a hydrogen
  from a neighbouring carbon, while covalent bond formation shows the reacting
  species joining through the new bond.
- After the visual simulation, the AI overview explains any notable features or applications of this reaction and
  the conditions required for the reaction to take place.
- Unsupported chemistry is reported as unsupported and returns the reactants as products, not treated as false.

## Initial chemistry domain

The viability engine is intended to cover predominantly inorganic chemistry
using explicit reaction rules and algorithms. It also includes a corresponding
reaction map for organic compounds and transformations up to A-Level standard.
The initial implemented domain remains closed-world aqueous ionic chemistry
under ordinary classroom-laboratory conditions:

- precipitation reactions;
- strong acid/strong base neutralization;
- a small, curated set of gas-forming reactions;
- explicit no-net-reaction outcomes.

Chemistry outside the available inorganic rules or A-Level organic reaction map
is reported as unsupported rather than being guessed by the AI. The simulations
are explanatory representations of validated reactions, not quantitative
kinetics or general molecular dynamics.

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
- [Interface design system](docs/ui-design-system.md)
- [Agent workflow and providers](docs/agent-workflow.md)
- [Safety policy](docs/safety.md)
- [Verification strategy](docs/verification.md)
- [Build Week delivery plan](docs/delivery-plan.md)
- [Build Week implementation plan](docs/implementation-plan.md)

## Releases

Pushing a tag in the exact form `vMAJOR.MINOR.PATCH` builds and publishes a
GitHub Release containing a Windows MSI, a Linux AppImage and standalone
x86_64 binary, and a universal macOS DMG. The tag version must match
`[workspace.package].version` in `Cargo.toml`.

The packages are currently unsigned. Windows SmartScreen and macOS Gatekeeper
may therefore warn when they are downloaded and opened.

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
```

The normative grammar is [`grammar/chems.ebnf`](grammar/chems.ebnf). There is no
legacy grammar or compatibility path. Formatting refuses incomplete source;
plain `chems format <path>` writes canonical source to standard output.

## Application shell

The Iced application opens in a structured Stage 1 reaction composer above a
complete square-tile periodic table that fits without horizontal scrolling.
Learners progressively build two independent reactant drafts, switch the active
slot, inspect atomic previews and input history, then launch a supported
illustrative reaction sequence directly. The active model uses the same slowly
orbiting electron-shell diagrams and curated shared-electron pairs as the
internal grouping engine; no intermediate manipulation screen interrupts the
flow. Every preview remains clearly labelled as untrusted pending future
chemistry validation. Periodic tiles retain each element's name and atomic mass,
stay close within the s, d, and p blocks, and use larger gaps between those
blocks. The fixed builder composition fits the controls and complete periodic
table on one page without builder scrolling. The existing
validated-record screen remains available for the canonical silver-chloride
fixture.

```sh
cargo run -p chemspec-app
```

## Current status

ChemSpec is in active implementation. The language design and Slices 0–2 are
complete, and the consolidated reaction-builder flow (`U-106`–`U-113`) is available for
review. Stage 5 provides a deterministic 2D reaction storyboard with balanced
representative counts, pause/restart/skip/return controls, and complete
multi-product presentation. It is explicitly an illustrative preview;
chemistry validation, 3D presentation, agent integration, and live simulation
remain later stages.

## License

MIT. See `LICENSE`.
