The project was built for the Education category of [OpenAI Build Week 2026](https://openai.devpost.com/).

ChemSpec is a chemistry exploration app, giving educators and learners the ability to select arbitrary reactions and watch them play. It provides both a 2D molecular animation taken step-by-step, or a 3D "human-perspective" view. It includes local, offline support for a massive range of reactions; and where it cannot assess the outcome locally, Codex is consulted to build the reaction live.

## How We Used Codex and GPT-5.6

We built ChemSpec from the ground up using GPT 5.6 Sol ("Sol"). Starting the project, we began with a team-meeting discussing our intended outcome. Once agreed, we handed off to a fresh thread, describing what we wanted to achieve and the components it would require. We requested Sol first ask us everything required to close out any assumptions, after which it was ready to write the [implementation plan](docs/plans/implementation-plan.md) and specifications.

Sol then worked through the stages it had specified, end-to-end.

### Designing with Codex

When designing or redesigning the UI, we relied on the Product Design product within Codex. A typical redesign consisted of handing Sol screenshots of the existing pages, providing our critiques, and describing the ideal outcome. Codex then used that to produce rapid visual mockups, considerably faster than using Iced's compile-and-run cycle for questions of composition, hierarchy, spacing and visual weight.

Once the mockup was approved, Sol would produce native Iced components within the app. Once done, Codex would autonomously launch the app, verify the outcome against the agreed mockup, and iterate further if needed.

The provider setup screen preserves this process in the repository. Its
[visual QA record](docs/archive/qa/provider-setup/README.md) includes the reference design, implementation captures, invalid and valid input states, a
normalized side-by-side comparison, the changes made after inspection, and the
final result. That entire QA process was captured and recorded autonomously by
GPT-5.6 Sol.

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

Codex is also part of the finished application. When the reviewed catalogue
and local reaction solver both decline, ChemSpec can ask the user's signed-in
Codex installation for a narrow, structured reaction claim. If local
graph-difference and reviewed-family mechanisms also decline, Codex may propose
an atom mapping and a bounded sequence of operations over structures supplied
by ChemSpec.

Those proposals are not trusted chemistry. Codex cannot author the structures,
coefficients, valence rules, internal identities, or validated simulation
frames. ChemSpec resolves and balances the reaction locally, then runs any
proposed mechanism through the same deterministic chemistry kernel used for
offline reactions. If it cannot validate the result, it declines to animate
it. The complete boundary is documented in the
[agent workflow](docs/agent-workflow.md).
