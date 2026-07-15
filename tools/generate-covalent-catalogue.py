#!/usr/bin/env python3
"""Generate the bounded covalent-combination catalogue and app registry records.

The generated reactions are finite reviewed bindings over reusable hydrogen-
compound and interhalogen rules.  The runtime never derives a product formula
from periodic-table position or valence alone.
"""

from __future__ import annotations

import argparse
import json
import shutil
from collections import OrderedDict
from pathlib import Path


RULE_PREMISE = "premise.rule.covalent-combinations.reviewed-outcomes"
STRUCTURE_PREMISE = "premise.structure.covalent-combinations.explicit-graphs"
VALENCE_PREMISE = "premise.valence.covalent-combinations.closed-domain"
OBSERVATION_PREMISE = "premise.observation.covalent-combinations"
COVALENT_PREMISES = [RULE_PREMISE, STRUCTURE_PREMISE, VALENCE_PREMISE]

HYDROGEN_STRUCTURE_PREMISE = "premise.structure.hydrogen"
HALOGEN_STRUCTURE_PREMISE = "premise.structure.diatomic-halogen"
HALOGEN_CATEGORY_PREMISE = "premise.category.halide"
HYDROGEN_HALIDE_STRUCTURE_PREMISE = "premise.structure.hydrogen-halide"
ATOMIC_NUMBERS = {"H": 1, "N": 7, "F": 9, "S": 16, "Cl": 17, "Br": 35, "I": 53}
ELEMENT_NAMES = {
    "H": "hydrogen",
    "N": "nitrogen",
    "F": "fluorine",
    "S": "sulfur",
    "Cl": "chlorine",
    "Br": "bromine",
    "I": "iodine",
}
ELEMENTAL_STRUCTURES = {
    "H": "Hydrogen",
    "N": "ElementalNitrogen",
    "F": "Fluorine",
    "S": "ElementalSulfur",
    "Cl": "Chlorine",
    "Br": "Bromine",
    "I": "Iodine",
}

HYDROGEN_HALIDES = [
    ("F", "HydrogenFluoride", "HF", "hydrogen fluoride"),
    ("Cl", "HydrogenChloride", "HCl", "hydrogen chloride"),
    ("Br", "HydrogenBromide", "HBr", "hydrogen bromide"),
    ("I", "HydrogenIodide", "HI", "hydrogen iodide"),
]

INTERHALOGENS = {
    1: [
        ("Cl", "F", "ClF", "chlorine monofluoride"),
        ("Br", "F", "BrF", "bromine monofluoride"),
        ("I", "F", "IF", "iodine monofluoride"),
        ("Br", "Cl", "BrCl", "bromine monochloride"),
        ("I", "Cl", "ICl", "iodine monochloride"),
        ("I", "Br", "IBr", "iodine monobromide"),
    ],
    3: [
        ("Cl", "F", "ClF3", "chlorine trifluoride"),
        ("Br", "F", "BrF3", "bromine trifluoride"),
        ("I", "F", "IF3", "iodine trifluoride"),
        ("I", "Cl", "ICl3", "iodine trichloride"),
    ],
    5: [
        ("Cl", "F", "ClF5", "chlorine pentafluoride"),
        ("Br", "F", "BrF5", "bromine pentafluoride"),
        ("I", "F", "IF5", "iodine pentafluoride"),
    ],
    7: [("I", "F", "IF7", "iodine heptafluoride")],
}


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def atom(label: str, element: object, non_bonding: int, unpaired: int) -> dict:
    return {
        "label": label,
        "element": element,
        "formal_charge": 0,
        "non_bonding_electrons": non_bonding,
        "unpaired_electrons": unpaired,
    }


def bond(left: str, right: str, order: str = "single") -> dict:
    return {"left": left, "right": right, "order": order}


def state_tuple(charge: int, non_bonding: int, unpaired: int) -> list[int]:
    return [charge, non_bonding, unpaired]


def binary_state(left: list[int], right: list[int]) -> dict:
    return {"left": left, "right": right}


def parameter(category: str) -> dict:
    return {"kind": "element", "category": category}


def parameter_value(name: str) -> dict:
    return {"parameter": name}


def exact(structure: str) -> dict:
    return {"kind": "exact", "structure": structure}


def template_ref(template: str, arguments: dict) -> dict:
    return {"kind": "template", "template": template, "arguments": arguments}


def role(side: str, coefficient: int) -> dict:
    return {"side": side, "representation": "molecular", "coefficient": coefficient}


def mapping(reactant: str, product: str, premises: list[str]) -> dict:
    return {"reactant": reactant, "product": product, "premise_ids": premises}


def cleavage(left: str, right: str, order: str, before: dict, after: dict) -> dict:
    return {
        "kind": "cleave_covalent",
        "premise_ids": COVALENT_PREMISES,
        "edge": [left, right, order],
        "allocation": "homolytic",
        "before": before,
        "after": after,
    }


