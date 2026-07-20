# ChemSpec

ChemSpec is a chemistry exploration app built for the Education category of [OpenAI Build Week 2026](https://openai.devpost.com/). Learners construct a reaction question, follow its structural changes atom by atom, and then see a human-level 3D interpretation of the same outcome.

**[Try the web demo](https://charles-mills.github.io/ChemSpec/) · [Project documentation](docs/)**

It uses three paths for resolving chemistry:

- **Tier A:** Catalogued reactions, available ~instantly.
- **Tier B:** Families of reactions that are aglorithmically solved, available in 0-3 seconds.
- **Tier C:** Reactions not supported by Tiers A or B, which are deferred to Codex; if valid, the reaction is available within 10-60 seconds.

## What ChemSpec Does

Chemistry equations describe inputs and outputs, but they do not make it easy to see which atoms persist, which relationships change, or how those changes relate to visible observations. ChemSpec connects those views in one guided experience for learners and educators.

1. **Ask:** construct reactants from the periodic table or enter a recognised
   name or formula.
2. **Resolve:** determine the outcome through one of the 3 tiers.
3. **Inspect:** follow stable atoms, bonds, ionic associations, electron transfers, products, and observations through the 2D sequence.
4. **Observe:** continue into an illustrative macroscopic 3D view compiled from the same reaction.
5. **Explain:** inspect the equation, products, and structural derivation.

## How ChemSpec Works

```text
reaction request
  -> reviewed catalogue fast path
  -> algorithmic reaction solver
     -> miss: revalidated cache, then Codex claim
  -> exact balancing and checked declaration
  -> local graph-difference or reviewed-family mechanism
     -> miss: bounded Codex mapping and operation proposal
  -> deterministic chemistry kernel
  -> validated renderer-independent frames
  -> structural 2D and macroscopic 3D presentation
```

Whenever Codex is consulted, its response must be validated and approved by the ChemSpec kernel, or its output is refused. The full crate boundaries
and contracts are documented in the [system architecture](docs/system-architecture.md).

## Reactions to Try

ChemSpec supports 1000s of reactions without deferring to Codex. To try one, use the **Dice Roll** button in the app, choose your own, or enter any of the following examples:

| Reactants | Outcome | Preview |
| --- | --- | :---: |
| `sodium` + `water` | `2Na + 2H2O -> 2NaOH + H2` | <img src="https://github.com/user-attachments/assets/df046f43-08e8-4cba-ad26-647444534ba2" width="120" /> |
| `HCl` + `NaOH` | `HCl + NaOH -> NaCl + H2O` | <img src="https://github.com/user-attachments/assets/73ccbd19-843b-4180-be22-e76c80c1aa83" width="120" /> |
| `AgNO3` + `NaCl` | `AgNO3 + NaCl -> AgCl + NaNO3` | <img src="https://github.com/user-attachments/assets/63ed794b-8747-4a38-809e-7110eb170328" width="120" /> |
| `HCl` + `NaHCO3` | `HCl + NaHCO3 -> NaCl + H2O + CO2` | <img src="https://github.com/user-attachments/assets/f8475d42-b3b9-424d-8129-0864cebf8685" width="120" /> |

## Running ChemSpec

### Web demo

Open the [ChemSpec web demo](https://charles-mills.github.io/ChemSpec/) in a browser with WebGPU support. The web build runs in local mode.

### Desktop app

With [rustup](https://rustup.rs/) available, run the following: 

```sh
git clone https://github.com/charles-mills/ChemSpec.git
cd ChemSpec
cargo run -p chemspec-app
```

Local chemistry does not require an account or network connection. To use the LLM path, install the [Codex CLI](https://github.com/openai/codex), sign in with a ChatGPT account and relaunch ChemSpec.

## How We Used Codex and GPT-5.6

We built ChemSpec from the ground up using GPT 5.6 Sol ("Sol"). We began the project with a team-meeting discussing our intended outcome; once agreed, we handed off to a fresh thread, describing what we wanted to achieve and the components it would require. We requested Sol first ask us everything required to close out any assumptions, after which it was ready to write the [implementation plan](docs/plans/implementation-plan.md) and specifications. Sol then worked through the stages it had specified, end-to-end.

### Designing with Codex

When designing or redesigning the UI, we relied on the Product Design product within Codex. A typical redesign consisted of handing Sol screenshots of the existing pages, providing our critiques, and describing the ideal outcome. Codex then used that to produce rapid visual mockups, considerably faster than using Iced's compile-and-run cycle for questions of composition, hierarchy, spacing and visual weight.

Once the mockup was approved, Sol would produce native Iced components within the app. Once done, Codex would autonomously launch the app, verify the outcome against the agreed mockup, and iterate further if needed.

The provider setup screen preserves this process in the repository. Its [visual QA record](docs/archive/qa/provider-setup/README.md) includes the initial reference design, implementation captures, invalid and valid input states, a normalised side-by-side comparison, the changes made after inspection, and the final result. That entire QA process was captured and recorded autonomously by GPT-5.6 Sol.

### Engineering with Codex

We used repository documents as working contracts for Codex. Larger changes
were divided into bounded tasks in the
[implementation plan](docs/plans/implementation-plan.md), while decisions and
failed assumptions were recorded in the
[rebuild decision log](docs/plans/rebuild-decisions.md). Codex implemented and
reviewed changes across the Rust workspace, ran focused verification while
iterating, and helped us inspect the native application through dedicated
`ChemSpec Agent Smoke` builds.

This workflow is visible in the Git history as well as the documentation:
ChemSpec has Codex-specific branches and commits for UI integration, visual
inspection, sizing fixes, and code review. Packaged smoke checks are recorded
for the reaction builder and representative 3D reactions rather than inferred
from unit tests.

### Codex inside ChemSpec

Codex is also part of the finished application. When the reviewed catalogue and local reaction solver both decline, ChemSpec can ask the user's signed-in Codex installation for a narrow, structured reaction claim. If local graph-difference and reviewed-family mechanisms also decline, Codex may propose an atom mapping and a bounded sequence of operations over structures supplied by ChemSpec.

Those proposals are not validated chemistry. Codex cannot author the structures, coefficients, valence rules, internal identities, or validate
d simulation frames. ChemSpec resolves and balances the reaction locally, then runs any proposed mechanism through the same deterministic chemistry kernel used for offline reactions. If it cannot validate the result, it declines to animate it. The complete boundary is documented in the [agent workflow](docs/agent-workflow.md).
## Running ChemSpec

## Trust, Scope, and Safety

- Exact chemistry quantities use rational and decimal representations rather than binary floating point.
- Raw or stale model output cannot enter the simulation.
- A model-proposed mechanism must pass the same conservation and structural validation as a locally derived one.
- Unsupported, invalid, and provider-failure states remain distinct.
- ChemSpec is an educational explanatory model, not a laboratory procedure, kinetics engine, or molecular-dynamics system.

The detailed boundaries are recorded in the [product specification](docs/product-spec.md), [chemistry engine](docs/chemistry-engine.md), and [safety policy](docs/safety.md).

## Technology

- Rust [(link)](https://rust-lang.org/)
- Iced [(link)](https://iced.rs/)
- A custom `.chems` language and a deterministic kernel
- The Codex CLI

## Team and License

ChemSpec was built by Aryan Saini, Charles Mills, Oliver Robbins, and Patryk Gutowski for
OpenAI Build Week 2026. See [CONTRIBUTORS.md](CONTRIBUTORS.md) for contributor
details.

ChemSpec is released under the [MIT License](LICENSE).
