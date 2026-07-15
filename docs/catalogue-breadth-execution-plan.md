# Catalogue breadth execution plan

Status: implemented, externally attested, promoted, and pinned as part of the
`core-chemistry` trusted aggregate.

This plan implements the four generalized reaction families fixed by
`docs/implementation-plan.md`. Every finite binding is exercised through the
ordinary `.chems` parser, generalized elaborator, structural kernel, and frame
generator in `crates/chems-cli/tests/authoring.rs`.

## Shared boundaries

- Element identities come from the existing 118-element registry.
- `Categories.AlkaliMetal` is `{Li, Na, K}`.
- `Categories.Halide` is `{F, Cl, Br, I}`.
- Candidate premises remain `provisional`; candidate inspection never promotes
  its own output.
- HF reaches an explicit unsupported weak-acid case in the neutralization and
  gas-evolution families.
- Fluorine reaches an explicit unsupported case in halogen displacement.
- No laboratory quantities, procedures, or hazard-bypass instructions are
  encoded.

Primary chemistry sources:

- [OpenStax Chemistry 2e, section 4.2](https://openstax.org/books/chemistry-2e/pages/4-2-classifying-chemical-reactions): precipitation, strong-acid/strong-base neutralization, carbonate and bicarbonate gas evolution.
- [OpenStax Chemistry 2e, section 18.11](https://openstax.org/books/chemistry-2e/pages/18-11-occurrence-preparation-and-properties-of-halogens): halogen reactivity and chloride/bromide/iodide displacement.
- [RSC: The chemistry of silver](https://edu.rsc.org/experiments/the-chemistry-of-silver/516.article): AgCl white, AgBr cream/pale yellow, AgI yellow.
- [RSC: Halogens in aqueous solution and their displacement reactions](https://edu.rsc.org/experiments/halogens-in-aqueous-solution-and-their-displacement-reactions/733.article): secondary-school aqueous halogen displacement observations.

The external sources support element identities, terminology, qualitative
outcomes, explicit chemistry boundaries, and observations. Exact graph
representations, closed valence tuples, and transient execution states cite
`docs/generalized-chemistry-rules.md` as an internal design authority and are
explicitly labeled explanatory modeling assumptions, not empirical evidence or
claims of physical intermediates or mechanism.

## Family 1: silver-halide precipitation

Package: `catalogue/candidates/precipitation-silver-halide`

Rule: `Rules.SilverHalidePrecipitation`

- Supported: `AgNO3 + NaX -> AgX + NaNO3`, where `X` is Cl, Br, or I.
- Member-specific observations:
  - AgCl: white;
  - AgBr: cream;
  - AgI: yellow.
- Explicit unsupported: F, because AgF is soluble rather than a precipitate.
- Rewrite: two ionic dissociations, two ionic associations, then exact product
  assignment. No electron or covalent change is inferred.
- Execution coverage: all four category members, including the F boundary.

## Family 2: strong-acid/strong-base neutralization

Package: `catalogue/candidates/acid-base-neutralization`

Rule: `Rules.MonoproticAcidHydroxideNeutralization`

- Supported: `HX + MOH -> MX + H2O`, where `M` is Li, Na, or K and `X` is Cl,
  Br, or I: nine supported bindings.
- Explicit unsupported: `X = F`, because the current rule does not model the
  partial-dissociation equilibrium of HF.
- Execution coverage: the complete 3 x 4 finite domain.

## Family 3: acid-carbonate gas evolution

Package: `catalogue/candidates/acid-carbonate-gas-evolution`

Rules:

- `Rules.MonoproticAcidBicarbonateGasEvolution`:
  `HX + MHCO3 -> MX + H2O + CO2`.
- `Rules.DiproticAcidCarbonateGasEvolution`:
  `2 HX + M2CO3 -> 2 MX + H2O + CO2`.

Both rules support `M` in `{Li, Na, K}` and `X` in `{Cl, Br, I}`. Both reach
the explicit HF boundary at `X = F`. The carbonate rule uses two exact proton
transfers before the same checked carbonic-acid bond reorganization used by
the bicarbonate family. Carbonate is first-class candidate content, not a
deferred extension.

Execution coverage: both rules across the complete 3 x 4 domain, for 18
supported and six explicit unsupported invocations.

## Family 4: aqueous halogen displacement

Package: `catalogue/candidates/single-displacement-halogen`

Rule: `Rules.HalogenDisplacement`

Representative equation:

```text
X2 + 2 NaY -> 2 NaX + Y2
```

The bounded aqueous reactivity order is `Cl > Br > I`.

- Supported: `(Cl, Br)`, `(Cl, I)`, and `(Br, I)`.
- Explicit unsupported: equal and reversed pairs, which have no expected
  displacement outcome in this model.
- Explicit unsupported: any pair involving F, which requires a distinct,
  safety-reviewed fluorine model.
- Rewrite: homolytic cleavage of `X2`, dissociation of two sodium-halide
  instances, two independently checked one-electron transfers, formation of
  `Y2`, two ionic associations, and exact product assignments.
- Execution coverage: all 16 bindings over `{F, Cl, Br, I}²`.

This replaces the rejected alkali-metal-on-aqueous-alkali-salt model. It is a
conventional secondary-school single-displacement family and requires no
change to the metallic-operation contract.

## Completion checklist

- [x] Member-specific precipitation observations are source-backed and kernel-executed.
- [x] Every acid-base binding executes or reaches its explicit boundary.
- [x] Carbonate and bicarbonate are both first-class executable rules.
- [x] Single displacement models a conventional aqueous halogen activity series.
- [x] Every finite supported and unsupported binding is exercised end to end.
- [x] Evidence uses direct section-level sources.
- [x] Package-order independence is checked.
- [x] Exact digest receives a separate host-selected review attestation.
- [x] The attested digest is promoted into `catalogue/trusted/core-chemistry/`.

## Reproduction

The output path must not already exist.

```sh
cargo run -p chems-cli -- catalogue check --out /tmp/chems-breadth-review-<unique> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-carbonate-gas-evolution \
  catalogue/candidates/single-displacement-halogen

cargo test -p chems-cli --test authoring
```
