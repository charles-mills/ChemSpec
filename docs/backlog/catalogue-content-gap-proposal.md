# Proposal: trusted-catalogue content additions (UNATTESTED — needs chemistry review)

Status: **draft, not reviewed, not attested**. Nothing here may enter
`catalogue/reference/` until a chemist reviews it and `review.json` is
re-attested against the regenerated aggregate digest.

Provenance: gaps found by `cargo run -p agent --bin corpus-expectation-audit`
(see `docs/plans/dynamic-reaction-rebuild-plan.md`). Corpus cases currently expect
`invalid` for these inputs — that is honest and intentional; do not change
expectations before content lands.

Out of scope here: organic reactant structures (CH4, C2H4, C2H5OH, C6H12O6)
— those are the DYN-105 identity-adapter roadmap, not catalogue content.

---

## 1. Extend the alkali-metal family to Rb and Cs

Edits to `catalogue/candidates/periodic-table-and-alkali-water/candidate.json`.
`Rules.AlkaliMetalWithWater`, both templates, and the graph patterns are
parameterized over `Categories.AlkaliMetal`, so extending membership plus
adding the four applications is the whole change — no new rule needed.

### 1a. Category membership

```json
{
  "id": "Categories.AlkaliMetal",
  "subject": "element",
  "membership": { "kind": "explicit", "members": ["Cs", "K", "Li", "Na", "Rb"] },
  "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"]
}
```

### 1b. New structure applications (append to `structure_applications`)

```json
{ "id": "RubidiumMetal",     "template": "Templates.AlkaliMetal",          "arguments": { "member": "Rb" }, "formula": "Rb",   "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"] }
{ "id": "RubidiumHydroxide", "template": "Templates.AlkaliMetalHydroxide", "arguments": { "member": "Rb" }, "formula": "RbOH", "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"] }
{ "id": "CaesiumMetal",      "template": "Templates.AlkaliMetal",          "arguments": { "member": "Cs" }, "formula": "Cs",   "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"] }
{ "id": "CaesiumHydroxide",  "template": "Templates.AlkaliMetalHydroxide", "arguments": { "member": "Cs" }, "formula": "CsOH", "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"] }
```

### 1c. Premise rewording (triggers re-review by design)

`premise.rule.alkali-metal-water.standard-outcome` currently reads:

> Contact between Li, Na, or K metal and water has the reviewed
> representative outcome 2 M + 2 H2O -> 2 MOH + H2.

Proposed: "Contact between Li, Na, K, Rb, or Cs metal and water has the
reviewed representative outcome 2 M + 2 H2O -> 2 MOH + H2." Bump
`rule_version`. Add an evidence entry citing a source that covers Rb/Cs
explicitly (e.g. OpenStax Chemistry 2e, "Occurrence and Preparation of the
Representative Metals" / RSC Group 1 page) — the current
`evidence.openstax.chemistry-2e` locator covers periodic variations, which
the chemist should confirm suffices for Rb/Cs or supplement.

### Chemist decisions

- **Fr is deliberately excluded** (recommendation): no bulk observation of
  Fr + water exists; the corpus honestly escalates Fr to the model. Confirm
  or override.
- Confirm the shared vigor-agnostic observations
  (`premise.observation.alkali-metal-disappears`, `hydrogen-evolves`) remain
  acceptable for Rb/Cs, whose reactions are explosive — or whether
  member-specific observation notes are wanted.

---

## 2. Gold metal structure (identity only, no reaction rules)

Goal: Au + H2O resolves identity and yields an honest **no-reaction**
outcome, replacing the Cu + H2O corpus workaround.

**Recommended shape: a standalone structure, not a G11 category extension.**
Extending `Categories.TransitionG11HemioxideElement` /
`TransitionG11MonoxideElement` to Au would also assert Au2O/AuO oxygen
outcomes, which is chemically wrong for a metal that does not react with
oxygen. A standalone structure carries identity with no outcome claims.

New small candidate package `catalogue/candidates/noble-metal-identity/`
(candidate.json + example.chems + evidence.json), containing one structure
mirroring the `CalciumMetal` shape and the G11 valence convention
(formal charge = delocalized electrons = neutral valence electrons = 11
for Au, matching `Templates.TransitionG11HemioxideMetal`'s treatment of Cu):

```json
{
  "representation": "metallic",
  "id": "GoldMetal",
  "premise_id": "premise.structure.gold-metal",
  "formula": "Au",
  "sites": [
    { "label": "au", "element": "Au", "formal_charge": 11, "non_bonding_electrons": 0, "unpaired_electrons": 0 }
  ],
  "domains": [
    { "label": "metallic", "sites": ["au"], "delocalized_electrons": 11 }
  ]
}
```

With premises (both `provisional`, no reviewers):

- `premise.structure.gold-metal` — "ChemSpec represents gold metal as an
  Au site core with eleven domain-owned delocalized valence electrons for
  explanatory execution." (evidence: IUPAC Gold Book metallic bond +
  ChemSpec design authority, mirroring existing structure premises)
- `premise.observation.gold-water-inert` — "Contact between gold metal and
  water produces no reaction under the reviewed conditions." (evidence:
  chemist to select; RSC/OpenStax noble-metal reactivity)