def formation(left: str, right: str, before: dict, after: dict) -> dict:
    return {
        "kind": "form_covalent",
        "premise_ids": COVALENT_PREMISES,
        "edge": [left, right, "single"],
        "electron_contribution": {"left": 1, "right": 1},
        "before": before,
        "after": after,
    }


def assignment(atoms: list[str], product: str) -> dict:
    return {
        "kind": "assign_product",
        "premise_ids": COVALENT_PREMISES,
        "atoms": atoms,
        "product": product,
    }


def observation_compatibility(disappearing_role: str) -> list[dict]:
    return [
        {
            "subject_role": "compound",
            "predicate": "forms",
            "evidence_subject": "product",
            "premise_id": OBSERVATION_PREMISE,
        },
        {
            "subject_role": disappearing_role,
            "predicate": "disappears",
            "evidence_subject": "element",
            "premise_id": OBSERVATION_PREMISE,
        },
    ]


def generalized_rule(
    rule_id: str,
    parameters: dict,
    roles: dict,
    reactants: dict,
    products: dict,
    patterns: dict,
    correspondence: list[dict],
    rewrite: list[dict],
    disappearing_role: str,
    extra_premises: list[str],
    when: dict | None = None,
) -> dict:
    return {
        "id": rule_id,
        "parameters": parameters,
        "roles": roles,
        "reactants": reactants,
        "cases": [
            {
                "status": "supported",
                "id": "reviewed-outcome",
                "when": when or {"kind": "always"},
                "products": products,
                "patterns": patterns,
                "correspondence": correspondence,
                "rewrite": rewrite,
                "observation_compatibility": observation_compatibility(disappearing_role),
                "premise_ids": [RULE_PREMISE, OBSERVATION_PREMISE],
            }
        ],
        "applicability": {
            "premise_id": RULE_PREMISE,
            "request_relation": "contact",
            "required_context": "selected theoretical covalent-combination outcome from the reviewed finite catalogue",
        },
        "model_assumptions": {
            "event": "representative",
            "sequence": "explanatory",
            "premise_ids": [RULE_PREMISE],
        },
        "premise_ids": list(
            dict.fromkeys(
                [
                    "premise.elements.iupac-periodic-table",
                    RULE_PREMISE,
                    STRUCTURE_PREMISE,
                    VALENCE_PREMISE,
                    OBSERVATION_PREMISE,
                    *extra_premises,
                ]
            )
        ),
    }


def covalent_pattern(pattern_id: str, element: object, labels: list[str], edges: list[tuple]) -> dict:
    variables = OrderedDict(
        (label, {"atom": {"element": element, "formal_charge": 0}}) for label in labels
    )
    relationships = [
        {
            "kind": "covalent",
            "bond": f"bond{index}",
            "left": left,
            "right": right,
            "order": order,
        }
        for index, (left, right, order) in enumerate(edges, 1)
    ]
    return {
        "id": pattern_id,
        "variables": variables,
        "relationships": relationships,
        "premise_ids": COVALENT_PREMISES,
    }


def pair_condition(pairs: list[tuple[str, str, str, str]]) -> dict:
    predicates = [
        {
            "kind": "all",
            "predicates": [
                {"kind": "parameter_equals", "parameter": "central", "value": central},
                {"kind": "parameter_equals", "parameter": "ligand", "value": ligand},
            ],
        }
        for central, ligand, _formula, _name in pairs
    ]
    return predicates[0] if len(predicates) == 1 else {"kind": "any", "predicates": predicates}


def add_state(states: dict, element: str, non_bonding: int, unpaired: int, bond_sum: int) -> None:
    key = (element, 0, non_bonding, unpaired, bond_sum)
    states[key] = {
        "element": element,
        "formal_charge": 0,
        "non_bonding_electrons": non_bonding,
        "unpaired_electrons": unpaired,
        "covalent_bond_order_sum": bond_sum,
    }


def hydrogen_pattern() -> dict:
    return covalent_pattern(
        "Patterns.CovalentHydrogen",
        "H",
        ["h1", "h2"],
        [("h1", "h2", "single")],
    )


