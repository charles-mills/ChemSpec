# Oxygen reaction screening

ChemSpec has a closed-world, data-driven first pass for reactions with
elemental oxygen. The builder accepts `O2` with every element in the
118-element registry and returns one of four typed outcomes:

- a representative product and balanced equation;
- no standard direct reaction;
- an ambiguous outcome where one product cannot be selected honestly; or
- unsupported where a unique reviewed case is not present.

The records live in `catalogue/oxygen-screening/oxygen.json` and are validated
by `chem-catalogue` against the trusted element registry. Product oxygen count
is stored per case, so normal oxides, peroxides, superoxides and higher oxides
are not derived from one unreliable group-valence formula. The default case is
unsupported and cannot assert a product.

Compound screening is intentionally narrower. It accepts only compounds that
already have structural identities in the catalogue: `H2O`, `LiOH`, `NaOH`
and `KOH` (with `H2` also present as a molecular structure). An arbitrary
formula assembled in the UI cannot enter this path.

Sixty-eight representative outcomes now have authored structural models in
`catalogue/candidates/oxygen-reactions`. Forty reusable oxygen rules cover
normal ionic oxides, peroxides, superoxides, covalent dioxides, water, boron
oxide, phosphorus(V) oxide, periodic-group transition-metal oxides,
mixed-valence M3O4, covalent MO3, bridged M2O7 and molecular MO4. Their
ordinary invocations live under `conformance/end-to-end/oxygen-*-001.chems`.

The transition-metal slice contains 51 selectable outcomes for 27 elements.
Metal sources are grouped by periodic group and delocalised-electron count;
product rules are grouped by stoichiometry and oxidation-state vector. An
element-specific structure application supplies only the family parameter and
formula. It does not contain a copied reaction procedure or stored compound
name. Product names are derived from the validated final structural frame.

The operations explicitly cleave or reduce O=O, localise metallic valence
electrons, transfer electrons, form covalent bonds, associate ionic products,
apply superoxide O-O delocalisation and assign product atoms. Multi-electron
metallic release and compact symmetry canonicalisation are shared domain and
catalogue mechanisms rather than reaction-specific application code.
Independent structure symmetries are canonicalised in linear family passes
rather than by building their Cartesian product, so equivalent M2O5 ions do
not exhaust a certificate limit.

The merged catalogue is promoted into the application trust root through an
explicit AI review attestation; this is not represented as human chemist
certification. All 68 oxygen registry entries are executable structural
simulations. The promoted aggregate digest is
`877f0dfe4f1140c89d315c3e11fb6e257ea417a19614259910bbfd346d09aeeb`.

The present molecular process vocabulary is sufficient for the authored
stoichiometric formula-unit and finite-molecule models: metallic release,
electron transfer, electron reconfiguration, ionic association, O=O cleavage,
single/double covalent formation and bridged oxygen are available. It does not
yet portray infinite crystal-lattice propagation, non-stoichiometric oxides,
phase defects or metal-cluster/dimer cations. Those require a periodic-solid
or variable-occupancy model and are not approximated as extra picker options.

This separation is the extension point for future reaction families: add
reviewed applicability/outcome cases first, then add structural rules and
fixtures only for cases the simulator can portray accurately.
