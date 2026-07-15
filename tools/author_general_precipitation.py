"""Author common precipitation outcomes by reusable ionic reassociation."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "catalogue/candidates/precipitation-general"
PREMISE = "premise.rule.precipitation.solubility-table"
STRUCTURE = "premise.structure.precipitation-ions"
OBSERVATION = "premise.observation.general-precipitate-forms"
VALENCE = "premise.valence.general-precipitation"
BOUND = [PREMISE, STRUCTURE, VALENCE]


def atom(label: str, element: str, charge: int, nonbonding: int, unpaired: int = 0) -> dict:
    return {"label": label, "element": element, "formal_charge": charge, "non_bonding_electrons": nonbonding, "unpaired_electrons": unpaired}


def mono(label: str, element: str, charge: int, nonbonding: int, unpaired: int = 0) -> dict:
    return {"label": label, "atoms": [atom(label, element, charge, nonbonding, unpaired)], "bonds": [], "groups": []}


def hydroxide(label: str) -> dict:
    return {"label": label, "atoms": [atom(f"o_{label}", "O", -1, 6), atom(f"h_{label}", "H", 0, 0)], "bonds": [{"left": f"o_{label}", "right": f"h_{label}", "order": "single"}], "groups": []}


def carbonate(label: str) -> dict:
    return {"label": label, "atoms": [atom(f"c_{label}", "C", 0, 0), atom(f"o1_{label}", "O", 0, 4), atom(f"o2_{label}", "O", -1, 6), atom(f"o3_{label}", "O", -1, 6)], "bonds": [{"left": f"c_{label}", "right": f"o1_{label}", "order": "double"}, {"left": f"c_{label}", "right": f"o2_{label}", "order": "single"}, {"left": f"c_{label}", "right": f"o3_{label}", "order": "single"}], "groups": []}


def sulfate(label: str) -> dict:
    return {"label": label, "atoms": [atom(f"s_{label}", "S", 0, 0), atom(f"o1_{label}", "O", 0, 4), atom(f"o2_{label}", "O", 0, 4), atom(f"o3_{label}", "O", -1, 6), atom(f"o4_{label}", "O", -1, 6)], "bonds": [{"left": f"s_{label}", "right": f"o1_{label}", "order": "double"}, {"left": f"s_{label}", "right": f"o2_{label}", "order": "double"}, {"left": f"s_{label}", "right": f"o3_{label}", "order": "single"}, {"left": f"s_{label}", "right": f"o4_{label}", "order": "single"}], "groups": []}


def ionic(identifier: str, formula: str, components: list[dict], trait: str) -> dict:
    return {"representation": "ionic", "id": identifier, "premise_id": STRUCTURE, "formula": formula, "components": components, "associations": [{"label": "salt", "components": [item["label"] for item in components]}], "traits": [{"trait": trait, "sites": {"reactive_site": f"{components[0]['label']}.{components[0]['atoms'][0]['label']}"}, "premise_ids": [PREMISE]}]}


def pattern(structure: dict) -> dict:
    variables, relationships = {}, []
    group_names = []
    for component_index, component in enumerate(structure["components"], 1):
        names = []
        for item in component["atoms"]:
            variables[item["label"]] = {"atom": {"element": item["element"]}}
            names.append(item["label"])
        for index, bond in enumerate(component["bonds"], 1):
            relationships.append({"kind": "covalent", "bond": f"{component['label']}Bond{index}", "left": bond["left"], "right": bond["right"], "order": bond["order"]})
        group_name = f"component{component_index}"
        group_names.append(group_name)
        relationships.append({"kind": "group_membership", "group": group_name, "atoms": names})
    relationships.append({"kind": "ionic_association", "association": "salt", "groups": group_names})
    return {"id": f"Patterns.{structure['id']}", "variables": variables, "relationships": relationships, "premise_ids": BOUND}


def component_atoms(component: dict) -> list[str]:
    return [item["label"] for item in component["atoms"]]


def local_paths(component: dict) -> list[str]:
    return [f"{component['label']}.{item['label']}" for item in component["atoms"]]


def source_atoms(role: str, instance: int, component: dict) -> list[str]:
    return [f"{role}[{instance}].{item['label']}" for item in component["atoms"]]


def precipitation_rule(case: dict, records: dict[str, dict]) -> dict:
    metal_source, reagent = records[case["metal_source"]], records[case["reagent"]]
    precipitate, spectator = records[case["precipitate"]], records["PrecipitationSodiumChloride"]
    reagent_coefficient = case["reagent_coefficient"]
    spectator_count = case["metal_charge"]
    roles = {"metalSource": {"side": "reactant", "representation": "ionic", "coefficient": 1}, "reagent": {"side": "reactant", "representation": "ionic", "coefficient": reagent_coefficient}, "precipitate": {"side": "product", "representation": "ionic", "coefficient": 1}, "spectator": {"side": "product", "representation": "ionic", "coefficient": spectator_count}}
    rewrite = [{"kind": "dissociate_ionic", "premise_ids": BOUND, "association": "metalSource[1].salt"}]
    for index in range(1, reagent_coefficient + 1):
        rewrite.append({"kind": "dissociate_ionic", "premise_ids": BOUND, "association": f"reagent[{index}].salt"})

    metal_atom = source_atoms("metalSource", 1, metal_source["components"][0])[0]
    chloride_atoms = [source_atoms("metalSource", 1, component)[0] for component in metal_source["components"][1:]]
    sodium_atoms, anion_groups = [], []
    for instance in range(1, reagent_coefficient + 1):
        for component in reagent["components"]:
            atoms = source_atoms("reagent", instance, component)
            if component["atoms"][0]["element"] == "Na":
                sodium_atoms.extend(atoms)
            else:
                anion_groups.append(atoms)
    assert len(sodium_atoms) == spectator_count == len(chloride_atoms)
    precipitate_atoms = [metal_atom, *[site for group in anion_groups for site in group]]
    rewrite.append({"kind": "associate_ionic", "premise_ids": BOUND, "label": "ionic.precipitate", "components": [[metal_atom], *anion_groups], "component_charges": [case["metal_charge"], *([case["anion_charge"]] * len(anion_groups))]})
    for index, (sodium, chloride) in enumerate(zip(sodium_atoms, chloride_atoms), 1):
        rewrite.append({"kind": "associate_ionic", "premise_ids": BOUND, "label": f"ionic.spectator{index}", "components": [[sodium], [chloride]], "component_charges": [1, -1]})
    rewrite.append({"kind": "assign_product", "premise_ids": BOUND, "atoms": precipitate_atoms, "product": "precipitate[1]"})
    for index, (sodium, chloride) in enumerate(zip(sodium_atoms, chloride_atoms), 1):
        rewrite.append({"kind": "assign_product", "premise_ids": BOUND, "atoms": [sodium, chloride], "product": f"spectator[{index}]"})

    correspondence = [{"reactant": metal_atom, "product": f"precipitate[1].{local_paths(precipitate['components'][0])[0]}", "premise_ids": BOUND}]
    product_anions = precipitate["components"][1:]
    for source_group, product_component in zip(anion_groups, product_anions):
        correspondence.extend({"reactant": source, "product": f"precipitate[1].{target}", "premise_ids": BOUND} for source, target in zip(source_group, local_paths(product_component)))
    for index, (sodium, chloride) in enumerate(zip(sodium_atoms, chloride_atoms), 1):
        correspondence.extend(({"reactant": sodium, "product": f"spectator[{index}].sodium.sodium", "premise_ids": BOUND}, {"reactant": chloride, "product": f"spectator[{index}].chloride.chloride", "premise_ids": BOUND}))
    return {"id": f"Rules.{case['id']}", "parameters": {"outcome": {"kind": "enum", "values": ["precipitation"]}}, "roles": roles, "reactants": {"metalSource": {"kind": "exact", "structure": metal_source["id"]}, "reagent": {"kind": "exact", "structure": reagent["id"]}}, "cases": [{"status": "supported", "id": "precipitation", "when": {"kind": "parameter_equals", "parameter": "outcome", "value": "precipitation"}, "products": {"precipitate": {"kind": "exact", "structure": precipitate["id"]}, "spectator": {"kind": "exact", "structure": spectator["id"]}}, "patterns": {"metalSource": f"Patterns.{metal_source['id']}", "reagent": f"Patterns.{reagent['id']}"}, "correspondence": correspondence, "rewrite": rewrite, "observation_compatibility": [{"subject_role": "precipitate", "predicate": "forms", "evidence_subject": case["product_name"], "premise_id": OBSERVATION}], "premise_ids": [PREMISE, STRUCTURE, VALENCE, OBSERVATION]}], "applicability": {"premise_id": PREMISE, "request_relation": "contact", "required_context": "both ionic reactant graphs and an insoluble product are present in the reviewed solubility table"}, "model_assumptions": {"event": "representative", "sequence": "explanatory", "premise_ids": [PREMISE]}, "premise_ids": [PREMISE, STRUCTURE, VALENCE, OBSERVATION]}


def main() -> None:
    sodium = lambda label: mono(label, "Na", 1, 0)
    chloride = lambda label: mono(label, "Cl", -1, 8)
    structures = [
        ionic("PrecipitationSodiumChloride", "NaCl", [sodium("sodium"), chloride("chloride")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationBariumChloride", "BaCl2", [mono("cation", "Ba", 2, 0), chloride("chloride1"), chloride("chloride2")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationCalciumChloride", "CaCl2", [mono("cation", "Ca", 2, 0), chloride("chloride1"), chloride("chloride2")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationCopperChloride", "CuCl2", [mono("cation", "Cu", 2, 9, 1), chloride("chloride1"), chloride("chloride2")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationIronChloride", "FeCl3", [mono("cation", "Fe", 3, 5, 5), chloride("chloride1"), chloride("chloride2"), chloride("chloride3")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationSodiumSulfate", "Na2SO4", [sodium("sodium1"), sodium("sodium2"), sulfate("sulfate")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationSodiumCarbonate", "Na2CO3", [{"label": "sodiums", "atoms": [atom("sodium1", "Na", 1, 0), atom("sodium2", "Na", 1, 0)], "bonds": [], "groups": []}, carbonate("carbonate")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationSodiumHydroxide", "NaOH", [sodium("sodium"), hydroxide("hydroxide")], "Traits.SolubleIonicReactant"),
        ionic("PrecipitationBariumSulfate", "BaSO4", [mono("cation", "Ba", 2, 0), sulfate("sulfate")], "Traits.InsolubleIonicProduct"),
        ionic("PrecipitationCalciumCarbonate", "CaCO3", [mono("cation", "Ca", 2, 0), carbonate("carbonate")], "Traits.InsolubleIonicProduct"),
        ionic("PrecipitationCopperHydroxide", "CuO2H2", [mono("cation", "Cu", 2, 9, 1), hydroxide("hydroxide1"), hydroxide("hydroxide2")], "Traits.InsolubleIonicProduct"),
        ionic("PrecipitationIronHydroxide", "FeO3H3", [mono("cation", "Fe", 3, 5, 5), hydroxide("hydroxide1"), hydroxide("hydroxide2"), hydroxide("hydroxide3")], "Traits.InsolubleIonicProduct"),
    ]
    records = {item["id"]: item for item in structures}
    cases = [
        {"id": "BariumSulfatePrecipitation", "metal_source": "PrecipitationBariumChloride", "reagent": "PrecipitationSodiumSulfate", "precipitate": "PrecipitationBariumSulfate", "reagent_coefficient": 1, "metal_charge": 2, "anion_charge": -2, "product_name": "barium sulfate"},
        {"id": "CalciumCarbonatePrecipitation", "metal_source": "PrecipitationCalciumChloride", "reagent": "PrecipitationSodiumCarbonate", "precipitate": "PrecipitationCalciumCarbonate", "reagent_coefficient": 1, "metal_charge": 2, "anion_charge": -2, "product_name": "calcium carbonate"},
        {"id": "CopperHydroxidePrecipitation", "metal_source": "PrecipitationCopperChloride", "reagent": "PrecipitationSodiumHydroxide", "precipitate": "PrecipitationCopperHydroxide", "reagent_coefficient": 2, "metal_charge": 2, "anion_charge": -1, "product_name": "copper hydroxide"},
        {"id": "IronHydroxidePrecipitation", "metal_source": "PrecipitationIronChloride", "reagent": "PrecipitationSodiumHydroxide", "precipitate": "PrecipitationIronHydroxide", "reagent_coefficient": 3, "metal_charge": 3, "anion_charge": -1, "product_name": "iron(III) hydroxide"},
    ]
    evidence_url = "https://openstax.org/books/chemistry-2e/pages/4-2-classifying-chemical-reactions"
    evidence = [{"id": "evidence.openstax.precipitation", "title": "Chemistry 2e", "publisher": "OpenStax", "locator": "Precipitation Reactions and Solubility Guidelines", "reference": evidence_url, "publication_date": "2019-02-14", "retrieved_on": "2026-07-15", "usage": "Reviewed common insoluble ionic products"}]
    premise = lambda identifier, statement: {"id": identifier, "statement": statement, "evidence": ["evidence.openstax.precipitation"], "review": {"status": "provisional", "reviewers": []}, "rule_version": "1"}
    state_set = set()
    for structure in structures:
        for component in structure["components"]:
            bond_sums = {item["label"]: 0 for item in component["atoms"]}
            for bond in component["bonds"]:
                order = {"single": 1, "double": 2, "triple": 3}[bond["order"]]
                bond_sums[bond["left"]] += order
                bond_sums[bond["right"]] += order
            for item in component["atoms"]:
                state_set.add((item["element"], item["formal_charge"], item["non_bonding_electrons"], item["unpaired_electrons"], bond_sums[item["label"]]))
    neutral = {"H": 1, "C": 4, "O": 6, "S": 6, "Na": 1, "Cl": 7, "Ba": 2, "Ca": 2, "Cu": 11, "Fe": 8}
    valence = {"premise_id": VALENCE, "neutral_valence": [{"element": element, "neutral_valence_electrons": value} for element, value in neutral.items()], "supported_states": [{"element": element, "formal_charge": charge, "non_bonding_electrons": nonbonding, "unpaired_electrons": unpaired, "covalent_bond_order_sum": bond_sum} for element, charge, nonbonding, unpaired, bond_sum in sorted(state_set)], "metallic_domain_states": []}
    document = {"schema_version": 1, "id": "precipitation-general", "evidence": evidence, "premises": [premise(PREMISE, "The finite reviewed solubility table produces BaSO4, CaCO3, Cu(OH)2 and Fe(OH)3 by ionic reassociation."), premise(STRUCTURE, "Reactant salts and precipitates preserve explicit monatomic or covalently bonded polyatomic ion groups during ionic reassociation."), premise(VALENCE, "The listed monatomic and polyatomic ion states form the closed structural domain for these reassociations."), premise(OBSERVATION, "Formation of the selected insoluble ionic product is compatible with the theoretical precipitation outcome.")], "valence_premises": [valence], "structures": structures, "rules": [], "elements": [], "element_categories": [], "structural_traits": [], "structure_templates": [], "structure_applications": [], "graph_patterns": [pattern(item) for item in structures], "generalized_rules": [precipitation_rule(case, records) for case in cases]}
    PACKAGE.mkdir(parents=True, exist_ok=True)
    (PACKAGE / "candidate.json").write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    registry_path = ROOT / "catalogue/experience-registry.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    registry["experiences"] = [item for item in registry["experiences"] if not item["id"].startswith("precipitation-general-")]
    for case in cases:
        source_record, reagent, product = records[case["metal_source"]], records[case["reagent"]], records[case["precipitate"]]
        slug = f"precipitation-general-{case['id'].lower()}"
        source_path, evidence_path = f"conformance/end-to-end/{slug}-001.chems", f"conformance/observations/{slug}-001.evidence.json"
        rc, sc = case["reagent_coefficient"], case["metal_charge"]
        chems = f"""chems 1