def hydrogen_halide_rule() -> dict:
    premises = COVALENT_PREMISES + [HYDROGEN_STRUCTURE_PREMISE, HALOGEN_STRUCTURE_PREMISE]
    correspondence = [
        mapping("hydrogen[1].h1", "compound[1].h", premises),
        mapping("hydrogen[1].h2", "compound[2].h", premises),
        mapping("halogen[1].x1", "compound[1].x", premises),
        mapping("halogen[1].x2", "compound[2].x", premises),
    ]
    rewrite = [
        cleavage(
            "hydrogen[1].h1",
            "hydrogen[1].h2",
            "single",
            binary_state(state_tuple(0, 0, 0), state_tuple(0, 0, 0)),
            binary_state(state_tuple(0, 1, 1), state_tuple(0, 1, 1)),
        ),
        cleavage(
            "halogen[1].x1",
            "halogen[1].x2",
            "single",
            binary_state(state_tuple(0, 6, 0), state_tuple(0, 6, 0)),
            binary_state(state_tuple(0, 7, 1), state_tuple(0, 7, 1)),
        ),
    ]
    for index in (1, 2):
        rewrite.append(
            formation(
                f"hydrogen[1].h{index}",
                f"halogen[1].x{index}",
                binary_state(state_tuple(0, 1, 1), state_tuple(0, 7, 1)),
                binary_state(state_tuple(0, 0, 0), state_tuple(0, 6, 0)),
            )
        )
        rewrite.append(
            assignment(
                [f"hydrogen[1].h{index}", f"halogen[1].x{index}"],
                f"compound[{index}]",
            )
        )
    return generalized_rule(
        "Rules.HydrogenHalideCombination",
        {"halide": parameter("Categories.Halide")},
        {"hydrogen": role("reactant", 1), "halogen": role("reactant", 1), "compound": role("product", 2)},
        {
            "hydrogen": exact("Hydrogen"),
            "halogen": template_ref("Templates.DiatomicHalogen", {"halogen": parameter_value("halide")}),
        },
        {"compound": template_ref("Templates.HydrogenHalide", {"halide": parameter_value("halide")})},
        {"hydrogen": "Patterns.CovalentHydrogen", "halogen": "Patterns.CovalentHydrogenHalidePartner"},
        correspondence,
        rewrite,
        "halogen",
        [HYDROGEN_STRUCTURE_PREMISE, HALOGEN_CATEGORY_PREMISE, HALOGEN_STRUCTURE_PREMISE, HYDROGEN_HALIDE_STRUCTURE_PREMISE],
    )


def ammonia_rule() -> tuple[list[dict], list[dict], dict]:
    source_template = {
        "representation": "molecular",
        "id": "Templates.CovalentNitrogenSource",
        "parameters": {"member": parameter("Categories.CovalentNitrogen")},
        "atoms": [atom("a1", parameter_value("member"), 2, 0), atom("a2", parameter_value("member"), 2, 0)],
        "bonds": [bond("a1", "a2", "triple")],
        "groups": [],
        "premise_ids": COVALENT_PREMISES,
    }
    product_template = {
        "representation": "molecular",
        "id": "Templates.Ammonia",
        "parameters": {"member": parameter("Categories.CovalentNitrogen")},
        "atoms": [
            atom("n", parameter_value("member"), 2, 0),
            atom("h1", "H", 0, 0),
            atom("h2", "H", 0, 0),
            atom("h3", "H", 0, 0),
        ],
        "bonds": [bond("n", f"h{index}") for index in range(1, 4)],
        "groups": [],
        "premise_ids": COVALENT_PREMISES,
    }
    applications = [
        {"id": "CovalentNitrogen", "template": "Templates.CovalentNitrogenSource", "arguments": {"member": "N"}, "formula": "N2", "premise_ids": COVALENT_PREMISES},
        {"id": "Ammonia", "template": "Templates.Ammonia", "arguments": {"member": "N"}, "formula": "NH3", "premise_ids": COVALENT_PREMISES},
    ]
    premises = COVALENT_PREMISES + [HYDROGEN_STRUCTURE_PREMISE]
    correspondence = [
        mapping("nitrogen[1].a1", "compound[1].n", premises),
        mapping("nitrogen[1].a2", "compound[2].n", premises),
    ]
    for hydrogen in range(1, 4):
        correspondence.extend(
            [
                mapping(f"hydrogen[{hydrogen}].h1", f"compound[1].h{hydrogen}", premises),
                mapping(f"hydrogen[{hydrogen}].h2", f"compound[2].h{hydrogen}", premises),
            ]
        )
    rewrite = [
        cleavage(
            "nitrogen[1].a1",
            "nitrogen[1].a2",
            "triple",
            binary_state(state_tuple(0, 2, 0), state_tuple(0, 2, 0)),
            binary_state(state_tuple(0, 5, 3), state_tuple(0, 5, 3)),
        )
    ]
    for hydrogen in range(1, 4):
        rewrite.append(
            cleavage(
                f"hydrogen[{hydrogen}].h1",
                f"hydrogen[{hydrogen}].h2",
                "single",
                binary_state(state_tuple(0, 0, 0), state_tuple(0, 0, 0)),
                binary_state(state_tuple(0, 1, 1), state_tuple(0, 1, 1)),
            )
        )
    for product in (1, 2):
        nitrogen = f"nitrogen[1].a{product}"
        for hydrogen in range(1, 4):
            before_nb = 6 - hydrogen
            before_unpaired = 4 - hydrogen
            rewrite.append(
                formation(
                    nitrogen,
                    f"hydrogen[{hydrogen}].h{product}",
                    binary_state(state_tuple(0, before_nb, before_unpaired), state_tuple(0, 1, 1)),
                    binary_state(state_tuple(0, before_nb - 1, before_unpaired - 1), state_tuple(0, 0, 0)),
                )
            )
        rewrite.append(
            assignment(
                [nitrogen, *[f"hydrogen[{hydrogen}].h{product}" for hydrogen in range(1, 4)]],
                f"compound[{product}]",
            )
        )
    rule = generalized_rule(
        "Rules.AmmoniaCombination",
        {"member": parameter("Categories.CovalentNitrogen")},
        {"hydrogen": role("reactant", 3), "nitrogen": role("reactant", 1), "compound": role("product", 2)},
        {"hydrogen": exact("Hydrogen"), "nitrogen": template_ref("Templates.CovalentNitrogenSource", {"member": parameter_value("member")})},
        {"compound": template_ref("Templates.Ammonia", {"member": parameter_value("member")})},
        {"hydrogen": "Patterns.CovalentHydrogen", "nitrogen": "Patterns.CovalentNitrogen"},
        correspondence,
        rewrite,
        "nitrogen",
        [HYDROGEN_STRUCTURE_PREMISE],
    )
    return [source_template, product_template], applications, rule


