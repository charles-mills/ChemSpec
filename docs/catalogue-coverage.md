# Reference catalogue coverage

## Authority

The current catalogue truth is machine-readable:

- [`../catalogue/reference/core-chemistry/catalogue.json`](../catalogue/reference/core-chemistry/catalogue.json)
  is the promoted catalogue;
- [`../catalogue/reviews/core-chemistry.review.json`](../catalogue/reviews/core-chemistry.review.json)
  binds the exact semantic digest, review scope, premise set, and reviewer
  disclosure; and
- the application host pins the accepted catalogue and review digests before
  it can load validated chemistry through `chem-catalogue`.

This document is a navigation and scope summary. Counts and digests in the
machine-readable trust artefacts are authoritative when they differ from prose.

## Current reviewed surface

The promoted review covers 205 finite experiences:

| Surface | Experiences | Boundary |
| --- | ---: | --- |
| Established generalized families | 36 | Alkali-metal/water, silver-halide precipitation, strong-acid neutralization, carbonate and bicarbonate gas evolution, and aqueous halogen displacement |
| Elemental oxygen | 68 | Explicit representative products only; missing or ambiguous cases do not inherit a group-valence guess |
| Fixed-charge main-group ion pairs | 81 | Group 1, Group 2, and aluminium cations with the reviewed monatomic anion families |
| Finite covalent combinations | 20 | Explicit hydrogen-compound and interhalogen outcomes; multiple reviewed products require learner selection |

The 118-element registry provides identity metadata, not universal reaction
coverage. Runtime algorithms may derive chemistry outside this fast path, but
their results cross the same exact balancing and kernel validation and cannot
promote themselves into the catalogue.

## Source and regeneration

Untrusted authoring shards live under [`../catalogue/candidates/`](../catalogue/candidates/).
The authoring compiler rejects unknown fields, self-asserted review state, and
candidate mutation of the host trust root. Generated inspection artefacts are
not independent conformance oracles.

The larger finite surfaces are generated reproducibly by repository tools:

- `tools/generate-oxygen-catalogue.py` owns the oxygen and fixed-charge
  main-group source expansion, including the reviewed macroscopic
  standard-phase records; and
- `tools/generate-covalent-catalogue.py` owns the finite covalent package and
  fixtures.

Use the ordinary candidate compiler and tests to inspect proposed content. A
separate host-selected review must bind the resulting digest before deliberate
promotion and pinning.

## Verification

The shared offline gates are documented in [`verification.md`](verification.md).
To inspect application-visible outcomes without a GUI, use the headless reaction
path described in the repository README, for example:

```sh
cargo run -p chemspec-app -- react sodium water
cargo run -p chemspec-app -- react --verbose HCl NaOH
```
