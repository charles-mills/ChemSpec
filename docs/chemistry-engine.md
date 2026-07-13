# Chemistry engine and validator

## Trust model

The chemistry engine is a small deterministic trusted kernel. The agent may
propose a reaction, but it cannot declare its own output valid.

`Validated` means:

> Given the declared inputs and assumptions, the program is internally
> consistent and its result follows from ChemSpec's explicit, versioned
> chemical rules and accepted catalogue data.

It does not mean that ChemSpec has mathematically proven every real-world
outcome or modelled all competing reactions.

## Validation contract

A passing program guarantees the following within the supported domain.

### Language correctness

- The document parses unambiguously.
- Names resolve to declared or catalogued entities.
- Quantities have valid dimensions and compatible units.
- Required phases and conditions are present or supplied by explicit defaults.

### Chemical identity correctness

- Every substance belongs to the selected catalogue.
- Formula, composition, charge, phase, and dissociation behaviour match accepted
  catalogue facts.
- An unfamiliar substance cannot acquire invented properties during a run.

### Equation correctness

- Every element is conserved.
- Net electric charge is conserved.
- Stoichiometric coefficients are positive and normalized.
- Only appropriate aqueous species dissociate.
- Complete ionic equations preserve all species.
- Net ionic equations remove spectator ions correctly.

### Supported reaction derivation

The outcome must be derivable from an implemented rule family:

- precipitation through versioned solubility rules;
- strong acid/base neutralization;
- a curated gas-forming rule;
- explicit no-net-reaction determination.

A balanced equation alone is insufficient evidence that a reaction occurs.

### Quantitative consistency

When quantities are supplied:

- amount conversions are dimensionally sound;
- the limiting reagent is identified correctly;
- consumption does not exceed supply;
- remaining reactants and theoretical products follow stoichiometry.

### Evidence traceability

Every proof-relevant empirical premise resolves to a versioned catalogue fact
with provenance. Live web research may explain the agent's proposal, but it
cannot become validator authority during that run.

## Result states

- **Validated** — all required claims are derived in the supported domain.
- **Validated with assumptions** — the derivation depends on displayed
  assumptions such as temperature or idealized dissociation.
- **Unsupported** — the input may represent legitimate chemistry, but required
  data or reaction rules are unavailable.
- **Invalid** — syntax, identity, invariants, or declared claims are wrong.

Unsupported chemistry is never presented as chemically false.

## Derivation artifact

The validator produces a structured derivation, not a Boolean:

```text
AgNO3(aq) + NaCl(aq)
  ├─ substances recognized
  ├─ quantities normalized
  ├─ aqueous strong electrolytes dissociated
  ├─ candidate products generated
  ├─ AgCl classified insoluble by catalogue rule
  ├─ atoms conserved
  ├─ charge conserved
  ├─ stoichiometry solved
  └─ net ionic result: Ag+ + Cl- -> AgCl(s)
```

The same artifact supports:

- the educational **Why?** view;
- precise diagnostics;
- agent repairs;
- provenance export;
- regression tests.

## Trusted catalogue

The catalogue is part of ChemSpec's trusted computing base. Runtime agents may
read it but cannot modify it.

Conceptual substance entry:

```text
Substance
  stable identifier
  preferred name and aliases
  formula and elemental composition
  net charge
  supported phases
  aqueous dissociation
  condition-qualified solubility
  appearance
  concise hazard classification
  evidence references
```

Reaction rules are stored separately from substance facts. A substance entry
must not become a hard-coded experiment.

Catalogue requirements:

- stable unique identifiers;
- explicit schema and semantic versions;
- deterministic content digest;
- provenance for each proof-relevant empirical fact;
- conditions attached to facts that are not universal;
- no flattened resolution of genuine source conflicts;
- reproducible review and release process.

Each `.chems` file names its catalogue version. Saved experiment provenance
also records the exact catalogue digest.

## Catalogue updates

Live research may propose a future catalogue addition. Promotion into the
trusted catalogue requires:

1. human authorship or review;
2. supporting sources;
3. schema validation;
4. chemical invariant tests;
5. regression tests against known experiments;
6. a new catalogue version or digest.

The active catalogue is immutable during an experiment.

## Initial chemistry universe

The initial catalogue should provide enough breadth for all three supported
reaction families and meaningful no-reaction examples. It should favour a
coherent, well-evidenced set of acids, bases, carbonates, soluble ionic
compounds, and insoluble products over a long unreviewed list.

Canonical fixtures:

| Inputs | Expected class | Key observation |
| --- | --- | --- |
| `AgNO3(aq) + NaCl(aq)` | Precipitation | White `AgCl(s)` |
| `HCl(aq) + NaOH(aq)` | Neutralization | Water formation |
| `HCl(aq) + NaHCO3(aq)` | Gas formation | `CO2(g)` bubbles |
| `KNO3(aq) + NaCl(aq)` | No net reaction | No supported visible change |

## Explicit non-guarantees

The validator does not initially prove:

- exhaustive coverage of competing real-world reactions;
- exact experimental yield;
- reaction rate;
- detailed equilibrium behaviour;
- impurity effects;
- universal safety;
- literal microscopic fidelity of the particle animation.

These limits must remain visible in both product copy and developer APIs.