def hydrogen_sulfide_rule() -> tuple[list[dict], list[dict], dict]:
    source_template = {
        "representation": "molecular",
        "id": "Templates.CovalentSulfurSource",
        "parameters": {"member": parameter("Categories.CovalentSulfur")},
        "atoms": [atom(f"a{index}", parameter_value("member"), 4, 0) for index in range(1, 9)],
        "bonds": [bond(f"a{index}", f"a{index + 1 if index < 8 else 1}") for index in range(1, 9)],
        "groups": [],
        "premise_ids": COVALENT_PREMISES,
    }
    product_template = {
        "representation": "molecular",
        "id": "Templates.HydrogenSulfide",
        "parameters": {"member": parameter("Categories.CovalentSulfur")},
        "atoms": [atom("s", parameter_value("member"), 4, 0), atom("h1", "H", 0, 0), atom("h2", "H", 0, 0)],
        "bonds": [bond("s", "h1"), bond("s", "h2")],
        "groups": [],
        "premise_ids": COVALENT_PREMISES,
    }
    applications = [
        {"id": "CovalentSulfur", "template": "Templates.CovalentSulfurSource", "arguments": {"member": "S"}, "formula": "S8", "premise_ids": COVALENT_PREMISES},
        {"id": "HydrogenSulfide", "template": "Templates.HydrogenSulfide", "arguments": {"member": "S"}, "formula": "H2S", "premise_ids": COVALENT_PREMISES},
    ]
    premises = COVALENT_PREMISES + [HYDROGEN_STRUCTURE_PREMISE]
    correspondence = []
    for product in range(1, 9):
        correspondence.extend(
            [
                mapping(f"sulfur[1].a{product}", f"compound[{product}].s", premises),
                mapping(f"hydrogen[{product}].h1", f"compound[{product}].h1", premises),
                mapping(f"hydrogen[{product}].h2", f"compound[{product}].h2", premises),
            ]
        )
    rewrite = []
    sulfur_states = {index: [4, 0, 2] for index in range(1, 9)}
    sulfur_edges = [(index, index + 1 if index < 8 else 1) for index in range(1, 9)]
    for left, right in sulfur_edges:
        left_state = sulfur_states[left]
        right_state = sulfur_states[right]
        rewrite.append(
            cleavage(
                f"sulfur[1].a{left}",
                f"sulfur[1].a{right}",
                "single",
                binary_state(state_tuple(0, left_state[0], left_state[1]), state_tuple(0, right_state[0], right_state[1])),
                binary_state(state_tuple(0, left_state[0] + 1, left_state[1] + 1), state_tuple(0, right_state[0] + 1, right_state[1] + 1)),
            )
        )
        sulfur_states[left] = [left_state[0] + 1, left_state[1] + 1, left_state[2] - 1]
        sulfur_states[right] = [right_state[0] + 1, right_state[1] + 1, right_state[2] - 1]
    for hydrogen in range(1, 9):
        rewrite.append(
            cleavage(
                f"hydrogen[{hydrogen}].h1",
                f"hydrogen[{hydrogen}].h2",
                "single",
                binary_state(state_tuple(0, 0, 0), state_tuple(0, 0, 0)),
                binary_state(state_tuple(0, 1, 1), state_tuple(0, 1, 1)),
            )
        )
    for product in range(1, 9):
        sulfur = f"sulfur[1].a{product}"
        for hydrogen in (1, 2):
            before_nb = 7 - hydrogen
            before_unpaired = 3 - hydrogen
            rewrite.append(
                formation(
                    sulfur,
                    f"hydrogen[{product}].h{hydrogen}",
                    binary_state(state_tuple(0, before_nb, before_unpaired), state_tuple(0, 1, 1)),
                    binary_state(state_tuple(0, before_nb - 1, before_unpaired - 1), state_tuple(0, 0, 0)),
                )
            )
        rewrite.append(
            assignment(
                [sulfur, f"hydrogen[{product}].h1", f"hydrogen[{product}].h2"],
                f"compound[{product}]",
            )
        )
    rule = generalized_rule(
        "Rules.HydrogenSulfideCombination",
        {"member": parameter("Categories.CovalentSulfur")},
        {"hydrogen": role("reactant", 8), "sulfur": role("reactant", 1), "compound": role("product", 8)},
        {"hydrogen": exact("Hydrogen"), "sulfur": template_ref("Templates.CovalentSulfurSource", {"member": parameter_value("member")})},
        {"compound": template_ref("Templates.HydrogenSulfide", {"member": parameter_value("member")})},
        {"hydrogen": "Patterns.CovalentHydrogen", "sulfur": "Patterns.CovalentSulfur"},
        correspondence,
        rewrite,
        "sulfur",
        [HYDROGEN_STRUCTURE_PREMISE],
    )
    return [source_template, product_template], applications, rule


