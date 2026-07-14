# Product specification

## Summary

ChemSpec lets a learner compose a reaction visually or describe it in natural
language, then turns that untrusted request into a researched, validated, and
explainable virtual experiment.

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
5. Individually reviewed reactive-metal transformations, beginning with the
   lithium/water educational slice. This does not confer general redox support.

Supported inputs:

- catalogued pure compounds;
- solid, liquid, gas, and aqueous phases where meaningful;
- amounts, concentrations, and volumes;
- water as the solvent;
- explicit or default room-temperature and atmospheric-pressure assumptions.

Initial exclusions:

- arbitrary consumer materials with unknown compositions;
- organic reactions and mechanisms;
- general redox and electrochemistry beyond individually reviewed closed rules;
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

### 1. Build the question

The primary entry point is a structured two-reactant composer above a complete
118-element periodic table. The equation lane keeps Reactant 1, a plus sign,
Reactant 2, and the reaction arrow visible together. One reactant slot is
explicitly active. Clicking a square element tile, or dragging it through the
window-level drag plane into a slot, adds that element to the active draft.
Formula text updates progressively and an adjacent history records concise
selection events.

Element selection is a composition aid, not a chemistry conclusion. It does
not establish a substance, formula, reaction, or supported outcome. Later
builder stages may turn the learner's choices into a request, but that request
must still pass through parsing and chemistry validation before simulation.

Recognised Stage 1 formulae are composition previews only. An incomplete or
unknown collection remains labelled as an unrecognised or intermediate draft;
it is never silently promoted to a substance. The active model reuses the
deterministic shell canvas: outer electrons orbit slowly and curated covalent
previews show shared pairs. This remains illustrative presentation, not a
chemistry conclusion.

Once the two drafts match a supported reaction-request candidate, the primary
action launches the illustrative reaction sequence directly. The previous
intermediate manipulation workspace is not a separate screen in the canonical
journey. Its deterministic atom grouping and candidate logic remain internal
presentation machinery; exact atom identities are copied into that state before
the sequence starts, and no trusted chemistry value is constructed.

Loose atoms use a simplified shell diagram with electrons shown only on the
outermost shell. The orbit is illustrative and stops in reduced-motion mode.
When a recognised grouping forms, its shell diagrams move into one deterministic
grouped-atomic surface; they are not replaced by a ball-and-bond molecular
model. Both representations describe the untrusted composition preview;
neither is a reaction simulation or a validation result. Electron revolution
is deliberately slow and illustrative. Covalent groupings show shared electron
pairs between the relevant shell models; ionic groupings do not claim shared
pairs.

The Stage 1 trigger appears once the two drafts match a small structured
reaction-request catalogue. Unsupported combinations remain editable but cannot
launch the sequence. Selecting `Start Reaction` copies the drafts into the
internal preview state and starts the explicitly illustrative storyboard. It
does not create trusted products, confer validation, or emit a simulation frame.

Stage 5 is unavailable until the downstream language and chemistry pipeline
produces a current validated structural artifact and renderer-independent
frames. A recognised Stage 4 candidate opens an explicit validation gate; it
does not launch the former formula-driven storyboard. Once implemented, Stage
5 is two sequential views: a 2D educational structural explanation followed by
a dedicated genuine-3D spatial view. Both use the same stable atom identities,
immutable graph states, active structural operations, observations, and model
disclosures. The sequence is labelled representative and explanatory; it is not
presented as a proven elementary mechanism or molecular-dynamics trajectory.

Natural-language entry remains available for learners who already know what
they want to ask.

### 2. Choose a provider

At startup, choose **Use Codex subscription** or **Use OpenAI API key**. The
previous selection may be focused, but the choice remains visible.

### 3. Ask

The learner enters the request in ordinary language. Example prompts help
learners discover the supported domain without requiring `.chems` knowledge.

### 4. Watch the workflow

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

### 5. Validate

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

### 6. Animate and simulate

The learner first watches a complete generated 2D educational explanation. A
deterministic planner groups the trusted structural states, operations, and
observations into introduction, reactant, equation, structural-change,
observation, and summary scenes. The learner can pause, restart, and step
without changing chemical meaning.

The separate **View in Real Life** page is a macroscopic low-poly diorama, not a
3D molecular viewer. It composes reviewed reusable environments, vessels,
material forms, effects, and camera behaviours. For silver chloride:

- aqueous ions begin dispersed;
- `Ag+` and `Cl-` form representative clusters in a 1:1 ratio;
- the clusters settle as a white precipitate;
- `Na+` and `NO3-` remain dispersed as spectator ions;
- quantity indicators reflect limiting-reagent consumption.

For lithium and water, the 2D explanation uses metallic electron domains,
explicit reviewed electron/bond operations, stable atoms, and product
membership. The real-world page uses a generic beaker, liquid volume, metal
chunk, gas-evolution effects, and near-isometric camera profiles selected by
reviewed presentation metadata. It never turns atomic models into giant
laboratory objects or invents unsupported flames, explosions, or observations.

### 7. Explain

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
