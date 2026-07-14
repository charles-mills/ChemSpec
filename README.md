# ChemSpec

ChemSpec is an AI-assisted theoretical chemistry explorer. A learner proposes a
reaction; reviewed catalogue rules identify a supported outcome; an agent
researches qualitative observations and writes concise `.chems 1`; and a
deterministic structural kernel expands and validates the exact atom-mapped
changes before paired observation and structural simulations can run.

The project is being built for the Education category of
[OpenAI Build Week](https://openai.devpost.com/).

## Product contract

```text
request
  -> reviewed-rule applicability
  -> evidence-backed qualitative observations
  -> concise structural .chems 1
  -> deterministic mapping and operation expansion
  -> graph, charge, electron, and product validation
  -> paired observation and structural-change frames
  -> explanatory overview
```

- Formulae summarize composition; catalogue graphs define structure.
- Shared and dative covalent bonds, ionic associations, and metallic domains
  remain distinct; dative direction records donor-pair origin on a single bond.
- Applicability belongs to reviewed reaction rules, not agent invention.
- The authored source is visible and editable.
- The expanded atom map and structural certificate are visible and derived.
- The validator is the only component that can construct trusted chemistry.
- The renderer visualizes validated states and never discovers outcomes.
- Unsupported chemistry remains Unsupported rather than false or guessed.

ChemSpec shows a representative theoretical outcome. It is not a laboratory
instruction system, molecular-dynamics simulator, bulk solution model, or
automatic mechanism proof.

## Example

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

The reviewed rule supplies applicability, structures, deterministic instance
expansion, complete atom mapping, exact electron allocations, and the ordered
structural-operation template. The kernel validates the expanded result in
full; `by apply` cannot select or omit checks.

## Documentation

- [Product specification](docs/product-spec.md)
- [The `.chems` language](docs/chems-language.md)
- [Normative `.chems` specification](docs/chems-specification.md)
- [Structural architecture](docs/structural-chems-architecture.md)
- [`.chems` implementation plan](docs/chems-implementation-plan.md)
- [Generalized chemistry rules (locked forward design)](docs/generalized-chemistry-rules.md)
- [Generalized rules implementation plan](docs/generalized-rules-implementation-plan.md)
- [Catalogue candidate and Luna handoff](docs/luna-catalogue-handoff.md)
- [Chemistry engine](docs/chemistry-engine.md)
- [System architecture](docs/system-architecture.md)
- [Agent workflow and providers](docs/agent-workflow.md)
- [Verification strategy](docs/verification.md)
- [Conformance contract](conformance/README.md)

## Workspace

The Rust workspace separates pure structural values, language tooling, trusted
catalogue data, and validation:

- `chem-domain` — exact identities and structural domain values;
- `chems-lang` — lossless `.chems 1` frontend and formatter;
- `chem-catalogue` — immutable reviewed structures and rules;
- `chem-kernel` — resolution, expansion, graph validation, and artifacts;
- `chems-cli` — parsing, formatting, source/certificate inspection, and the
  closed candidate-package authoring compiler, whose generated chemistry
  remains explicitly untrusted; and
- `chems-conformance` — specification, grammar, fixture, and coverage gates.
- `chemspec-app` — native Iced composition UI and trusted frame renderer.

The desktop application is native Rust using Iced and `wgpu`. Provider support
offers either a Codex subscription through the local `codex` binary or direct
Responses API access with an API key.

## Language status

The structural `.chems 1` implementation is complete through the fixed seven
slices. The bundled lithium-and-water catalogue is promoted through an exact
host-pinned AI review attestation. The attestation names its AI reviewer and
limitations; it is an explicit product trust decision, not human expert
certification.

Generalized element categories, structure templates, graph patterns, and
reaction families are implemented without changing the authored `.chems 1`
grammar. The candidate-authoring path can merge and exercise catalogue content,
but it cannot promote its own output; chemistry review and host trust-root
updates remain deliberate source-controlled host actions. Runtime agents and
candidate packages still cannot promote themselves.

The Iced application now consumes the canonical `.chems 1` source through the
real catalogue, expansion, kernel-validation, and `SimulationFrames` APIs. Its
local periodic-table and composition models are draft presentation only; they
do not choose products or construct bonds for simulation.

## Development commands

```sh
cargo run -p chems-conformance -- validate
cargo run -p chems-cli -- inspect source conformance/expansion/canonical-expansion-001.chems
cargo run -p chems-cli -- catalogue check --out /tmp/chems-review \
  catalogue/candidates/periodic-table-and-alkali-water
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p chemspec-app
```

## Releases

Pushing a tag in the exact form `vMAJOR.MINOR.PATCH` builds a Windows MSI, a
Linux AppImage and standalone binary, and a universal macOS DMG. The tag must
match `[workspace.package].version`. Packages are currently unsigned.

## License

ChemSpec is licensed under the [MIT License](LICENSE).