def interhalogen_template(count: int) -> dict:
    return {
        "representation": "molecular",
        "id": f"Templates.Interhalogen{count}",
        "parameters": {
            "central": parameter("Categories.Halide"),
            "ligand": parameter("Categories.Halide"),
        },
        "atoms": [
            atom("y", parameter_value("central"), 7 - count, 0),
            *[atom(f"x{index}", parameter_value("ligand"), 6, 0) for index in range(1, count + 1)],
        ],
        "bonds": [bond("y", f"x{index}") for index in range(1, count + 1)],
        "groups": [],
        "premise_ids": COVALENT_PREMISES,
    }


def interhalogen_rule(count: int, pairs: list[tuple[str, str, str, str]]) -> dict:
    premises = COVALENT_PREMISES + [HALOGEN_STRUCTURE_PREMISE]
    correspondence = [
        mapping("central[1].x1", "compound[1].y", premises),
        mapping("central[1].x2", "compound[2].y", premises),
    ]
    for ligand in range(1, count + 1):
        correspondence.extend(
            [
                mapping(f"ligand[{ligand}].x1", f"compound[1].x{ligand}", premises),
                mapping(f"ligand[{ligand}].x2", f"compound[2].x{ligand}", premises),
            ]
        )
    rewrite = [
        cleavage(
            "central[1].x1",
            "central[1].x2",
            "single",
            binary_state(state_tuple(0, 6, 0), state_tuple(0, 6, 0)),
            binary_state(state_tuple(0, 7, 1), state_tuple(0, 7, 1)),
        )
    ]
    for ligand in range(1, count + 1):
        rewrite.append(
            cleavage(
                f"ligand[{ligand}].x1",
                f"ligand[{ligand}].x2",
                "single",
                binary_state(state_tuple(0, 6, 0), state_tuple(0, 6, 0)),
                binary_state(state_tuple(0, 7, 1), state_tuple(0, 7, 1)),
            )
        )
    if count > 1:
        for central in (1, 2):
            rewrite.append(
                {
                    "kind": "reconfigure_electrons",
                    "premise_ids": COVALENT_PREMISES,
                    "atom": f"central[1].x{central}",
                    "before": state_tuple(0, 7, 1),
                    "after": state_tuple(0, 7, count),
                }
            )
    for product in (1, 2):
        central = f"central[1].x{product}"
        product_atoms = [central]
        for ligand in range(1, count + 1):
            ligand_atom = f"ligand[{ligand}].x{product}"
            before_nb = 8 - ligand
            before_unpaired = count - ligand + 1
            rewrite.append(
                formation(
                    central,
                    ligand_atom,
                    binary_state(state_tuple(0, before_nb, before_unpaired), state_tuple(0, 7, 1)),
                    binary_state(state_tuple(0, before_nb - 1, before_unpaired - 1), state_tuple(0, 6, 0)),
                )
            )
            product_atoms.append(ligand_atom)
        rewrite.append(assignment(product_atoms, f"compound[{product}]"))
    return generalized_rule(
        f"Rules.Interhalogen{count}Combination",
        {"central": parameter("Categories.Halide"), "ligand": parameter("Categories.Halide")},
        {"central": role("reactant", 1), "ligand": role("reactant", count), "compound": role("product", 2)},
        {
            "central": template_ref("Templates.DiatomicHalogen", {"halogen": parameter_value("central")}),
            "ligand": template_ref("Templates.DiatomicHalogen", {"halogen": parameter_value("ligand")}),
        },
        {
            "compound": template_ref(
                f"Templates.Interhalogen{count}",
                {"central": parameter_value("central"), "ligand": parameter_value("ligand")},
            )
        },
        {"central": "Patterns.InterhalogenCentral", "ligand": "Patterns.InterhalogenLigand"},
        correspondence,
        rewrite,
        "ligand",
        [HALOGEN_CATEGORY_PREMISE, HALOGEN_STRUCTURE_PREMISE],
        pair_condition(pairs),
    )


