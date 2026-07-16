# Finite covalent-combination catalogue

ChemSpec supports 20 reviewed, representative covalent outcomes between two
element selections. A periodic-table selection denotes the element's reviewed
standard molecular state (`H₂`, `N₂`, `F₂`, `S₈`, `Cl₂`, `Br₂`, or `I₂` where
applicable); it does not denote an isolated atom or invite runtime valence
guessing.

The source package is `catalogue/candidates/covalent-combinations`. Seven
generalized rules cover the finite matrix below, while each product has an
explicit trusted structural graph and its own end-to-end `.chems 1` fixture.

| Selected elements | Reviewed products |
| --- | --- |
| H + F | HF |
| H + Cl | HCl |
| H + Br | HBr |
| H + I | HI |
| H + N | NH₃ |
| H + S | H₂S |
| Cl + F | ClF, ClF₃, ClF₅ |
| Br + F | BrF, BrF₃, BrF₅ |
| I + F | IF, IF₃, IF₅, IF₇ |
| Br + Cl | BrCl |
| I + Cl | ICl, ICl₃ |
| I + Br | IBr |

The hydrogen equations and direct-combination boundaries are sourced from
[OpenStax Chemistry: Atoms First, section 18.5](https://openstax.org/books/chemistry-atoms-first/pages/18-5-occurrence-preparation-and-compounds-of-hydrogen).
The interhalogen formula families, elemental preparation, and localized
single-bond star graphs are sourced from
[OpenStax Chemistry 2e, section 18.11](https://openstax.org/books/chemistry-2e/pages/18-11-occurrence-preparation-and-properties-of-halogens).

## Trust and runtime behaviour

- The generator enumerates only the reviewed element pairs and product
  identities above. Categories constrain its reusable rules; they do not grant
  unlisted category members runtime support.
- Every rule cleaves explicit elemental bonds, forms explicit covalent bonds,
  assigns every product atom, and passes kernel electron and charge
  conservation.
- Product selection remains explicit when one reactant pair has several
  reviewed outcomes. The application never chooses between them heuristically.
- Reactant tooltips and product-choice cards resolve structural graphs by exact
  trusted identity. Formula ambiguity fails closed.
- The IF₇ symmetry proof is bounded. Raw graph matching retains its existing
  4,096-result limit; only complete structure-automorphism and certificate
  checks receive the finite allowance required by two 7-coordinate products.

Regenerate the package, fixtures, evidence files, and registry records with:

```sh
python3 tools/generate-covalent-catalogue.py
```

The trusted aggregate contains 205 experiences after promotion: 36 established
finite bindings, 68 oxygen outcomes, 81 fixed-charge ionic pairs, and these 20
covalent combinations.
