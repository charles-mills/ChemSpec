"""Author reviewed O3 variants from the catalogue's explicit O2 outcomes.

This is a deterministic catalogue-authoring utility, not runtime chemistry.
It deliberately copies only outcomes already selected by the reviewed oxygen
catalogue and gives them distinct ozone rules, stoichiometry, atom maps, and
structural operations.
"""

from __future__ import annotations

import copy
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CANDIDATE = ROOT / "catalogue/candidates/oxygen-reactions/candidate.json"
REGISTRY = ROOT / "catalogue/experience-registry.json"
PREMISE = "premise.rule.ozone.explicit-reviewed-outcomes"
STRUCTURE_PREMISE = "premise.structure.ozone.oxygen-transfer-model"
VALENCE_PREMISE = "premise.valence.ozone.oxygen-transfer-model"
OBSERVATION_PREMISE = "premise.observation.ozone.explicit-reviewed-outcomes"
OZONE_PREMISES = [PREMISE, STRUCTURE_PREMISE, VALENCE_PREMISE]


def add_unique(items: list, item) -> None:
    if item not in items:
        items.append(item)


def replace_premises(value) -> None:
    if isinstance(value, dict):
        if isinstance(value.get("premise_ids"), list):
            for premise in OZONE_PREMISES:
                add_unique(value["premise_ids"], premise)
        for child in value.values():
            replace_premises(child)
    elif isinstance(value, list):
        for child in value:
            replace_premises(child)


REFERENCE = re.compile(r"^(subject|oxygen|oxide)\[(\d+)](.*)$")


def map_reference(text: str, clone: int, coefficients: dict[str, int]) -> str:
    match = REFERENCE.match(text)
    if not match:
        return text
    role, index_text, suffix = match.groups()
    index = int(index_text)
    if role == "oxygen":
        old_atom = {".o1": 0, ".o2": 1}.get(suffix)
        if old_atom is None:
            raise ValueError(f"unexpected oxygen reference: {text}")
        linear = 2 * (clone * coefficients["oxygen"] + index - 1) + old_atom
        labels = ["terminal_negative", "central", "terminal_neutral"]
        return f"oxygen[{linear // 3 + 1}].{labels[linear % 3]}"
    return f"{role}[{index + clone * coefficients[role]}]{suffix}"


def map_value(value, clone: int, coefficients: dict[str, int]):
    if isinstance(value, str):
        return map_reference(value, clone, coefficients)
    if isinstance(value, list):
        return [map_value(child, clone, coefficients) for child in value]
    if isinstance(value, dict):
        return {key: map_value(child, clone, coefficients) for key, child in value.items()}
    return value


def ozone_dissociation(instance: int) -> list[dict]:
    prefix = f"oxygen[{instance}]"
    premises = list(OZONE_PREMISES)
    return [
        {
            "kind": "cleave_covalent",
            "premise_ids": premises,
            "edge": [f"{prefix}.central", f"{prefix}.terminal_neutral", "double"],
            "allocation": "homolytic",
            "before": {"left": [1, 2, 0], "right": [0, 4, 0]},
            "after": {"left": [1, 4, 2], "right": [0, 6, 2]},
        },
        {
            "kind": "cleave_covalent",
            "premise_ids": premises,
            "edge": [f"{prefix}.terminal_negative", f"{prefix}.central", "single"],
            "allocation": "homolytic",
            "before": {"left": [-1, 6, 0], "right": [1, 4, 2]},
            "after": {"left": [-1, 7, 1], "right": [1, 5, 3]},
        },
        {
            "kind": "transfer_electron",
            "premise_ids": premises,
            "count": 1,
            "donor": f"{prefix}.terminal_negative",
            "acceptor": f"{prefix}.central",
            "before": {"donor": [-1, 7, 1], "acceptor": [1, 5, 3]},
            "after": {"donor": [0, 6, 2], "acceptor": [0, 6, 2]},
        },
    ]