def build_candidate() -> tuple[dict, list[dict]]:
    states: dict[tuple, dict] = {}
    add_state(states, "H", 0, 0, 1)
    add_state(states, "H", 1, 1, 0)
    for halogen in ("F", "Cl", "Br", "I"):
        add_state(states, halogen, 6, 0, 1)
        add_state(states, halogen, 7, 1, 0)
        for count in (1, 3, 5, 7):
            for formed in range(0, count + 1):
                add_state(states, halogen, 7 - formed, count - formed, formed)
    for formed in range(0, 4):
        add_state(states, "N", 5 - formed, 3 - formed, formed)
    add_state(states, "N", 2, 0, 3)
    for formed in range(0, 3):
        add_state(states, "S", 6 - formed, 2 - formed, formed)
    add_state(states, "S", 4, 0, 2)

    ammonia_templates, ammonia_applications, ammonia_reaction = ammonia_rule()
    sulfur_templates, sulfur_applications, hydrogen_sulfide_reaction = hydrogen_sulfide_rule()

    templates = [*ammonia_templates, *sulfur_templates, *[interhalogen_template(count) for count in INTERHALOGENS]]
    applications = [*ammonia_applications, *sulfur_applications]
    experiences = []
    for count, pairs in INTERHALOGENS.items():
        for central, ligand, formula, product_name in pairs:
            structure_id = f"Interhalogen{formula}"
            applications.append(
                {
                    "id": structure_id,
                    "template": f"Templates.Interhalogen{count}",
                    "arguments": {"central": central, "ligand": ligand},
                    "formula": formula,
                    "premise_ids": COVALENT_PREMISES,
                }
            )
            experiences.append(
                {
                    "slug": f"{central.lower()}-{ligand.lower()}-{formula.lower()}",
                    "reaction": f"{formula}FromElements",
                    "first": central,
                    "second": ligand,
                    "first_coefficient": 1,
                    "second_coefficient": count,
                    "first_structure": ELEMENTAL_STRUCTURES[central],
                    "second_structure": ELEMENTAL_STRUCTURES[ligand],
                    "first_formula": f"{central}2",
                    "second_formula": f"{ligand}2",
                    "product_coefficient": 2,
                    "product_structure": structure_id,
                    "product_formula": formula,
                    "product_name": product_name,
                    "equation": f"{central}2 + {'' if count == 1 else f'{count} '}{ligand}2 -> 2 {formula}",
                    "rule": f"Rules.Interhalogen{count}Combination",
                    "roles": ("central", "ligand"),
                }
            )

    for halide, product_structure, formula, product_name in HYDROGEN_HALIDES:
        experiences.append(
            {
                "slug": f"h-{halide.lower()}-{formula.lower()}",
                "reaction": f"{formula}FromElements",
                "first": "H",
                "second": halide,
                "first_coefficient": 1,
                "second_coefficient": 1,
                "first_structure": "Hydrogen",
                "second_structure": ELEMENTAL_STRUCTURES[halide],
                "first_formula": "H2",
                "second_formula": f"{halide}2",
                "product_coefficient": 2,
                "product_structure": product_structure,
                "product_formula": formula,
                "product_name": product_name,
                "equation": f"H2 + {halide}2 -> 2 {formula}",
                "rule": "Rules.HydrogenHalideCombination",
                "roles": ("hydrogen", "halogen"),
            }
        )
    experiences.extend(
        [
            {
                "slug": "h-n-nh3",
                "reaction": "AmmoniaFromElements",
                "first": "H",
                "second": "N",
                "first_coefficient": 3,
                "second_coefficient": 1,
                "first_structure": "Hydrogen",
                "second_structure": "CovalentNitrogen",
                "first_formula": "H2",
                "second_formula": "N2",
                "product_coefficient": 2,
                "product_structure": "Ammonia",
                "product_formula": "NH3",
                "product_name": "ammonia",
                "equation": "3 H2 + N2 -> 2 NH3",
                "rule": "Rules.AmmoniaCombination",
                "roles": ("hydrogen", "nitrogen"),
            },
            {
                "slug": "h-s-h2s",
                "reaction": "HydrogenSulfideFromElements",
                "first": "H",
                "second": "S",
                "first_coefficient": 8,
                "second_coefficient": 1,
                "first_structure": "Hydrogen",
                "second_structure": "CovalentSulfur",
                "first_formula": "H2",
                "second_formula": "S8",
                "product_coefficient": 8,
                "product_structure": "HydrogenSulfide",
                "product_formula": "H2S",
                "product_name": "hydrogen sulfide",
                "equation": "8 H2 + S8 -> 8 H2S",
                "rule": "Rules.HydrogenSulfideCombination",
                "roles": ("hydrogen", "sulfur"),
            },
        ]
    )

    sulfur_edges = [(f"a{index}", f"a{index + 1 if index < 8 else 1}", "single") for index in range(1, 9)]
    patterns = [
        hydrogen_pattern(),
        covalent_pattern(
            "Patterns.CovalentHydrogenHalidePartner",
            parameter_value("halide"),
            ["x1", "x2"],
            [("x1", "x2", "single")],
        ),
        covalent_pattern(
            "Patterns.CovalentNitrogen",
            parameter_value("member"),
            ["a1", "a2"],
            [("a1", "a2", "triple")],
        ),
        covalent_pattern(
            "Patterns.CovalentSulfur",
            parameter_value("member"),
            [f"a{index}" for index in range(1, 9)],
            sulfur_edges,
        ),
        covalent_pattern(
            "Patterns.InterhalogenCentral",
            parameter_value("central"),
            ["x1", "x2"],
            [("x1", "x2", "single")],
        ),
        covalent_pattern(
            "Patterns.InterhalogenLigand",
            parameter_value("ligand"),
            ["x1", "x2"],
            [("x1", "x2", "single")],
        ),
    ]

    candidate = {
        "schema_version": 1,
        "id": "covalent-combinations",
        "evidence": [
            {
                "id": "evidence.openstax.hydrogen-compounds",
                "title": "Chemistry: Atoms First",
                "publisher": "OpenStax",
                "locator": "Occurrence, Preparation, and Compounds of Hydrogen",
                "reference": "https://openstax.org/books/chemistry-atoms-first/pages/18-5-occurrence-preparation-and-compounds-of-hydrogen",
                "retrieved_on": "2026-07-15",
                "usage": "Diatomic hydrogen, ammonia, water, hydrogen sulfide, and hydrogen-halide combination equations and stated conditions",
            },
            {
                "id": "evidence.openstax.interhalogens",
                "title": "Chemistry 2e",
                "publisher": "OpenStax",
                "locator": "Occurrence, Preparation, and Properties of Halogens",
                "reference": "https://openstax.org/books/chemistry-2e/pages/18-11-occurrence-preparation-and-properties-of-halogens",
                "retrieved_on": "2026-07-15",
                "usage": "Diatomic halogens, interhalogen formula families, direct elemental formation, and single-bond star structures",
            },
        ],
        "premises": [
            {
                "id": RULE_PREMISE,
                "statement": "Only the listed hydrogen-compound and interhalogen bindings have the representative balanced covalent outcomes encoded by this finite catalogue.",
                "evidence": ["evidence.openstax.hydrogen-compounds", "evidence.openstax.interhalogens"],
                "review": {"status": "provisional", "reviewers": []},
                "rule_version": "1",
            },
            {
                "id": STRUCTURE_PREMISE,
                "statement": "Supported products use explicit localized Lewis graphs; interhalogens have one heavier central halogen joined by single bonds to an odd number of lighter halogens.",
                "evidence": ["evidence.openstax.hydrogen-compounds", "evidence.openstax.interhalogens"],
                "review": {"status": "provisional", "reviewers": []},
                "rule_version": "1",
            },
            {
                "id": VALENCE_PREMISE,
                "statement": "The listed exact electron states are the closed explanatory domain used to cleave elemental bonds and form each reviewed covalent product graph.",
                "evidence": ["evidence.openstax.hydrogen-compounds", "evidence.openstax.interhalogens"],
                "review": {"status": "provisional", "reviewers": []},
                "rule_version": "1",
            },
            {
                "id": OBSERVATION_PREMISE,
                "statement": "Formation of the selected covalent product and disappearance of an elemental reactant are compatible generic observations for these representative theoretical experiences.",
                "evidence": ["evidence.openstax.hydrogen-compounds", "evidence.openstax.interhalogens"],
                "review": {"status": "provisional", "reviewers": []},
                "rule_version": "1",
            },
        ],
        "valence_premises": [
            {
                "premise_id": VALENCE_PREMISE,
                "neutral_valence": [
                    {"element": symbol, "neutral_valence_electrons": value}
                    for symbol, value in [("H", 1), ("N", 5), ("F", 7), ("S", 6), ("Cl", 7), ("Br", 7), ("I", 7)]
                ],
                "supported_states": list(states.values()),
                "metallic_domain_states": [],
            }
        ],
        "structures": [],
        "rules": [],
        "elements": [],
        "element_categories": [
            {
                "id": "Categories.CovalentNitrogen",
                "subject": "element",
                "membership": {"kind": "explicit", "members": ["N"]},
                "premise_ids": [RULE_PREMISE],
            },
            {
                "id": "Categories.CovalentSulfur",
                "subject": "element",
                "membership": {"kind": "explicit", "members": ["S"]},
                "premise_ids": [RULE_PREMISE],
            },
        ],
        "structural_traits": [],
        "structure_templates": templates,
        "structure_applications": applications,
        "graph_patterns": patterns,
        "generalized_rules": [
            hydrogen_halide_rule(),
            ammonia_reaction,
            hydrogen_sulfide_reaction,
            *[interhalogen_rule(count, pairs) for count, pairs in INTERHALOGENS.items()],
        ],
    }
    return candidate, experiences


