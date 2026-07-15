"""Generate structural complete-combustion rules for catalogue alkanes.

One family algorithm derives formulae, integer stoichiometry, atom mapping and
bond operations.  The output remains ordinary reviewed catalogue rules and
ordinary .chems experiences; the application contains no fuel-specific logic.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "catalogue/candidates/combustion"
PREMISE = "premise.rule.hydrocarbon-combustion.complete"
STRUCTURE = "premise.structure.unbranched-alkane-series"
VALENCE = "premise.valence.hydrocarbon-combustion"
OBSERVATION = "premise.observation.combustion-products-form"
DEPENDENCIES = [
    "premise.rule.element-oxygen.representative-outcomes",
    "premise.structure.element-oxygen.structural-models",
    "premise.valence.element-oxygen.closed-domain",
    "premise.structure.water",
    "premise.structure.carbon-dioxide",
]
BOUND = [PREMISE, STRUCTURE, VALENCE, *DEPENDENCIES]
NAMES = [
    "Methane", "Ethane", "Propane", "Butane", "Pentane",
    "Hexane", "Heptane", "Octane", "Nonane", "Decane",
]


@dataclass(frozen=True)
class Alkane:
    carbons: int
    name: str

    @property
    def hydrogens(self) -> int:
        return 2 * self.carbons + 2

    @property
    def formula(self) -> str:
        return ("C" if self.carbons == 1 else f"C{self.carbons}") + f"H{self.hydrogens}"

    @property
    def fuel_coefficient(self) -> int:
        return 1 if self.carbons % 2 else 2

    @property
    def oxygen_coefficient(self) -> int:
        return self.fuel_coefficient * (3 * self.carbons + 1) // 2

    @property
    def carbon_dioxide_coefficient(self) -> int:
        return self.fuel_coefficient * self.carbons

    @property
    def water_coefficient(self) -> int:
        return self.fuel_coefficient * (self.carbons + 1)


ALKANES = [Alkane(index, name) for index, name in enumerate(NAMES, 1)]


def operation(kind: str, **values) -> dict:
    return {"kind": kind, "premise_ids": BOUND, **values}


def cleavage(left: str, right: str, order: str, left_before: list[int],
             right_before: list[int], left_after: list[int],
             right_after: list[int]) -> dict:
    return operation(
        "cleave_covalent", edge=[left, right, order], allocation="homolytic",
        before={"left": left_before, "right": right_before},
        after={"left": left_after, "right": right_after},
    )


def formation(left: str, right: str, order: str, left_before: list[int],
              right_before: list[int], left_after: list[int],
              right_after: list[int]) -> dict:
    contribution = 1 if order == "single" else 2
    return operation(
        "form_covalent", edge=[left, right, order],
        electron_contribution={"left": contribution, "right": contribution},
        before={"left": left_before, "right": right_before},
        after={"left": left_after, "right": right_after},
    )


def alkane_graph(alkane: Alkane) -> tuple[dict, dict, list[tuple[str, str]]]:
    atoms = [
        {"label": f"c{i}", "element": "C", "formal_charge": 0,
         "non_bonding_electrons": 0, "unpaired_electrons": 0}
        for i in range(1, alkane.carbons + 1)
    ]
    bonds: list[dict] = []
    attachments: list[tuple[str, str]] = []
    for index in range(1, alkane.carbons):
        bonds.append({"left": f"c{index}", "right": f"c{index + 1}", "order": "single"})
    hydrogen = 1
    for carbon in range(1, alkane.carbons + 1):
        count = 4 if alkane.carbons == 1 else (3 if carbon in (1, alkane.carbons) else 2)
        for _ in range(count):
            label = f"h{hydrogen}"
            atoms.append({"label": label, "element": "H", "formal_charge": 0,
                          "non_bonding_electrons": 0, "unpaired_electrons": 0})
            bonds.append({"left": f"c{carbon}", "right": label, "order": "single"})
            attachments.append((f"c{carbon}", label))
            hydrogen += 1
    structure = {
        "representation": "molecular", "id": alkane.name,
        "premise_id": STRUCTURE, "formula": alkane.formula,
        "atoms": atoms, "bonds": bonds, "groups": [],
        "traits": [{"trait": "Traits.CombustibleFuel", "sites": {"reactive_site": "c1"}, "premise_ids": [PREMISE]}],
    }
    variables = {atom["label"]: {"atom": {"element": atom["element"]}} for atom in atoms}
    relationships = [
        {"kind": "covalent", "bond": f"bond{index}", "left": bond["left"],
         "right": bond["right"], "order": "single"}
        for index, bond in enumerate(bonds, 1)
    ]
    pattern = {
        "id": f"Patterns.{alkane.name}", "variables": variables,
        "relationships": relationships, "premise_ids": BOUND,
    }
    return structure, pattern, [(bond["left"], bond["right"]) for bond in bonds]


def alkane_rule(alkane: Alkane, bonds: list[tuple[str, str]]) -> dict:
    rewrite: list[dict] = []
    correspondence: list[dict] = []
    carbon_refs: list[str] = []
    hydrogen_refs: list[str] = []

    # Atom-local counters make the electron ledger independent of fuel size.
    for fuel_index in range(1, alkane.fuel_coefficient + 1):
        cleaved: dict[str, int] = {}
        for left_label, right_label in bonds:
            left = f"fuel[{fuel_index}].{left_label}"
            right = f"fuel[{fuel_index}].{right_label}"
            left_count = cleaved.get(left_label, 0)
            right_count = cleaved.get(right_label, 0)
            rewrite.append(cleavage(
                left, right, "single",
                [0, left_count, left_count], [0, right_count, right_count],
                [0, left_count + 1, left_count + 1], [0, right_count + 1, right_count + 1],
            ))
            cleaved[left_label] = left_count + 1
            cleaved[right_label] = right_count + 1
        carbon_refs.extend(f"fuel[{fuel_index}].c{i}" for i in range(1, alkane.carbons + 1))
        hydrogen_refs.extend(f"fuel[{fuel_index}].h{i}" for i in range(1, alkane.hydrogens + 1))

    oxygen_refs: list[str] = []
    for oxygen_index in range(1, alkane.oxygen_coefficient + 1):
        left, right = f"oxygen[{oxygen_index}].o1", f"oxygen[{oxygen_index}].o2"
        rewrite.append(cleavage(left, right, "double", [0, 4, 0], [0, 4, 0], [0, 6, 2], [0, 6, 2]))
        oxygen_refs.extend((left, right))

    oxygen_cursor = 0
    for product_index, carbon in enumerate(carbon_refs, 1):
        first, second = oxygen_refs[oxygen_cursor:oxygen_cursor + 2]
        oxygen_cursor += 2
        rewrite.extend((
            formation(carbon, first, "double", [0, 4, 4], [0, 6, 2], [0, 2, 2], [0, 4, 0]),
            formation(carbon, second, "double", [0, 2, 2], [0, 6, 2], [0, 0, 0], [0, 4, 0]),
            operation("assign_product", atoms=[carbon, first, second], product=f"carbonDioxide[{product_index}]"),
        ))
        correspondence.extend((
            {"reactant": carbon, "product": f"carbonDioxide[{product_index}].c", "premise_ids": BOUND},
            {"reactant": first, "product": f"carbonDioxide[{product_index}].o_a", "premise_ids": BOUND},
            {"reactant": second, "product": f"carbonDioxide[{product_index}].o_c", "premise_ids": BOUND},
        ))

    for product_index in range(1, alkane.water_coefficient + 1):
        oxygen = oxygen_refs[oxygen_cursor]
        oxygen_cursor += 1
        h1, h2 = hydrogen_refs[(product_index - 1) * 2:product_index * 2]
        rewrite.extend((
            formation(oxygen, h1, "single", [0, 6, 2], [0, 1, 1], [0, 5, 1], [0, 0, 0]),
            formation(oxygen, h2, "single", [0, 5, 1], [0, 1, 1], [0, 4, 0], [0, 0, 0]),
            operation("assign_product", atoms=[oxygen, h1, h2], product=f"water[{product_index}]"),
        ))
        correspondence.extend((
            {"reactant": oxygen, "product": f"water[{product_index}].o", "premise_ids": BOUND},
            {"reactant": h1, "product": f"water[{product_index}].h1", "premise_ids": BOUND},
            {"reactant": h2, "product": f"water[{product_index}].h2", "premise_ids": BOUND},
        ))
    assert oxygen_cursor == len(oxygen_refs)

    return {
        "id": f"Rules.{alkane.name}CompleteCombustion",
        "parameters": {"outcome": {"kind": "enum", "values": ["complete"]}},
        "roles": {
            "fuel": {"side": "reactant", "representation": "molecular", "coefficient": alkane.fuel_coefficient},
            "oxygen": {"side": "reactant", "representation": "molecular", "coefficient": alkane.oxygen_coefficient},
            "carbonDioxide": {"side": "product", "representation": "molecular", "coefficient": alkane.carbon_dioxide_coefficient},
            "water": {"side": "product", "representation": "molecular", "coefficient": alkane.water_coefficient},
        },
        "reactants": {"fuel": {"kind": "exact", "structure": alkane.name}, "oxygen": {"kind": "exact", "structure": "Oxygen"}},
        "cases": [{
            "status": "supported", "id": "complete",
            "when": {"kind": "parameter_equals", "parameter": "outcome", "value": "complete"},
            "products": {"carbonDioxide": {"kind": "exact", "structure": "CarbonDioxide"}, "water": {"kind": "exact", "structure": "Water"}},
            "patterns": {"fuel": f"Patterns.{alkane.name}", "oxygen": "Patterns.Oxygen"},
            "correspondence": correspondence, "rewrite": rewrite,
            "observation_compatibility": [{"subject_role": "carbonDioxide", "predicate": "forms", "evidence_subject": "carbon dioxide", "premise_id": OBSERVATION}],
            "premise_ids": [PREMISE, STRUCTURE, VALENCE, OBSERVATION, *DEPENDENCIES],
        }],
        "applicability": {"premise_id": PREMISE, "request_relation": "contact", "required_context": "the reviewed complete-combustion outcome; no temperature, pressure, quantity, or macroscopic-effect model"},
        "model_assumptions": {"event": "representative", "sequence": "explanatory", "premise_ids": [PREMISE]},
        "premise_ids": [PREMISE, STRUCTURE, VALENCE, OBSERVATION, *DEPENDENCIES],
    }


def coefficient(value: int) -> str:
    return "" if value == 1 else str(value)


def experience(alkane: Alkane, evidence_url: str) -> tuple[str, str, dict]:
    slug = f"combustion-{alkane.name.lower()}-complete"
    source_path = f"conformance/end-to-end/{slug}-001.chems"
    evidence_path = f"conformance/observations/{slug}-001.evidence.json"
    source = f"""chems 1