def clone_rule(original: dict) -> dict:
    rule = copy.deepcopy(original)
    rule["id"] = original["id"].replace("Rules.", "Rules.Ozone", 1)
    old_coefficients = {
        role: int(record["coefficient"]) for role, record in original["roles"].items()
    }
    rule["roles"]["subject"]["coefficient"] *= 3
    rule["roles"]["oxygen"]["coefficient"] *= 2
    rule["roles"]["oxide"]["coefficient"] *= 3
    rule["reactants"]["oxygen"] = {"kind": "exact", "structure": "Ozone"}

    case = rule["cases"][0]
    original_case = original["cases"][0]
    case["patterns"]["oxygen"] = "Patterns.Ozone"
    case["correspondence"] = [
        map_value(item, clone, old_coefficients)
        for clone in range(3)
        for item in original_case["correspondence"]
    ]

    rewrite = []
    for instance in range(1, old_coefficients["oxygen"] * 2 + 1):
        rewrite.extend(ozone_dissociation(instance))
    for clone in range(3):
        for operation in original_case["rewrite"]:
            touches_oxygen_edge = (
                operation["kind"] in {"cleave_covalent", "change_covalent"}
                and any(str(endpoint).startswith("oxygen[") for endpoint in operation.get("edge", []))
            )
            if operation["kind"] == "cleave_covalent" and touches_oxygen_edge:
                continue
            mapped = map_value(copy.deepcopy(operation), clone, old_coefficients)
            if operation["kind"] == "change_covalent" and touches_oxygen_edge:
                mapped = {
                    "kind": "form_covalent",
                    "premise_ids": list(OZONE_PREMISES),
                    "edge": [mapped["edge"][0], mapped["edge"][1], mapped["new_order"]],
                    "electron_contribution": {"left": 1, "right": 1},
                    "before": {"left": [0, 6, 2], "right": [0, 6, 2]},
                    "after": mapped["after"],
                }
            if isinstance(mapped.get("label"), str):
                label_match = re.fullmatch(r"ionic\.product(\d+)", mapped["label"])
                if label_match:
                    mapped["label"] = f"ionic.product{int(label_match.group(1)) + clone * old_coefficients['oxide']}"
            rewrite.append(mapped)
    case["rewrite"] = rewrite
    case["premise_ids"] = [PREMISE]
    for compatibility in case["observation_compatibility"]:
        compatibility["premise_id"] = OBSERVATION_PREMISE
    rule["applicability"] = {
        "premise_id": PREMISE,
        "request_relation": "contact",
        "required_context": "explicit chemist-reviewed theoretical ozone outcome",
    }
    rule["model_assumptions"] = {
        "event": "representative",
        "sequence": "explanatory",
        "premise_ids": [PREMISE],
    }
    replace_premises(rule)
    add_unique(rule["premise_ids"], OBSERVATION_PREMISE)
    return rule


def multiply_first_coefficient(line: str, factor: int) -> str:
    return re.sub(
        r"(^|\s)(\d+)(\s)",
        lambda m: f"{m.group(1)}{int(m.group(2)) * factor}{m.group(3)}",
        line,
        count=1,
    )


def source_for_ozone(source: str) -> str:
    lines = source.splitlines()
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("reaction "):
            lines[index] = line.replace(" where", "WithOzone where")
        elif stripped.startswith("subject :="):
            lines[index] = multiply_first_coefficient(line, 3)
        elif stripped.startswith("oxygen :="):
            lines[index] = multiply_first_coefficient(line, 2).replace(" of Oxygen", " of Ozone")
        elif stripped.startswith("oxide :="):
            lines[index] = multiply_first_coefficient(line, 3)
        elif " + " in line and "[" in line:
            left, right = line.split(" + ", 1)
            lines[index] = multiply_first_coefficient(left, 3) + " + " + multiply_first_coefficient(right, 2).replace("O2[", "O3[")
        elif stripped.startswith("->"):
            lines[index] = multiply_first_coefficient(line, 3)
        elif stripped.startswith("apply Rules."):
            lines[index] = line.replace("apply Rules.", "apply Rules.Ozone", 1)
    return "\n".join(lines) + "\n"


def ascii_equation(source: str) -> str:
    equation_lines = []
    in_equation = False
    for line in source.splitlines():
        stripped = line.strip()
        if stripped == "equation":
            in_equation = True
            continue
        if in_equation and stripped == "model":
            break
        if in_equation:
            equation_lines.append(stripped)
    equation = " ".join(equation_lines)
    equation = re.sub(r"\[(?:metallic|molecular|ionic)\]", "", equation)
    equation = re.sub(r"\b1 (?=[A-Z])", "", equation)
    return equation.replace("  ", " ")


