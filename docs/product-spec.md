# Product specification

## Summary

ChemSpec turns a natural-language chemistry question into a researched,
validated, and explainable virtual experiment.

The product exists to make chemical exploration cheaper, safer, and more
transparent. It is not a chatbot wrapped around an animation: the model's
proposal must pass a deterministic chemistry-aware validation layer before the
application will simulate it.

## Audience

ChemSpec is built primarily for secondary-school chemistry students aged
roughly 14–18. Educators and introductory undergraduate students are important
secondary audiences.

The interface uses progressive disclosure rather than separate audience modes:

- the default view explains the experiment accessibly;
- **Why did this happen?** exposes the derivation;
- **Inspect `.chems`** exposes the source program;
- **Sources** exposes research provenance;
- **Validation** exposes the formal checks.

## Learning outcome

After running an initial aqueous experiment, a learner should be able to
explain:

- what changed and what remained;
- why the supported reaction occurred or why there was no net reaction;
- how the molecular, complete ionic, and net ionic equations relate;
- which species are participating ions and which are spectators;
- how atoms and charge remain conserved;
- how supplied quantities determine the limiting reagent.

ChemSpec is a learning and pre-lab tool, not a replacement for supervised
practical laboratory work.

## Initial scope

The first chemistry universe is deliberately bounded:

> Known pure substances interacting in water under declared, ordinary
> classroom-laboratory conditions.

Supported reaction families:

1. Precipitation reactions.
2. Strong acid/strong base neutralization.
3. A curated set of gas-forming reactions, beginning with acid/carbonate
   chemistry.
4. No-net-reaction outcomes.

Supported inputs:

- catalogued pure compounds;
- solid, liquid, gas, and aqueous phases where meaningful;
- amounts, concentrations, and volumes;
- water as the solvent;
- explicit or default room-temperature and atmospheric-pressure assumptions.

Initial exclusions:

- arbitrary consumer materials with unknown compositions;
- organic reactions and mechanisms;
- general redox and electrochemistry;
- combustion;
- quantitative kinetics;
- reversible equilibria beyond explicitly supported rules;
- weak-acid and weak-base quantitative modelling;
- thermodynamic prediction from first principles;
- multi-stage real laboratory procedures.

The word *substance* is preferred to *material* in the initial product. A named
commercial product, alloy, biological sample, or unspecified mixture must not
be silently treated as a pure compound.

## Canonical journey

The canonical request is:

> What happens if I mix 50 mL of 0.100 M silver nitrate with 50 mL of
> 0.100 M sodium chloride?

### 1. Choose a provider

At startup, choose **Use Codex subscription** or **Use OpenAI API key**. The
previous selection may be focused, but the choice remains visible.

### 2. Ask

The learner enters the request in ordinary language. Example prompts help
learners discover the supported domain without requiring `.chems` knowledge.

### 3. Watch the workflow

The application exposes action summaries rather than hidden model reasoning:

```text
✓ Identified the requested substances
● Researching aqueous behaviour...
  Found 3 relevant sources
○ Predicting the reaction
○ Writing .chems
○ Validating
```

Source cards appear as evidence is found. The generated source becomes visible
as soon as it is available.

### 4. Validate

A successful result reports the checks and assumptions:

```text
Validated with assumptions

✓ Syntax and types
✓ Known substances
✓ Atoms conserved
✓ Charge conserved
✓ Precipitation rule established
✓ Stoichiometry solved

Assumptions
• Aqueous solutions
• 25 degC
• 1 atm
• Idealized complete dissociation
```

If validation fails, the application highlights the exact source location and
may ask the agent for a bounded repair. Each patch remains visible.

### 5. Simulate

The learner can play, pause, restart, adjust presentation speed, toggle particle
labels, and step through reaction stages. For silver chloride:

- aqueous ions begin dispersed;
- `Ag+` and `Cl-` form representative clusters in a 1:1 ratio;
- the clusters settle as a white precipitate;
- `Na+` and `NO3-` remain dispersed as spectator ions;
- quantity indicators reflect limiting-reagent consumption.

### 6. Explain

The explanation connects the macroscopic observation, particle view, equations,
validation derivation, assumptions, and evidence. Selecting a species or claim
in one representation should highlight its counterparts in the others where
practical.

## Product states

An experiment result is always one of:

- **Validated** — completely derived inside the supported domain.
- **Validated with assumptions** — derived under displayed environmental or
  idealization assumptions.
- **Unsupported** — potentially legitimate chemistry outside the current
  catalogue or reaction rules.
- **Invalid** — internally inconsistent, malformed, unknown, or contradicted by
  the trusted inputs.

`Unsupported` and `Invalid` are intentionally distinct.

## Simulation claim

ChemSpec provides an explanatory, quantitatively constrained particle model.
It preserves the validated stoichiometry, phases, limiting reagent, spectator
ions, and supported observations. Particle scale, motion, spatial density, and
elapsed time are illustrative.

Persistent disclosure:

> Explanatory particle model. Quantities and reaction relationships are
> validated; particle scale, motion, and elapsed time are illustrative.

The product does not initially claim molecular dynamics, real reaction rates,
activation energies, intermolecular forces, quantitative diffusion, solvation
shells, or crystal-structure fidelity.

## Success criteria

The product succeeds when a learner can:

1. ask a supported chemistry question naturally;
2. see what the agent researched and proposed;
3. inspect a readable `.chems` program;
4. distinguish deterministic validation from model confidence;
5. explore the validated result visually;
6. explain why the supported reaction occurred.

No invalid program may reach the simulation.

## References

- [OpenAI Build Week requirements and judging criteria](https://openai.devpost.com/)
- [OpenStax: classifying aqueous chemical reactions](https://openstax.org/books/chemistry-2e/pages/4-2-classifying-chemical-reactions)
- [AQA GCSE chemistry subject content](https://www.aqa.org.uk/subjects/science/gcse-science-8464/specification/chemistry-subject-content)