def evidence_packet(source_kind: str | None = None) -> dict:
    selected_source_ids = {
        "hydrogen": ["S1"],
        "halogen": ["S2"],
        None: ["S1", "S2"],
    }[source_kind]
    return {
        "schema_version": 1,
        "id": "Evidence.CovalentCombination@1",
        "claims": [
            {"id": "R1", "subject_role": "product", "subject": "product", "predicate": "forms", "sources": selected_source_ids},
            {"id": "R2", "subject_role": "reactant", "subject": "element", "predicate": "disappears", "sources": selected_source_ids},
        ],
        "sources": [source for source in [
            {
                "id": "S1",
                "title": "Occurrence, Preparation, and Compounds of Hydrogen",
                "publisher": "OpenStax",
                "url": "https://openstax.org/books/chemistry-atoms-first/pages/18-5-occurrence-preparation-and-compounds-of-hydrogen",
                "supports": ["R1", "R2"],
            },
            {
                "id": "S2",
                "title": "Occurrence, Preparation, and Properties of Halogens",
                "publisher": "OpenStax",
                "url": "https://openstax.org/books/chemistry-2e/pages/18-11-occurrence-preparation-and-properties-of-halogens",
                "supports": ["R1", "R2"],
            },
        ] if source["id"] in selected_source_ids],
    }


