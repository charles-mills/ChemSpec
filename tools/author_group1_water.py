"""Extend the generalized Group 1 + water family to Rb and Cs.

Francium remains explicitly outside the executable family because the project
does not have a reviewed practical reaction model for it.
"""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CANDIDATE = ROOT / "catalogue/candidates/periodic-table-and-alkali-water/candidate.json"
REGISTRY = ROOT / "catalogue/experience-registry.json"
MEMBERS = (
    ("Rubidium", "rubidium", "Rb", 37),
    ("Caesium", "caesium", "Cs", 55),
)


def main() -> None:
    document = json.loads(CANDIDATE.read_text(encoding="utf-8"))
    category = next(c for c in document["element_categories"] if c["id"] == "Categories.AlkaliMetal")
    # Preserve the original three-member category used by the bounded acid,
    # carbonate, and precipitation slices. Family domains must not widen as a
    # side effect of adding water chemistry.
    category["membership"]["members"] = ["K", "Li", "Na"]
    document["element_categories"] = [
        c for c in document["element_categories"]
        if c["id"] != "Categories.WaterReactiveAlkaliMetal"
    ]
    document["element_categories"].append({
        "id": "Categories.WaterReactiveAlkaliMetal",
        "subject": "element",
        "membership": {"kind": "explicit", "members": ["Cs", "K", "Li", "Na", "Rb"]},
        "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"],
    })
    for template_id in ("Templates.AlkaliMetal", "Templates.AlkaliMetalHydroxide"):
        template = next(t for t in document["structure_templates"] if t["id"] == template_id)
        template["parameters"]["member"]["category"] = "Categories.WaterReactiveAlkaliMetal"
    rule = next(r for r in document["generalized_rules"] if r["id"] == "Rules.AlkaliMetalWithWater")
    rule["parameters"]["member"]["category"] = "Categories.WaterReactiveAlkaliMetal"

    premise = next(p for p in document["premises"] if p["id"] == "premise.rule.alkali-metal-water.standard-outcome")
    premise["statement"] = "Contact between Li, Na, K, Rb, or Cs metal and water has the reviewed representative outcome 2 M + 2 H2O -> 2 MOH + H2."

    valence = next(v for v in document["valence_premises"] if v["premise_id"] == "premise.valence.alkali-h-o.initial-domain")
    for symbol in ("Rb", "Cs"):
        if not any(v["element"] == symbol for v in valence["neutral_valence"]):
            valence["neutral_valence"].append({"element": symbol, "neutral_valence_electrons": 1})
        states = [
            {"element": symbol, "formal_charge": 1, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 0},
            {"element": symbol, "formal_charge": 0, "non_bonding_electrons": 1, "unpaired_electrons": 1, "covalent_bond_order_sum": 0},
        ]
        for state in states:
            if state not in valence["supported_states"]:
                valence["supported_states"].append(state)
        metallic = {"element": symbol, "site_formal_charge": 1, "site_local_electrons": 0, "delocalized_electrons_per_site": 1}
        if metallic not in valence["metallic_domain_states"]:
            valence["metallic_domain_states"].append(metallic)

    applications = document["structure_applications"]
    applications[:] = [a for a in applications if a["id"] not in {"RubidiumMetal", "CaesiumMetal", "RubidiumHydroxide", "CaesiumHydroxide"}]
    for name, lower, symbol, _ in MEMBERS:
        applications.extend((
            {"id": f"{name}Metal", "template": "Templates.AlkaliMetal", "arguments": {"member": symbol}, "formula": symbol, "aliases": [lower], "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"]},
            {"id": f"{name}Hydroxide", "template": "Templates.AlkaliMetalHydroxide", "arguments": {"member": symbol}, "formula": f"{symbol}OH", "aliases": [f"{lower}-hydroxide"], "premise_ids": ["premise.rule.alkali-metal-water.standard-outcome"]},
        ))
    CANDIDATE.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")

    registry = json.loads(REGISTRY.read_text(encoding="utf-8"))
    ids = {f"alkali-water-{lower}" for _, lower, _, _ in MEMBERS}
    registry["experiences"] = [e for e in registry["experiences"] if e["id"] not in ids]
    for name, lower, symbol, atomic_number in MEMBERS:
        source_path = f"conformance/end-to-end/alkali-water-{lower}-001.chems"
        evidence_path = f"conformance/observations/alkali-water-{lower}-001.evidence.json"
        source = f"""chems 1
use catalog ChemSpec.Theoretical@1

reaction {name}AndWater where
  reactants
    metal := 2 of {name}Metal
    water := 2 of Water
  products
    hydroxide := 2 of {name}Hydroxide
    hydrogen := 1 of Hydrogen
  equation
    2 {symbol}[metallic] + 2 H2O[molecular]
    -> 2 {symbol}OH[ionic] + H2[molecular]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.AlkaliWater{name}@1
    gas hydrogen evolves claim R1
    reactant metal disappears claim R2
  by
    apply Rules.AlkaliMetalWithWater
      metal := metal
      water := water
      hydroxide := hydroxide
      gasProduct := hydrogen
"""
        evidence = {
            "schema_version": 1,
            "id": f"Evidence.AlkaliWater{name}@1",
            "claims": [
                {"id": "R1", "subject_role": "gas", "subject": "hydrogen", "predicate": "evolves", "sources": ["S1"]},
                {"id": "R2", "subject_role": "reactant", "subject": "alkali metal", "predicate": "disappears", "sources": ["S1"]},
            ],
            "sources": [{"id": "S1", "title": "Reactions of Group 1 Elements with Water", "publisher": "Chemistry LibreTexts", "url": "https://chem.libretexts.org/Bookshelves/Inorganic_Chemistry/Supplemental_Modules_and_Websites_(Inorganic_Chemistry)/Descriptive_Chemistry/Elements_Organized_by_Block/1_s-Block_Elements/Group_1%3A_The_Alkali_Metals/2Reactions_of_the_Group_1_Elements/Reactions_of_Group_1_Elements_with_Water", "supports": ["R1", "R2"]}],
        }
        (ROOT / source_path).write_text(source, encoding="utf-8", newline="\n")
        (ROOT / evidence_path).write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        registry["experiences"].append({
            "id": f"alkali-water-{lower}", "status": "trusted", "family": "alkali_water",
            "participants": [{"kind": "element", "atomic_number": atomic_number}, {"kind": "composition", "formula": "H2O"}],
            "source_path": source_path, "evidence_path": evidence_path,
            "request": f"What happens when {lower} reacts with water?",
            "equation": f"2{symbol} + 2H2O -> 2{symbol}OH + H2", "subject_name": lower,
        })
    REGISTRY.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
