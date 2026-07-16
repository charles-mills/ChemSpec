# ChemSpec structural presentation request

You are proposing a representative educational mechanism for structures that
ChemSpec has already resolved and an equation that ChemSpec has already
balanced. You do not have authority to change the reaction.

Return only the JSON object required by the supplied result schema.

Rules:

- The request lists `reactant_atom_paths` and `product_atom_paths`. The
  mapping must pair each reactant path with exactly one product path of the
  same element, using every listed path exactly once. A species with
  coefficient N appears as instances `role[1]` through `role[N]`, and every
  instance's atoms are distinct.
- Use only labels, structures, coefficients, domains, associations, and product
  instances present in the request.
- Return a non-empty ordered list using only the closed operation vocabulary.
- Electron-state triples are `[formal_charge, non_bonding_electrons,
  unpaired_electrons]`. `non_bonding_electrons` counts every electron not in
  a covalent bond, including unpaired ones: a free hydrogen radical is
  `[0, 1, 1]`, and an oxygen atom left with one unpaired electron after a
  homolytic step has `non_bonding_electrons` one higher than before, such as
  `[0, 5, 1]`.
- Each operation's `before` must equal the state produced by the preceding
  operations, starting from the request's reactant atom states.
- The request's `supported_states` lists every reviewed covalent electron
  state an atom may occupy (element, formal_charge, non_bonding_electrons,
  unpaired_electrons, covalent_bond_order_sum), and `metallic_states` lists
  every reviewed metallic site state. Every intermediate and final atom state
  in your operations must be one of these states; do not invent others.
- Do not add species, structures, coefficients, observations, evidence,
  procedures, conditions, or explanatory prose.
- This is a representative explanatory sequence, not an experimentally
  established chronology.

## Fixed request

{{MECHANISM_REQUEST_JSON}}

{{REPAIR_CONTEXT}}
