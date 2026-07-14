# Slice 3 chemistry review handoff

This is the external acceptance gate for fixed Slice 3. It does not authorize
changes to the catalogue and does not expand the closed chemistry domain.

## Artifact under review

- Catalogue: `conformance/catalogue/lithium-rule-001.catalogue.json`
- Canonical catalogue digest:
  `cdf8afe54409acf1a4aa76ad772bd3e26207608f90cd0ee4c2f6f2ec0cf0bb4f`
- Review request: `conformance/catalogue/lithium-rule-001.review.json`
- Attestation schema: `schemas/chem-catalogue-review-1.schema.json`

Any catalogue change creates a different digest and requires a fresh review.

## Required chemistry checks

Review the exact JSON, including:

1. the lithium-metal site-core and delocalized-electron representation;
2. water, lithium-hydroxide, and hydrogen atom/electron structures;
3. every Li/H/O supported valence tuple;
4. the balanced `Rules.AlkaliMetalWithWater` reactant and product patterns;
5. the total element-preserving atom map;
6. every operation's before/after charge, non-bonding-electron, radical, bond,
   metallic-domain, and ionic-component declarations;
7. product assignments and final structural identities;
8. gas-evolution and lithium-disappearance compatibility facts; and
9. the disclosure that the ordered operations are explanatory, not a claimed
   elementary mechanism or kinetic model.

The review is limited to this exact lithium-water closed outcome. It does not
approve other alkali metals, laboratory instructions, kinetics, quantities,
or unlisted structures and rules.

## Required attestation

If the artifact is accepted, replace the pending review request with a JSON
attestation matching the schema and this shape:

```json
{
  "schema_version": 1,
  "id": "review.lithium-rule-001",
  "catalogue_digest": "cdf8afe54409acf1a4aa76ad772bd3e26207608f90cd0ee4c2f6f2ec0cf0bb4f",
  "reviewer": "REVIEWER NAME",
  "reviewed_on": "YYYY-MM-DD",
  "scope": "Exact closed lithium-water structural catalogue and rule",
  "method": "How the structures, mapping, states, and operations were checked",
  "sources": [
    "evidence.iupac.goldbook",
    "evidence.openstax.chemistry-2e"
  ],
  "premises": [
    "premise.observation.hydrogen-evolves",
    "premise.observation.lithium-disappears",
    "premise.rule.lithium-water.standard-outcome",
    "premise.structure.hydrogen",
    "premise.structure.lithium-hydroxide",
    "premise.structure.lithium-metal",
    "premise.structure.water",
    "premise.valence.li-h-o.initial-domain"
  ],
  "coverage_conclusion": "The reviewer's explicit conclusion",
  "limitation": "Only the exact closed lithium-water outcome is covered"
}
```

The application pins the canonical semantic digest of this attestation, so
whitespace and ordering of `sources` or `premises` do not affect review
identity. Until the completed attestation is independently supplied and its
digest is pinned in code, `TrustedCatalogue` construction remains impossible.
Later slices may be implemented and tested through the explicitly untrusted
review-candidate path, but no result may be represented as production-trusted.
