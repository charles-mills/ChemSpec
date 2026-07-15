"""Author standard divalent-metal displacement as catalogue graph rewrites."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "catalogue/candidates/metal-displacement"
PREMISE = "premise.rule.metal-displacement.activity-series"
STRUCTURE = "premise.structure.metal-displacement-chlorides"
VALENCE = "premise.valence.metal-displacement"
OBSERVATION = "premise.observation.displacement-product-forms"
BOUND = [PREMISE, STRUCTURE, VALENCE]
METALS = {
    "Mg": {"name": "Magnesium", "number": 12, "valence": 2, "ion_unpaired": 0, "rank": 4},
    "Zn": {"name": "Zinc", "number": 30, "valence": 12, "ion_unpaired": 0, "rank": 3},
    "Fe": {"name": "Iron", "number": 26, "valence": 8, "ion_unpaired": 4, "rank": 2},
    "Cu": {"name": "Copper", "number": 29, "valence": 11, "ion_unpaired": 1, "rank": 1},
}


def op(kind: str, **values) -> dict:
    return {"kind": kind, "premise_ids": BOUND, **values}


def structures(symbol: str, data: dict) -> tuple[dict, dict]:
    metal = {"representation": "metallic", "id": f"Displacement{data['name']}Metal", "premise_id": STRUCTURE, "formula": symbol, "sites": [{"label": "metal", "element": symbol, "formal_charge": data["valence"], "non_bonding_electrons": 0, "unpaired_electrons": 0}], "domains": [{"label": "metallic", "sites": ["metal"], "delocalized_electrons": data["valence"]}], "traits": [{"trait": "Traits.ElementalMetalReactant", "sites": {"reactive_site": "metal"}, "premise_ids": [PREMISE]}]}
    chloride = {"representation": "ionic", "id": f"Displacement{data['name']}Chloride", "premise_id": STRUCTURE, "formula": f"{symbol}Cl2", "components": [
        {"label": "cation", "atoms": [{"label": "metal", "element": symbol, "formal_charge": 2, "non_bonding_electrons": data["valence"] - 2, "unpaired_electrons": data["ion_unpaired"]}], "bonds": [], "groups": []},
        {"label": "anion1", "atoms": [{"label": "cl1", "element": "Cl", "formal_charge": -1, "non_bonding_electrons": 8, "unpaired_electrons": 0}], "bonds": [], "groups": []},
        {"label": "anion2", "atoms": [{"label": "cl2", "element": "Cl", "formal_charge": -1, "non_bonding_electrons": 8, "unpaired_electrons": 0}], "bonds": [], "groups": []},
    ], "associations": [{"label": "salt", "components": ["cation", "anion1", "anion2"]}], "traits": [{"trait": "Traits.SolubleIonicReactant", "sites": {"reactive_site": "cation.metal"}, "premise_ids": [PREMISE]}]}
    return metal, chloride


def patterns(symbol: str, data: dict) -> tuple[dict, dict]:
    metal = {"id": f"Patterns.Displacement{data['name']}Metal", "variables": {"metal": {"atom": {"element": symbol}}}, "relationships": [{"kind": "metallic_domain", "domain": "metallic", "sites": ["metal"], "delocalized_electrons": data["valence"]}], "premise_ids": BOUND}
    salt = {"id": f"Patterns.Displacement{data['name']}Chloride", "variables": {"metal": {"atom": {"element": symbol}}, "cl1": {"atom": {"element": "Cl"}}, "cl2": {"atom": {"element": "Cl"}}}, "relationships": [
        {"kind": "group_membership", "group": "cation", "atoms": ["metal"]},
        {"kind": "group_membership", "group": "anion1", "atoms": ["cl1"]},
        {"kind": "group_membership", "group": "anion2", "atoms": ["cl2"]},
        {"kind": "ionic_association", "association": "salt", "groups": ["cation", "anion1", "anion2"]},
    ], "premise_ids": BOUND}
    return metal, salt


def rule(displacing: str, displaced: str) -> dict:
    source, target = METALS[displacing], METALS[displaced]
    a, b = source["valence"], target["valence"]
    au, bu = source["ion_unpaired"], target["ion_unpaired"]
    roles = {"displacing": {"side": "reactant", "representation": "metallic", "coefficient": 1}, "saltSource": {"side": "reactant", "representation": "ionic", "coefficient": 1}, "saltProduct": {"side": "product", "representation": "ionic", "coefficient": 1}, "metalProduct": {"side": "product", "representation": "metallic", "coefficient": 1}}
    rewrite = [
        op("dissociate_ionic", association="saltSource[1].salt"),
        op("release_metallic", site="displacing[1].metal", domain="displacing[1].metallic", allocation="retain_electron", before={"site": [a, 0, 0], "domain_electrons": a}, after={"site": [0, a, a], "domain_electrons": 0}),
        op("transfer_electron", count=2, donor="displacing[1].metal", acceptor="saltSource[1].metal", before={"donor": [0, a, a], "acceptor": [2, b - 2, bu]}, after={"donor": [2, a - 2, a - 2], "acceptor": [0, b, bu + 2]}),
    ]
    if a - 2 != au:
        rewrite.append(op("reconfigure_electrons", atom="displacing[1].metal", before=[2, a - 2, a - 2], after=[2, a - 2, au]))
    rewrite.append(op("reconfigure_electrons", atom="saltSource[1].metal", before=[0, b, bu + 2], after=[0, b, b]))
    rewrite.extend((
        op("associate_ionic", label="ionic.displacement-product", components=[["displacing[1].metal"], ["saltSource[1].cl1"], ["saltSource[1].cl2"]], component_charges=[2, -1, -1]),
        op("join_metallic", site="saltSource[1].metal", domain="saltSource[1].metallicProduct", allocation="donate_electron", before={"site": [0, b, b], "domain_electrons": 0}, after={"site": [b, 0, 0], "domain_electrons": b}),
        op("assign_product", atoms=["displacing[1].metal", "saltSource[1].cl1", "saltSource[1].cl2"], product="saltProduct[1]"),
        op("assign_product", atoms=["saltSource[1].metal"], product="metalProduct[1]"),
    ))
    correspondence = [
        {"reactant": "displacing[1].metal", "product": "saltProduct[1].cation.metal", "premise_ids": BOUND},
        {"reactant": "saltSource[1].cl1", "product": "saltProduct[1].anion1.cl1", "premise_ids": BOUND},
        {"reactant": "saltSource[1].cl2", "product": "saltProduct[1].anion2.cl2", "premise_ids": BOUND},
        {"reactant": "saltSource[1].metal", "product": "metalProduct[1].metal", "premise_ids": BOUND},
    ]
    return {"id": f"Rules.{source['name']}Displaces{target['name']}", "parameters": {"outcome": {"kind": "enum", "values": ["displacement"]}}, "roles": roles, "reactants": {"displacing": {"kind": "exact", "structure": f"Displacement{source['name']}Metal"}, "saltSource": {"kind": "exact", "structure": f"Displacement{target['name']}Chloride"}}, "cases": [{"status": "supported", "id": "displacement", "when": {"kind": "parameter_equals", "parameter": "outcome", "value": "displacement"}, "products": {"saltProduct": {"kind": "exact", "structure": f"Displacement{source['name']}Chloride"}, "metalProduct": {"kind": "exact", "structure": f"Displacement{target['name']}Metal"}}, "patterns": {"displacing": f"Patterns.Displacement{source['name']}Metal", "saltSource": f"Patterns.Displacement{target['name']}Chloride"}, "correspondence": correspondence, "rewrite": rewrite, "observation_compatibility": [{"subject_role": "saltProduct", "predicate": "forms", "evidence_subject": "more reactive metal salt", "premise_id": OBSERVATION}], "premise_ids": [PREMISE, STRUCTURE, VALENCE, OBSERVATION]}], "applicability": {"premise_id": PREMISE, "request_relation": "contact", "required_context": "the displacing metal has a higher reviewed metal-displacement activity rank"}, "model_assumptions": {"event": "representative", "sequence": "explanatory", "premise_ids": [PREMISE]}, "premise_ids": [PREMISE, STRUCTURE, VALENCE, OBSERVATION]}


def main() -> None:
    evidence_url = "https://openstax.org/books/chemistry-2e/pages/4-2-classifying-chemical-reactions"
    evidence = [{"id": "evidence.openstax.metal-displacement", "title": "Chemistry 2e", "publisher": "OpenStax", "locator": "Single-Displacement Reactions", "reference": evidence_url, "publication_date": "2019-02-14", "retrieved_on": "2026-07-15", "usage": "Metal activity series displacement outcomes"}]
    make_premise = lambda identifier, statement: {"id": identifier, "statement": statement, "evidence": ["evidence.openstax.metal-displacement"], "review": {"status": "provisional", "reviewers": []}, "rule_version": "1"}
    structure_records, graph_patterns = [], []
    for symbol, data in METALS.items():
        structure_records.extend(structures(symbol, data))
        graph_patterns.extend(patterns(symbol, data))
    states, domains = set(), []
    for symbol, data in METALS.items():
        v, u = data["valence"], data["ion_unpaired"]
        for state in ((v, 0, 0), (0, v, v), (2, v - 2, v - 2), (2, v - 2, u), (0, v, u + 2)):
            states.add((symbol, *state, 0))
        domains.append({"element": symbol, "site_formal_charge": v, "site_local_electrons": 0, "delocalized_electrons_per_site": v})
    states.add(("Cl", -1, 8, 0, 0))
    pairs = [(a, b) for a, ad in METALS.items() for b, bd in METALS.items() if ad["rank"] > bd["rank"]]
    document = {"schema_version": 1, "id": "metal-displacement", "evidence": evidence, "premises": [make_premise(PREMISE, "Within the reviewed Mg > Zn > Fe > Cu activity series, a higher-ranked metal displaces a lower-ranked divalent metal from its chloride."), make_premise(STRUCTURE, "Divalent chlorides use one 2+ metal component and two chloride components; elemental metals use explicit delocalised metallic domains."), make_premise(VALENCE, "The listed states form the closed electron domain for the reviewed metal-displacement rewrites."), make_premise(OBSERVATION, "Formation of the higher-ranked metal chloride is compatible with the theoretical displacement outcome.")], "valence_premises": [{"premise_id": VALENCE, "neutral_valence": [{"element": symbol, "neutral_valence_electrons": data["valence"]} for symbol, data in METALS.items()] + [{"element": "Cl", "neutral_valence_electrons": 7}], "supported_states": [{"element": symbol, "formal_charge": charge, "non_bonding_electrons": nonbonding, "unpaired_electrons": unpaired, "covalent_bond_order_sum": bonds} for symbol, charge, nonbonding, unpaired, bonds in sorted(states)], "metallic_domain_states": domains}], "structures": structure_records, "rules": [], "elements": [], "element_categories": [], "structural_traits": [], "structure_templates": [], "structure_applications": [], "graph_patterns": graph_patterns, "generalized_rules": [rule(a, b) for a, b in pairs]}
    PACKAGE.mkdir(parents=True, exist_ok=True)
    (PACKAGE / "candidate.json").write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    registry_path = ROOT / "catalogue/experience-registry.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    registry["experiences"] = [item for item in registry["experiences"] if not item["id"].startswith("metal-displacement-")]
    for a, b in pairs:
        source, target = METALS[a], METALS[b]
        slug = f"metal-displacement-{a.lower()}-{b.lower()}"
        source_path = f"conformance/end-to-end/{slug}-001.chems"
        evidence_path = f"conformance/observations/{slug}-001.evidence.json"
        chems = f"""chems 1