use catalog ChemSpec.Theoretical@1
reaction {alkane.name}CompleteCombustion where
  reactants
    fuel := {alkane.fuel_coefficient} of {alkane.name}
    oxygen := {alkane.oxygen_coefficient} of Oxygen
  products
    carbonDioxide := {alkane.carbon_dioxide_coefficient} of CarbonDioxide
    water := {alkane.water_coefficient} of Water
  equation
    {alkane.fuel_coefficient} {alkane.formula}[molecular] + {alkane.oxygen_coefficient} O2[molecular]
    -> {alkane.carbon_dioxide_coefficient} CO2[molecular] + {alkane.water_coefficient} H2O[molecular]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.{alkane.name}Combustion@1
    product carbonDioxide forms claim R1
  by
    apply Rules.{alkane.name}CompleteCombustion
      fuel := fuel
      oxygen := oxygen
      carbonDioxide := carbonDioxide
      water := water
"""
    evidence = {"schema_version": 1, "id": f"Evidence.{alkane.name}Combustion@1", "claims": [{"id": "R1", "subject_role": "product", "subject": "carbon dioxide", "predicate": "forms", "sources": ["S1"]}], "sources": [{"id": "S1", "title": "Combustion reactions", "publisher": "OpenStax", "url": evidence_url, "supports": ["R1"]}]}
    (ROOT / source_path).write_text(source, encoding="utf-8", newline="\n")
    (ROOT / evidence_path).write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    equation = f"{coefficient(alkane.fuel_coefficient)}{alkane.formula} + {coefficient(alkane.oxygen_coefficient)}O2 -> {coefficient(alkane.carbon_dioxide_coefficient)}CO2 + {coefficient(alkane.water_coefficient)}H2O"
    registry = {"id": slug, "status": "trusted", "family": "combustion", "participants": [{"kind": "composition", "formula": alkane.formula}, {"kind": "element", "atomic_number": 8}], "source_path": source_path, "evidence_path": evidence_path, "request": f"What happens when {alkane.name.lower()} undergoes complete combustion?", "equation": equation, "subject_name": alkane.name.lower(), "product_name": "carbon dioxide and water", "product_structure": "CarbonDioxide"}
    return source, json.dumps(evidence, indent=2) + "\n", registry


def main() -> None:
    evidence_url = "https://openstax.org/books/chemistry-2e/pages/4-2-classifying-chemical-reactions"
    evidence = [{"id": "evidence.openstax.combustion", "title": "Chemistry 2e", "publisher": "OpenStax", "locator": "Combustion Reactions", "reference": evidence_url, "publication_date": "2019-02-14", "retrieved_on": "2026-07-15", "usage": "Complete hydrocarbon combustion products and balancing"}]
    make_premise = lambda identifier, statement: {"id": identifier, "statement": statement, "evidence": ["evidence.openstax.combustion"], "review": {"status": "provisional", "reviewers": []}, "rule_version": "1"}
    structures, patterns, rules = [], [], []
    for alkane in ALKANES:
        structure, pattern, bonds = alkane_graph(alkane)
        structures.append(structure)
        patterns.append(pattern)
        rules.append(alkane_rule(alkane, bonds))
    states = [{"element": "C", "formal_charge": 0, "non_bonding_electrons": 4 - degree, "unpaired_electrons": 4 - degree, "covalent_bond_order_sum": degree} for degree in range(5)]
    states.extend((
        {"element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 1},
        {"element": "H", "formal_charge": 0, "non_bonding_electrons": 1, "unpaired_electrons": 1, "covalent_bond_order_sum": 0},
        {"element": "O", "formal_charge": 0, "non_bonding_electrons": 4, "unpaired_electrons": 0, "covalent_bond_order_sum": 2},
        {"element": "O", "formal_charge": 0, "non_bonding_electrons": 5, "unpaired_electrons": 1, "covalent_bond_order_sum": 1},
        {"element": "O", "formal_charge": 0, "non_bonding_electrons": 6, "unpaired_electrons": 2, "covalent_bond_order_sum": 0},
    ))
    document = {"schema_version": 1, "id": "combustion", "evidence": evidence, "premises": [
        make_premise(PREMISE, "Every reviewed unbranched alkane CnH(2n+2) has a generated atom-conserving complete-combustion graph rewrite to CO2 and H2O."),
        make_premise(STRUCTURE, "The reviewed unbranched alkane series uses explicit C-C and C-H single covalent bonds."),
        make_premise(VALENCE, "The listed C, H and O states are the closed explanatory domain for hydrocarbon combustion rewrites."),
        make_premise(OBSERVATION, "Formation of carbon dioxide is compatible with these theoretical complete-combustion outcomes."),
    ], "valence_premises": [{"premise_id": VALENCE, "neutral_valence": [{"element": "C", "neutral_valence_electrons": 4}, {"element": "H", "neutral_valence_electrons": 1}, {"element": "O", "neutral_valence_electrons": 6}], "supported_states": states, "metallic_domain_states": []}], "structures": structures, "rules": [], "elements": [], "element_categories": [], "structural_traits": [], "structure_templates": [], "structure_applications": [], "graph_patterns": patterns, "generalized_rules": rules}
    PACKAGE.mkdir(parents=True, exist_ok=True)
    (PACKAGE / "candidate.json").write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    registry_path = ROOT / "catalogue/experience-registry.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    registry["experiences"] = [item for item in registry["experiences"] if not item["id"].startswith("combustion-")]
    for alkane in ALKANES:
        source, packet, entry = experience(alkane, evidence_url)
        registry["experiences"].append(entry)
        if alkane.carbons == 1:
            (PACKAGE / "example.chems").write_text(source, encoding="utf-8", newline="\n")
            (PACKAGE / "evidence.json").write_text(packet, encoding="utf-8", newline="\n")
    registry_path.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
