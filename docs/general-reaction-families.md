# General reaction-family architecture

ChemSpec screens reactions from reviewed element facts and structural traits;
the desktop must not identify a family by comparing compound names.

## Typed element facts

`ElementRecord.reaction_facts` contains:

- common ionic charges;
- independent `metal_displacement`, `hydrogen_displacement`, and
  `halogen_displacement` ranks; and
- a `cold_water`, `steam_only`, or `no_modelled_reaction` classification.

A rank is ordinal inside one named series only. There is intentionally no
universal reactivity number. Missing facts mean Unsupported, never no reaction.

## Structural reaction traits

Reviewed structures and templates can assert graph-backed traits including
Brønsted proton donor, hydroxide base, carbonate base, soluble ionic reactant,
insoluble ionic product, elemental metal, halogen oxidant, oxygen oxidant,
combustible fuel, and water reactant. `reaction_family_candidates` uses these
traits symmetrically and never switches on structure or compound IDs.

Classification only proposes a family. It cannot authorize products or frames.
An exact generalized case must still elaborate and pass the structural kernel.

The catalogue screening path is implemented. Some established desktop
experiences still enter through the older finite request registry/enums; those
must be migrated to catalogue-authored experiences before the application layer
itself is completely free of compound-specific request matching.

## Executable coverage

- **Metal + halogen:** the existing 22 fixed-charge rules cover 81 main-group
  ion-pair experiences by charge reduction.
- **Group 1 + water:** one generalized rule now covers Li, Na, K, Rb, and Cs.
  Francium remains unsupported rather than inferred from group membership.
- **Metal + acid:** three charge-family rules cover the reviewed +1, +2 and +3
  metals above hydrogen with HCl, HBr, or HI, producing the corresponding
  ionic halide and hydrogen. This adds 30 data-authored experiences.
- **Acid + base:** structures are recognized through proton-donor and
  hydroxide-base traits; the executable closed domain remains HCl/HBr/HI with
  LiOH/NaOH/KOH.
- **Carbonate + acid:** carbonate and bicarbonate structures carry the same
  reusable carbonate-family classification; the executable closed domain
  remains the reviewed alkali salts and HCl/HBr/HI.
- **Displacement:** halogen displacement uses its own activity order. Metal
  displacement now has six executable outcomes generated from the reviewed
  Mg > Zn > Fe > Cu series and explicit divalent chloride graphs. Reduction of
  the displaced ion creates a new metallic domain rather than faking a bond.
- **Precipitation:** soluble-reactant and insoluble-product traits replace
  compound-name detection. In addition to AgCl, AgBr, and AgI, the executable
  table now includes BaSO4, CaCO3, Cu(OH)2, and Fe(OH)3. Polyatomic covalent
  groups remain intact while ionic associations are exchanged.
- **Combustion:** one generator derives formulae, balanced integer
  stoichiometry, total atom correspondence, bond cleavage, and product-bond
  formation for the unbranched C1-C10 alkane catalogue. These ten experiences
  are ordinary trusted `.chems` sources. The generalized-instance limit is 128
  so decane's 75 instances remain bounded, and canonical full-graph matching
  avoids factorial enumeration of equivalent hydrogen atoms.

## Explicit non-universal boundaries

- Oxidising acids and passivation are separate from the non-oxidising acid rule.
- HF needs an equilibrium model.
- Lewis acids and bases are separate from the current Brønsted model.
- Complete and incomplete combustion are different outcome families.
- Solubility requires reviewed ion-specific facts and exceptions; it cannot be
  derived safely from a formula alone.
- Variable-charge metals require an explicit selectable oxidation-state/product
  outcome, as with transition-metal oxides.

These boundaries are represented as Unsupported knowledge. They are not silently
treated as impossible reactions.