use catalog ChemSpec.Theoretical@1
reaction {source['name']}Displaces{target['name']} where
  reactants
    displacing := 1 of Displacement{source['name']}Metal
    saltSource := 1 of Displacement{target['name']}Chloride
  products
    saltProduct := 1 of Displacement{source['name']}Chloride
    metalProduct := 1 of Displacement{target['name']}Metal
  equation
    1 {a}[metallic] + 1 {b}Cl2[ionic]
    -> 1 {a}Cl2[ionic] + 1 {b}[metallic]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.{source['name']}Displaces{target['name']}@1
    product saltProduct forms claim R1
  by
    apply Rules.{source['name']}Displaces{target['name']}
      displacing := displacing
      saltSource := saltSource
      saltProduct := saltProduct
      metalProduct := metalProduct
"""
        packet = {"schema_version": 1, "id": f"Evidence.{source['name']}Displaces{target['name']}@1", "claims": [{"id": "R1", "subject_role": "product", "subject": "more reactive metal salt", "predicate": "forms", "sources": ["S1"]}], "sources": [{"id": "S1", "title": "Single-displacement reactions", "publisher": "OpenStax", "url": evidence_url, "supports": ["R1"]}]}
        (ROOT / source_path).write_text(chems, encoding="utf-8", newline="\n")
        (ROOT / evidence_path).write_text(json.dumps(packet, indent=2) + "\n", encoding="utf-8")
        registry["experiences"].append({"id": slug, "status": "trusted", "family": "metal_displacement", "participants": [{"kind": "element", "atomic_number": source["number"]}, {"kind": "composition", "formula": f"{b}Cl2"}], "source_path": source_path, "evidence_path": evidence_path, "request": f"Can {source['name'].lower()} displace {target['name'].lower()} from its chloride?", "equation": f"{a} + {b}Cl2 -> {a}Cl2 + {b}", "subject_name": source["name"].lower(), "product_name": f"{source['name'].lower()} chloride and {target['name'].lower()}", "product_structure": f"Displacement{source['name']}Chloride"})
        if a == "Mg" and b == "Cu":
            (PACKAGE / "example.chems").write_text(chems, encoding="utf-8", newline="\n")
            (PACKAGE / "evidence.json").write_text(json.dumps(packet, indent=2) + "\n", encoding="utf-8")
    registry_path.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
