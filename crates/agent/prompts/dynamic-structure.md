# ChemSpec structure proposal request

ChemSpec has validated a balanced reaction outcome, but its reviewed structure
library has no structural graph for the reactant or product species listed below. Propose
one representative structural graph for each listed species. ChemSpec will
independently validate graph integrity, exact element inventory, charge, and
reviewed or ChemSpec-derived provisional valence states inside an isolated working catalogue; the proposal is
untrusted until that validation passes.

Return only the JSON object required by the supplied result schema.

Rules:

- Return exactly one structure per requested species. `id` must equal the
  requested `id` and `formula` must equal the requested `formula`.
- Return one graph per species, not one graph per reaction coefficient. Atom
  labels are unique within that species; ChemSpec later expands coefficient N
  into distinct instances `role[1]` through `role[N]`.
- `representation` is `molecular`, `ion`, `ionic`, or `metallic`. Choose the
  ordinary representative form of the species as written in the formula.
- An atom is `{label, element, formal_charge, non_bonding_electrons,
  unpaired_electrons}`. Labels are short unique lowercase identifiers such as
  `o1`. Electron counts must be internally consistent with the bonds you draw.
  `non_bonding_electrons` counts every electron not in a covalent bond,
  including unpaired electrons.
- A bond is `{left, right, order}` with order `single`, `double`, or `triple`.
- A molecular or ion structure has `atoms`, `bonds`, and `groups` (may be
  empty arrays). An ionic structure has `components`
  (`{label, atoms, bonds, groups}`) and `associations`
  (`{label, components}`). A metallic structure has `sites` (atoms) and
  `domains` (`{label, sites, delocalized_electrons}`).
- Represent an elemental metal in its ordinary metallic form with
  `representation: "metallic"` and a domain matching one of the supplied
  `metallic_states`. Every metallic site must have
  `non_bonding_electrons: 0` and `unpaired_electrons: 0`; its valence
  electrons belong only in the domain's `delocalized_electrons`. A metallic
  site's `formal_charge` must equal the number of delocalized electrons
  assigned to that site (the domain's `delocalized_electrons` divided by its
  site count), so the complete metallic structure remains neutral. Do not model
  a neutral elemental metal as a molecular atom with zero non-bonding and zero
  delocalized electrons, and do not count the same electron both locally and
  in the domain.
- The overall structure must be net charge neutral.
- The request includes reviewed `neutral_valence`, `supported_states`, and
  `metallic_states`. Prefer a reviewed state. When an ordinary structure needs
  an unlisted state and `provisional_states_allowed` is true, provide only the
  atom electron counts; ChemSpec derives the provisional valence record and
  accepts it only if the formal-charge identity and site checks pass. Never
  output or invent a valence table.
- For every covalent atom, obey the exact identity
  `formal_charge = neutral_valence - non_bonding_electrons - covalent_bond_order_sum`
  using one of that element's supplied `neutral_valence` values. On a repair,
  correct every atom named by the local diagnostic instead of repeating the
  rejected electron counts.
- Do not add species, coefficients, mechanisms, observations, sources,
  procedures, conditions, or explanatory prose.

## Requested species

{{STRUCTURE_REQUEST_JSON}}

{{REPAIR_CONTEXT}}
