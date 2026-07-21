# Chemistry coverage

## What the 212 count means

The promoted reference catalogue contains **212 reviewed, finite
experiences**. This is the size of the current host-pinned fast path, not the
number of reactions ChemSpec can answer and not a global allow-list of valid
reactant pairs.

| Promoted catalogue surface | Experiences | Reviewed boundary |
| --- | ---: | --- |
| Established generalized families | 43 | Alkali-metal/water (including bounded heavy-alkali water-contact variants), Ca/Sr/Ba with liquid water, Mg with steam, silver-halide precipitation, strong-acid neutralization, carbonate and bicarbonate gas evolution, and aqueous halogen displacement |
| Elemental oxygen | 68 | Explicit representative products; missing or ambiguous cases do not inherit a group-valence guess |
| Fixed-charge main-group ion pairs | 81 | Group 1, Group 2, and aluminium cations with the reviewed monatomic anion families |
| Finite covalent combinations | 20 | Explicit hydrogen-compound and interhalogen outcomes; multiple reviewed products require learner selection |

An experience is one reviewed binding of reactants, context, products,
structures, and presentation data. Some experiences are individual pairs and
some are concrete members of parameterized families. Catalogue membership
provides reviewed provenance and a fast route to a complete animation; it is
not the authority that permits simulation. Kernel validation is that authority.

The machine-readable catalogue and its attestation remain the source of truth
for the reviewed count:

- [`../catalogue/reference/core-chemistry/catalogue.json`](../catalogue/reference/core-chemistry/catalogue.json)
  is the promoted catalogue;
- [`../catalogue/reviews/core-chemistry.review.json`](../catalogue/reviews/core-chemistry.review.json)
  binds its semantic digest, review scope, premises, and reviewer disclosure;
  and
- the application pins both accepted digests before loading reviewed chemistry.

## Coverage beyond the catalogue

On a catalogue miss, ChemSpec does not immediately declare the reaction
unsupported. It resolves stable species identities and runs the deterministic
solver first. The solver recognizes reaction classes from validated structures
and derives products programmatically, so one algorithm can cover many
reactant combinations without enumerating every pair in the catalogue.

The current deterministic solver includes:

- acid-base reactions involving oxides, hydroxides, carbonates, and
  bicarbonates;
- acids with metals and insoluble metal sulfides;
- complete combustion and context-selected incomplete combustion;
- metal-water and oxide-water reactions;
- single metal displacement, aqueous halogen displacement, and precipitation
  or no-reaction decisions from activity and solubility rules;
- element-element synthesis where charge balance or unique structural
  generation gives one unambiguous product;
- alkene addition and hydrohalogenation, unambiguous light-driven alkane
  substitution, alcohol oxidation, esterification, ester hydrolysis, and
  alcohol dehydration within the supported organic graph subset;
- heat-driven carbonate, bicarbonate, hydroxide, and nitrate decomposition,
  plus the supported ammonium-cyanate rearrangement;
- silver-halide photolysis and water or aqueous-electrolyte electrolysis; and
- conservative no-reaction conclusions for cases the engine can establish,
  such as light noble gases, two elemental metals, identical closed-shell
  substances, less-active displacement attempts, and exchanges whose products
  all remain soluble.

These are structural and rule-based families, not a fixed table of pairs.
Their practical reach depends on which reactants can be resolved to exact
structures, which contexts the learner supplies, and whether the derived
products can be balanced and represented. Adding a validated identity can
therefore make existing algorithms applicable to further reactions without
adding another catalogue experience.

If no deterministic family applies, the runtime may ask the configured model
for a closed factual claim. The model supplies text and provenance only. The
application then resolves identities, balances the declaration exactly, and
attempts deterministic graph-diff derivation, reviewed-family matching, or a
bounded model-proposed mechanism. Raw provider output never reaches playback.

## Why there is no single total reaction count

The complete program surface does not have a useful fixed cardinality like
212. It is the union of:

1. the 212 currently promoted catalogue experiences;
2. reactions derived by structural algorithms over the current reviewed and
   cached identity registry; and
3. previously uncatalogued claims and mechanisms that can be constructed at
   runtime and pass the same exact validation gates.

Counting all names or ordered input strings would greatly overstate coverage
because aliases can denote the same species. Counting only catalogue pairs
would understate it because the algorithms are parameterized. Model-assisted
coverage also changes with resolvable identities and is intentionally not
pre-enumerated.

Coverage is still bounded in the engineering sense: requests contain one or
two reactants; deterministic families have explicit structural and contextual
guards; the chemistry language supports a closed set of representations and
operations; construction and repair are bounded; and ambiguous, unsafe,
unrepresentable, or invalid outcomes stop before simulation. The correct claim
is therefore that **catalogue coverage is finite, while program coverage is
extensible but validation-bounded**.

The 118-element registry supplies element identity metadata. It does not imply
that every element pair reacts or that every possible compound has a resolved
structure.

## Source and regeneration

Untrusted authoring shards live under
[`../catalogue/candidates/`](../catalogue/candidates/). The authoring compiler
rejects unknown fields, self-asserted review state, and candidate mutation of
the host trust root. Generated inspection artefacts are not independent
conformance oracles.

The larger finite catalogue surfaces are generated reproducibly:

- `tools/generate-oxygen-catalogue.py` owns the oxygen and fixed-charge
  main-group expansion, including reviewed macroscopic standard-phase records;
- `tools/generate-covalent-catalogue.py` owns the finite covalent package and
  fixtures; and
- `tools/author_group2_water.py` owns the reviewed alkaline-earth/water
  package. Ca, Sr, and Ba use `M + 2 H2O(l) -> M(OH)2 + H2`; magnesium is a
  condition-distinct steam rule, `Mg + H2O(g) -> MgO + H2`. Beryllium is not
  represented as reacting with ordinary water.

The Group 2 package was promoted on the user's attestation that a chemist had
reviewed and validated these additions. The attestation deliberately does not
invent an identity or credential for that reviewer.

A separate host-selected review must bind generated content before deliberate
promotion and pinning. Runtime-derived or model-proposed chemistry never
promotes itself into the reference catalogue.

## Verification

The shared offline gates are documented in
[`verification.md`](verification.md). To inspect an application-visible
outcome without starting the GUI, use the headless reaction path:

```sh
cargo run -p chemspec-app -- react sodium water
cargo run -p chemspec-app -- react --verbose HCl NaOH
```

A successful headless run demonstrates that the requested chemistry resolved,
balanced, validated, and produced the same renderer-independent frame artefact
the GUI would consume. It does not exercise the GPU renderer, live provider,
credentials, or platform packaging.
