# ChemSpec structure proposal request

ChemSpec has validated a balanced reaction outcome, but its reviewed structure
library has no structural graph for the product species listed below. Propose
one representative structural graph for each listed species. ChemSpec will
independently validate graph integrity, exact element inventory, charge, and
supported valence states inside an isolated working catalogue; the proposal is
untrusted until that validation passes.

Return only the JSON object required by the supplied result schema.

Rules:

- Return exactly one structure per requested species. `id` must equal the
  requested `id` and `formula` must equal the requested `formula`.
- `representation` is `molecular`, `ion`, `ionic`, or `metallic`. Choose the
  ordinary representative form of the species as written in the formula.
- An atom is `{label, element, formal_charge, non_bonding_electrons,
  unpaired_electrons}`. Labels are short unique lowercase identifiers such as
  `o1`. Electron counts must be internally consistent with the bonds you draw.
- A bond is `{left, right, order}` with order `single`, `double`, or `triple`.
- A molecular or ion structure has `atoms`, `bonds`, and `groups` (may be
  empty arrays). An ionic structure has `components`
  (`{label, atoms, bonds, groups}`) and `associations`
  (`{label, components}`). A metallic structure has `sites` (atoms) and
  `domains` (`{label, sites, delocalized_electrons}`).
- The overall structure must be net charge neutral.
- Do not add species, coefficients, mechanisms, observations, sources,
  procedures, conditions, or explanatory prose.

## Requested species

{{STRUCTURE_REQUEST_JSON}}

{{REPAIR_CONTEXT}}