use catalog ChemSpec.Theoretical@1
reaction {case['id']} where
  reactants
    metalSource := 1 of {source_record['id']}
    reagent := {rc} of {reagent['id']}
  products
    precipitate := 1 of {product['id']}
    spectator := {sc} of PrecipitationSodiumChloride
  equation
    1 {source_record['formula']}[ionic] + {rc} {reagent['formula']}[ionic]
    -> 1 {product['formula']}[ionic] + {sc} NaCl[ionic]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.{case['id']}@1
    product precipitate forms claim R1
  by
    apply Rules.{case['id']}
      metalSource := metalSource
      reagent := reagent
      precipitate := precipitate
      spectator := spectator
"""
        packet = {"schema_version": 1, "id": f"Evidence.{case['id']}@1", "claims": [{"id": "R1", "subject_role": "product", "subject": case["product_name"], "predicate": "forms", "sources": ["S1"]}], "sources": [{"id": "S1", "title": "Precipitation reactions", "publisher": "OpenStax", "url": evidence_url, "supports": ["R1"]}]}
        (ROOT / source_path).write_text(chems, encoding="utf-8", newline="\n")
        (ROOT / evidence_path).write_text(json.dumps(packet, indent=2) + "\n", encoding="utf-8")
        registry["experiences"].append({"id": slug, "status": "trusted", "family": "precipitation", "participants": [{"kind": "composition", "formula": source_record["formula"]}, {"kind": "composition", "formula": reagent["formula"]}], "source_path": source_path, "evidence_path": evidence_path, "request": f"What precipitate forms from {source_record['formula']} and {reagent['formula']}?", "equation": f"{source_record['formula']} + {'' if rc == 1 else rc}{reagent['formula']} -> {product['formula']} + {'' if sc == 1 else sc}NaCl", "subject_name": source_record["formula"], "product_name": case["product_name"], "product_structure": product["id"]})
        if case["id"] == "BariumSulfatePrecipitation":
            (PACKAGE / "example.chems").write_text(chems, encoding="utf-8", newline="\n")
            (PACKAGE / "evidence.json").write_text(json.dumps(packet, indent=2) + "\n", encoding="utf-8")
    registry_path.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