### Chemist decisions

- Confirm the 11-electron metallic convention (vs. treating Au as
  one delocalized 6s electron); consistency with the reviewed Cu treatment
  argues for 11.
- Confirm whether an explicit no-reaction premise/observation is wanted, or
  whether identity resolution alone (with the kernel's default no-match →
  no-reaction path) is the honest representation. Check with how Cu + H2O
  no-reaction currently resolves — mirror that.

---

## 3. Sulfate species and metal-displacement family (Zn + CuSO4 class)

Largest item — a new candidate package
`catalogue/candidates/single-displacement-metal-sulfate/`. Sketch below is
lower-fidelity than §1–2; the engineering wiring (graph patterns,
correspondence maps) follows the `single-displacement-halogen` package once
the chemistry is fixed.

### 3a. Sulfate anion representation

Mirroring the `SilverNitrate` precedent (charge-separated octet form, no
expanded octet — nitrate uses N+ with one double bond): S with formal
charge +2, four single-bonded O⁻, net −2.

```json
{
  "label": "sulfate",
  "atoms": [
    { "label": "s",  "element": "S", "formal_charge": 2,  "non_bonding_electrons": 0, "unpaired_electrons": 0 },
    { "label": "o1", "element": "O", "formal_charge": -1, "non_bonding_electrons": 6, "unpaired_electrons": 0 },
    { "label": "o2", "element": "O", "formal_charge": -1, "non_bonding_electrons": 6, "unpaired_electrons": 0 },
    { "label": "o3", "element": "O", "formal_charge": -1, "non_bonding_electrons": 6, "unpaired_electrons": 0 },
    { "label": "o4", "element": "O", "formal_charge": -1, "non_bonding_electrons": 6, "unpaired_electrons": 0 }
  ],
  "bonds": [
    { "left": "s", "right": "o1", "order": "single" },
    { "left": "s", "right": "o2", "order": "single" },
    { "left": "s", "right": "o3", "order": "single" },
    { "left": "s", "right": "o4", "order": "single" }
  ],
  "groups": []
}
```

**Chemist decision:** charge-separated octet form (above, consistent with
the reviewed nitrate) vs. expanded-octet form with two S=O double bonds.
Pick one; it must also match the valence-domain `supported_states` for S.

### 3b. Species

Template `Templates.DivalentMetalSulfate` over a new explicit category
`Categories.SulfateDisplacementMetal` (proposed members: **Zn, Cu** —
minimal set covering the corpus class; chemist may extend to Mg/Fe), each
an ionic association of M²⁺ (formal_charge 2) with the sulfate component,
formulas `ZnSO4` / `CuSO4`. Plus `Templates.DivalentMetal` applications for
the elemental metals (ZincMetal; CopperMetal exists only as a template-bound
oxygen experience today — a displacement-facing application is still
needed).

### 3c. Rule

`Rules.MetalSulfateDisplacement`, shaped like `Rules.HalogenDisplacement`
(bounded order, member-pair cases):

- Bounded aqueous activity order: **Zn > Cu** only. No general activity
  series — supported case `Zn + CuSO4 -> ZnSO4 + Cu`; the reverse contact
  (`Cu + ZnSO4`) is an explicit no-reaction case, mirroring how the halogen
  package bounds Cl > Br > I.
- Observations (chemist to confirm wording + evidence): blue solution fades /
  colourless Zn²⁺ solution forms; reddish copper solid deposits; zinc solid
  diminishes. Evidence: OpenStax Chemistry 2e single-displacement /
  activity-series section; RSC.

### Chemist decisions

- Member set and activity bound (Zn > Cu only, or wider?).
- Anhydrous CuSO4 vs. aqueous Cu²⁺(aq) colour attribution in observations.
- Sulfate Lewis form (§3a).

---

## Process checklist (after chemist sign-off)

1. Apply §1 edits; create §2 and §3 candidate packages
   (`candidate.json` + `example.chems` + `evidence.json` each; premises
   `provisional` with no reviewers).
2. `cargo run -p chems-cli -- catalogue check --out <dir> <all packages>`.
3. Chemist review → regenerate trusted aggregate under
   `catalogue/reference/core-chemistry/` → re-attest `review.json`
   (new `catalogue_digest`, updated scope text) and `promotion.json`.
4. Update pinned digests in `chem-catalogue`.
5. Re-run `cargo run -p agent --bin corpus-expectation-audit`; regenerate
   corpus expectations (Au + H2O → no-reaction replaces the Cu + H2O
   workaround; Rb/Cs + water → resolved family; Zn + CuSO4 → supported).