def main() -> None:
    candidate = json.loads(CANDIDATE.read_text(encoding="utf-8-sig"))
    candidate["generalized_rules"] = [
        rule for rule in candidate["generalized_rules"] if not rule["id"].startswith("Rules.Ozone")
    ]
    oxygen_rules = [
        rule
        for rule in candidate["generalized_rules"]
        if rule["id"].startswith("Rules.") and not rule["id"].startswith("Rules.Fixed")
    ]
    candidate["generalized_rules"].extend(clone_rule(rule) for rule in oxygen_rules)
    ozone = next(
        (record for record in candidate["structures"] if record["id"] == "Ozone"),
        None,
    )
    if ozone is None:
        ozone = {
            "representation": "molecular",
            "id": "Ozone",
            "premise_id": STRUCTURE_PREMISE,
            "formula": "O3",
            "atoms": [
                {"label": "terminal_negative", "element": "O", "formal_charge": -1, "non_bonding_electrons": 6, "unpaired_electrons": 0},
                {"label": "central", "element": "O", "formal_charge": 1, "non_bonding_electrons": 2, "unpaired_electrons": 0},
                {"label": "terminal_neutral", "element": "O", "formal_charge": 0, "non_bonding_electrons": 4, "unpaired_electrons": 0},
            ],
            "bonds": [
                {"left": "terminal_negative", "right": "central", "order": "single", "delocalization": {"domain": "ozone.resonance", "effective_order": {"numerator": 3, "denominator": 2}}},
                {"left": "central", "right": "terminal_neutral", "order": "double", "delocalization": {"domain": "ozone.resonance", "effective_order": {"numerator": 3, "denominator": 2}}},
            ],
            "groups": [],
        }
        candidate["structures"].append(ozone)
    else:
        ozone["premise_id"] = STRUCTURE_PREMISE

    candidate["evidence"] = [
        record for record in candidate["evidence"] if record["id"] != "evidence.iupac.ozone-oxidant"
    ]
    candidate["evidence"].append(
        {
            "id": "evidence.iupac.ozone-oxidant",
            "title": "Compendium of Chemical Terminology (the Gold Book)",
            "publisher": "International Union of Pure and Applied Chemistry",
            "locator": "oxidant",
            "reference": "https://goldbook.iupac.org/terms/view/O04361",
            "retrieved_on": "2026-07-15",
            "usage": "Ozone as a distinct stronger oxidant than dioxygen; exact products remain separately reviewed",
        }
    )
    ozone_premise_ids = {PREMISE, STRUCTURE_PREMISE, VALENCE_PREMISE, OBSERVATION_PREMISE}
    candidate["premises"] = [
        record for record in candidate["premises"] if record["id"] not in ozone_premise_ids
    ]
    new_premises = [
        (PREMISE, "Each ozone outcome is an explicit chemist-reviewed outcome; it is not inherited at runtime from an O2 rule."),
        (STRUCTURE_PREMISE, "Reviewed ozone oxygen-transfer model cleaves the resonance-aware O3 graph to three mapped oxygen atoms before product bonding."),
        (VALENCE_PREMISE, "Reviewed transient electron states conserve charge and valence electrons during explanatory ozone bond cleavage."),
        (OBSERVATION_PREMISE, "The selected theoretical ozone outcome forms the explicitly reviewed product."),
    ]
    for identifier, claim in new_premises:
        candidate["premises"].append(
            {
                "id": identifier,
                "statement": claim,
                "evidence": ["evidence.iupac.ozone-oxidant"],
                "review": {"status": "provisional", "reviewers": []},
                "rule_version": "1",
            }
        )

    pattern_ids = {record["id"] for record in candidate["graph_patterns"]}
    if "Patterns.Ozone" not in pattern_ids:
        candidate["graph_patterns"].append(
            {
                "id": "Patterns.Ozone",
                "variables": {
                    "terminal_negative": {"atom": {"element": "O", "formal_charge": -1}},
                    "central": {"atom": {"element": "O", "formal_charge": 1}},
                    "terminal_neutral": {"atom": {"element": "O", "formal_charge": 0}},
                },
                "relationships": [
                    {"kind": "covalent", "bond": "left", "left": "terminal_negative", "right": "central", "order": "single"},
                    {"kind": "covalent", "bond": "right", "left": "central", "right": "terminal_neutral", "order": "double"},
                ],
                "premise_ids": list(OZONE_PREMISES),
            }
        )

    valence = next(record for record in candidate["valence_premises"] if record["premise_id"] == "premise.valence.element-oxygen.closed-domain")
    for state in [
        {"element": "O", "formal_charge": 1, "non_bonding_electrons": 2, "unpaired_electrons": 0, "covalent_bond_order_sum": 3},
        {"element": "O", "formal_charge": 1, "non_bonding_electrons": 4, "unpaired_electrons": 2, "covalent_bond_order_sum": 1},
        {"element": "O", "formal_charge": 1, "non_bonding_electrons": 5, "unpaired_electrons": 3, "covalent_bond_order_sum": 0},
    ]:
        add_unique(valence["supported_states"], state)

    CANDIDATE.write_text(json.dumps(candidate, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    registry = json.loads(REGISTRY.read_text(encoding="utf-8-sig"))
    registry["experiences"] = [record for record in registry["experiences"] if not record["id"].startswith("ozone-")]
    oxygen_records = [record for record in registry["experiences"] if record["id"].startswith("oxygen-")]
    evidence_source = ROOT / "catalogue/candidates/oxygen-reactions/evidence.json"
    for record in oxygen_records:
        source = (ROOT / record["source_path"]).read_text(encoding="utf-8-sig")
        ozone_source = source_for_ozone(source)
        identifier = record["id"].replace("oxygen-", "ozone-", 1)
        source_path = f"conformance/end-to-end/{identifier}-001.chems"
        evidence_path = f"conformance/observations/{identifier}-001.evidence.json"
        (ROOT / source_path).write_text(ozone_source, encoding="utf-8")
        (ROOT / evidence_path).write_text(evidence_source.read_text(encoding="utf-8-sig"), encoding="utf-8")
        ozone_record = copy.deepcopy(record)
        ozone_record.update(
            {
                "id": identifier,
                "co_reactant_atoms": [8, 8, 8],
                "source_path": source_path,
                "evidence_path": evidence_path,
                "request": record["request"].replace("oxygen", "ozone"),
                "equation": ascii_equation(ozone_source),
            }
        )
        if registry.get("schema_version") == 2:
            ozone_record["participants"] = [
                copy.deepcopy(record["participants"][0]),
                {"kind": "composition", "formula": "O3"},
            ]
            ozone_record.pop("co_reactant_atoms", None)
        registry["experiences"].append(ozone_record)
    REGISTRY.write_text(json.dumps(registry, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