def source_for(experience: dict) -> str:
    first_role, second_role = experience["roles"]
    lines = [
        "chems 1",
        "use catalog ChemSpec.Theoretical@1",
        f"reaction {experience['reaction']} where",
        "  reactants",
        f"    {first_role} := {experience['first_coefficient']} of {experience['first_structure']}",
        f"    {second_role} := {experience['second_coefficient']} of {experience['second_structure']}",
        "  products",
        f"    compound := {experience['product_coefficient']} of {experience['product_structure']}",
        "  equation",
        f"    {experience['first_coefficient']} {experience['first_formula']}[molecular] + {experience['second_coefficient']} {experience['second_formula']}[molecular]",
        f"    -> {experience['product_coefficient']} {experience['product_formula']}[molecular]",
        "  model",
        "    event := representative",
        "    sequence := explanatory",
        "  observe from Evidence.CovalentCombination@1",
        "    product compound forms claim R1",
        f"    reactant {second_role} disappears claim R2",
        "  by",
        f"    apply {experience['rule']}",
        f"      {first_role} := {first_role}",
        f"      {second_role} := {second_role}",
        "      compound := compound",
    ]
    return "\n".join(lines) + "\n"


def update_registry(root: Path, experiences: list[dict]) -> None:
    registry_path = root / "catalogue/experience-registry.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    trusted_path = root / "catalogue/trusted/core-chemistry/catalogue.json"
    trusted = trusted_path.exists() and "Rules.HydrogenHalideCombination" in trusted_path.read_text(encoding="utf-8")
    previous_trusted = any(
        record.get("id", "").startswith("covalent-") and record.get("status") == "trusted"
        for record in registry["experiences"]
    )
    status = "trusted" if trusted or previous_trusted else "candidate"
    base = [record for record in registry["experiences"] if not record.get("id", "").startswith("covalent-")]
    records = []
    for experience in experiences:
        first = experience["first"]
        second = experience["second"]
        records.append(
            {
                "id": f"covalent-{experience['slug']}",
                "status": status,
                "family": "covalent_combination",
                "participants": [
                    {"kind": "element", "atomic_number": ATOMIC_NUMBERS[first]},
                    {"kind": "element", "atomic_number": ATOMIC_NUMBERS[second]},
                ],
                "source_path": f"conformance/end-to-end/covalent-{experience['slug']}-001.chems",
                "evidence_path": f"conformance/observations/covalent-{experience['slug']}-001.evidence.json",
                "request": f"What covalent product forms when {ELEMENT_NAMES[first]} reacts with {ELEMENT_NAMES[second]}?",
                "equation": experience["equation"],
                "subject_name": ELEMENT_NAMES[first],
                "product_name": experience["product_name"],
                "product_structure": experience["product_structure"],
            }
        )
    registry["experiences"] = base + records
    write_json(registry_path, registry)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    candidate, experiences = build_candidate()
    packet = evidence_packet()
    candidate_dir = root / "catalogue/candidates/covalent-combinations"
    end_to_end = root / "conformance/end-to-end"
    observations = root / "conformance/observations"
    write_json(candidate_dir / "candidate.json", candidate)
    write_json(candidate_dir / "evidence.json", packet)
    for experience in experiences:
        source_path = end_to_end / f"covalent-{experience['slug']}-001.chems"
        source_path.write_text(source_for(experience), encoding="utf-8")
        source_kind = "hydrogen" if experience["first"] == "H" else "halogen"
        write_json(
            observations / f"covalent-{experience['slug']}-001.evidence.json",
            evidence_packet(source_kind),
        )
    shutil.copyfile(end_to_end / "covalent-h-cl-hcl-001.chems", candidate_dir / "example.chems")
    update_registry(root, experiences)
    print(f"Generated 7 reusable covalent rules and {len(experiences)} finite experiences.")


if __name__ == "__main__":
    main()
