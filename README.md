# ChemSpec

ChemSpec is a theoretical chemistry explorer. A learner composes reactants;
an algorithmic solver predicts the outcome for the classroom reaction
families it knows (neutralization, displacement, combustion, decomposition,
precipitation, oxide chemistry, and more), a Lewis-structure generator
derives every species' structure from periodic-table physics, and a
graph-diff deriver computes the mechanism between the two validated
endpoints. A model (Codex) is consulted only when the algorithms decline,
and its claims cross exactly the same balancing, kernel validation, and
animation pipeline — the model is a fallback, never a shortcut.

The project is being built for the Education category of
[OpenAI Build Week](https://openai.devpost.com/).

<img alt="image" src="https://github.com/user-attachments/assets/adbbfc91-d12c-4244-b28d-2f04f31b82e8" />

## Product contract

```text
request
  -> algorithmic solver (reaction families, confident no-reactions)
     -> miss: reviewed catalogue fast path
        -> miss: model returns a closed factual claim
  -> stable species identity + exact balancing (all paths)
  -> structure generation for every species without a reviewed graph
  -> mechanism: graph diff between validated endpoints
     -> miss: model proposal, validated identically
  -> graph, charge, electron, and product validation
  -> paired observation and structural-change frames
```

- Formulae summarize composition; catalogue graphs define structure.
- Shared and dative covalent bonds, ionic associations, and metallic domains
  remain distinct; dative direction records donor-pair origin on a single bond.
- Reviewed-family applicability is selected locally and must pass deterministic
  validation. A model-proposed mechanism is separately disclosed and must pass
  the identical kernel; it never becomes a reviewed catalogue rule.
- The authored source is visible and editable.
- The expanded atom map and structural certificate are visible and derived.
- The validator is the only component that can construct renderer-eligible
  chemistry.
- The renderer visualizes validated states and never discovers outcomes.
- Malformed, unsafe, ambiguous, or unrepresentable chemistry remains blocked
  rather than false or guessed.

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
- [Macroscopic 3D visual system](docs/macroscopic-visual-system.md)
- [Agent workflow and providers](docs/agent-workflow.md)
- [Dynamic reaction outcome rebuild plan](docs/dynamic-reaction-rebuild-plan.md)
- [Verification strategy](docs/verification.md)
- [Conformance fixtures](conformance/README.md)

## Workspace

The Rust workspace separates pure structural values, language tooling,
catalogue data, provider output, and validation:

- `chem-domain` — exact identities and structural domain values;
- `chems-lang` — lossless `.chems 1` frontend and formatter;
- `chem-catalogue` — immutable reviewed structures/rules and strict working
  catalogue validation;
- `chem-kernel` — resolution, expansion, graph validation, and artifacts;
- `chem-presentation` — deterministic guided-scene and macroscopic-scene plans
  compiled only from validated kernel frames;
- `agent` — the algorithmic reaction solver, systematic naming (both
  directions), graph-diff mechanism derivation, compact claim/mechanism
  contracts, Codex invocation, exact outcome compilation, and cache v3;
- `chems-cli` — parsing, formatting, and source/expansion inspection;
- `chemspec-app` — native Iced composition UI plus Canvas 2D and wgpu 3D
  renderers.

The desktop application is native Rust using Iced and `wgpu`. The first live
dynamic provider uses a Codex subscription through the local `codex` binary.
Codex binary remains the default provider. The startup UI reserves BYOK/API as
a possible provider-neutral backup, but it is not connected and neither a
direct API implementation nor a hosted backend is required by the current
[dynamic reaction rebuild](docs/dynamic-reaction-rebuild-plan.md).

## Chemistry status

Chemistry is derived programmatically, with the reviewed catalogue as a
curated fast path rather than a boundary:

- The **structure generator** builds Lewis structures from an element
  multiset alone — octet/duet ledgers, expanded octets toward more
  electronegative partners, formal-charge distributions, symmetric-resonance
  delocalization (nitrate reads 4/3, benzene 3/2) — and declines honestly
  when a formula is genuinely ambiguous.
- The **reaction solver** covers the classroom families: acid-base (oxides,
  hydroxides, carbonates, bicarbonates), acid + metal and the activity
  series, single/double/halogen displacement with solubility rules, C/H/O
  combustion, anhydride hydration and slaking, metal + water, heat and
  electrolysis decomposition, plus confident no-reactions (noble gases,
  metal pairs, insoluble ions). Products carry systematic names.
- The **mechanism deriver** computes operation sequences as a graph diff
  between validated endpoint structures; the kernel validates the result
  identically whether it came from the deriver, a reviewed family, or a
  model proposal.
- The **reactant composer** accepts periodic-table drafts or typed names and
  formulas ("copper(II) sulfate", `Mg(NO3)2`, "zinc + hydrochloric acid"),
  and previews any compound the generator can build.

Runtime model claims cannot promote themselves into the reviewed catalogue,
and cache v3 revalidates cached outcomes for offline replay. Malformed,
ambiguous, or unrepresentable chemistry remains blocked rather than guessed.

## Development commands

```sh
cargo run -p chems-cli -- inspect source conformance/expansion/canonical-expansion-001.chems
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p chemspec-app
```

### macOS visual smoke tests

Computer Use must target a fresh, uniquely identified app bundle instead of a
raw `cargo run` process or the release-named `ChemSpec.app`. Install the same
packager version used by the release workflow, then launch the desired view:

```sh
cargo install cargo-packager --version 0.11.8 --locked
just agent-smoke builder
just agent-smoke 2d
just agent-smoke 3d
just agent-smoke stop
```

The launch command rebuilds the application, recreates
`target/agent-smoke/ChemSpec Agent Smoke.app`, verifies that its executable is
byte-identical to the fresh debug binary, and launches that exact path as a new
instance. Agents must use `ChemSpec Agent Smoke` as the Computer Use app name
and verify the mode-specific window title before judging the rendered UI:

- `ChemSpec Agent Smoke — Structural 2D`
- `ChemSpec Agent Smoke — Structural 3D`
- `ChemSpec Agent Smoke — Builder`

Do not target `ChemSpec` for an automated visual smoke; Computer Use may resolve
that name to an older registered development or release bundle.

## Releases

Pushing a tag in the exact form `vMAJOR.MINOR.PATCH` builds a Windows MSI, a
Linux AppImage and standalone binary, and a universal macOS DMG. The tag must
match `[workspace.package].version`. Packages are currently unsigned.

## License

ChemSpec is licensed under the [MIT License](LICENSE).
