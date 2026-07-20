#!/usr/bin/env python3
"""Generates the oxygen-reactions candidate package, its conformance
experiences, and the oxygen/ion-pair portion of the experience registry.

Port of the retired generate-oxygen-catalogue.ps1 (this machine has no
pwsh). Output order is replicated exactly: array order inside candidate.json
feeds the aggregate catalogue digest, so any reordering is a content change.
The macroscopic standard-phase records (added 2026-07-19) are emitted here
too, so regeneration cannot drop them.
"""

from __future__ import annotations

import json
import math
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CANDIDATE_DIR = ROOT / "catalogue/candidates/oxygen-reactions"
EXPERIENCE_DIR = ROOT / "conformance/end-to-end"
OBSERVATION_DIR = ROOT / "conformance/observations"

RULE_PREMISE = "premise.rule.element-oxygen.representative-outcomes"
STRUCTURE_PREMISE = "premise.structure.element-oxygen.structural-models"
VALENCE_PREMISE = "premise.valence.element-oxygen.closed-domain"
OBSERVATION_PREMISE = "premise.observation.element-oxygen"
ION_RULE_PREMISE = "premise.rule.fixed-charge-ion-pairs"
ION_STRUCTURE_PREMISE = "premise.structure.fixed-charge-ion-pairs"
ION_VALENCE_PREMISE = "premise.valence.fixed-charge-ion-pairs"
ION_OBSERVATION_PREMISE = "premise.observation.fixed-charge-ion-pairs"


def all_premises() -> list[str]:
    return [RULE_PREMISE, STRUCTURE_PREMISE, VALENCE_PREMISE]


def ion_premises() -> list[str]:
    return [ION_RULE_PREMISE, ION_STRUCTURE_PREMISE, ION_VALENCE_PREMISE]


def write_utf8(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def write_json(path: Path, value) -> None:
    write_utf8(path, json.dumps(value, indent=2) + "\n")


def atom(label, element, charge, non_bonding, unpaired):
    return {
        "label": label,
        "element": element,
        "formal_charge": charge,
        "non_bonding_electrons": non_bonding,
        "unpaired_electrons": unpaired,
    }


def bond(left, right, order, delocalization=None):
    value = {"left": left, "right": right, "order": order}
    if delocalization is not None:
        value["delocalization"] = delocalization
    return value


def binary_state(left, right):
    return {"left": left, "right": right}


def premise(premise_id, statement):
    return {
        "id": premise_id,
        "statement": statement,
        "evidence": ["evidence.openstax.oxygen-compounds"],
        "review": {"status": "provisional", "reviewers": []},
        "rule_version": "1",
    }


def parameter(category):
    return {"kind": "element", "category": category}


def parameter_value():
    return {"parameter": "member"}


def exact(structure):
    return {"kind": "exact", "structure": structure}


def template_ref(template):
    return {"kind": "template", "template": template, "arguments": {"member": parameter_value()}}


def role(side, representation, coefficient):
    return {"side": side, "representation": representation, "coefficient": coefficient}


def mapping_entry(reactant, product):
    return {"reactant": reactant, "product": product, "premise_ids": all_premises()}


def assignment(atoms, product):
    return {"kind": "assign_product", "premise_ids": all_premises(), "atoms": atoms, "product": product}


def observation_compatibility():
    return [
        {
            "subject_role": "oxide",
            "predicate": "forms",
            "evidence_subject": "oxide",
            "premise_id": OBSERVATION_PREMISE,
        },
        {
            "subject_role": "subject",
            "predicate": "disappears",
            "evidence_subject": "element",
            "premise_id": OBSERVATION_PREMISE,
        },
    ]


states: dict[str, dict] = {}


def add_state(element, charge, non_bonding, unpaired, bond_sum):
    key = f"{element}|{charge}|{non_bonding}|{unpaired}|{bond_sum}"
    states[key] = {
        "element": element,
        "formal_charge": charge,
        "non_bonding_electrons": non_bonding,
        "unpaired_electrons": unpaired,
        "covalent_bond_order_sum": bond_sum,
    }


ELEMENTS = {
    "H": 1, "Li": 1, "Be": 2, "B": 3, "C": 4, "N": 5, "O": 6, "F": 7,
    "Na": 1, "Mg": 2, "Al": 3, "Si": 4, "P": 5, "S": 6, "Cl": 7,
    "K": 1, "Ca": 2, "Sc": 3, "Ti": 4, "V": 5, "Cr": 6, "Mn": 7, "Fe": 8,
    "Co": 9, "Ni": 10, "Cu": 11, "Zn": 12, "Br": 7,
    "Rb": 1, "Sr": 2, "Y": 3, "Zr": 4, "Nb": 5, "Mo": 6, "Tc": 7, "Ru": 8,
    "Rh": 9, "Pd": 10, "Ag": 11, "Cd": 12, "I": 7,
    "Cs": 1, "Ba": 2, "Hf": 4, "Ta": 5, "W": 6, "Re": 7, "Os": 8, "Ir": 9,
    "Pt": 10, "Au": 11, "Hg": 12,
}

add_state("O", 0, 4, 0, 2)
add_state("O", 0, 5, 1, 1)
add_state("O", -1, 6, 0, 1)
add_state("O", 0, 6, 2, 0)
add_state("O", -1, 7, 1, 0)
add_state("O", -2, 8, 0, 0)
add_state("H", 0, 0, 0, 1)
add_state("H", 0, 1, 1, 0)
add_state("B", 0, 3, 3, 0)
add_state("B", 0, 2, 2, 1)
add_state("B", 0, 1, 1, 2)
add_state("B", 0, 0, 0, 3)
for element in ["C", "Si"]:
    add_state(element, 0, 4, 4, 0)
    add_state(element, 0, 2, 2, 2)
    add_state(element, 0, 0, 0, 4)
add_state("S", 0, 6, 4, 0)
add_state("S", 0, 4, 2, 2)
add_state("S", 0, 2, 0, 4)

categories: list[dict] = []
templates: list[dict] = []
applications: list[dict] = []
patterns: list[dict] = []
rules: list[dict] = []
structures: list[dict] = [
    {
        "representation": "molecular",
        "id": "Oxygen",
        "premise_id": STRUCTURE_PREMISE,
        "formula": "O2",
        "atoms": [atom("o1", "O", 0, 4, 0), atom("o2", "O", 0, 4, 0)],
        "bonds": [bond("o1", "o2", "double")],
        "groups": [],
    }
]


def add_category(category_id, members, premise_id=RULE_PREMISE):
    categories.append(
        {
            "id": category_id,
            "subject": "element",
            "membership": {"kind": "explicit", "members": list(members)},
            "premise_ids": [premise_id],
        }
    )


def add_metal_family_scaffold(name, category, members, charge, product_kind, metal_count, oxygen_count):
    add_category(category, members)
    metal_template = f"Templates.{name}Metal"
    product_template = f"Templates.{name}Product"
    metal_pattern = f"Patterns.{name}Metal"
    templates.append(
        {
            "representation": "metallic",
            "id": metal_template,
            "parameters": {"member": parameter(category)},
            "sites": [atom("metal", parameter_value(), charge, 0, 0)],
            "domains": [{"label": "metallic", "sites": ["metal"], "delocalized_electrons": charge}],
            "premise_ids": all_premises(),
        }
    )
    patterns.append(
        {
            "id": metal_pattern,
            "variables": {"metal": {"atom": {"element": parameter_value()}}},
            "relationships": [
                {"kind": "metallic_domain", "domain": "metallic", "sites": ["metal"], "delocalized_electrons": charge}
            ],
            "premise_ids": all_premises(),
        }
    )
    for member in members:
        add_state(member, charge, 0, 0, 0)
        for remaining in range(charge, -1, -1):
            add_state(member, charge - remaining, remaining, remaining, 0)
        application_id = f"Fe{name}MetalForOxygen" if member == "Fe" else f"{member}MetalForOxygen"
        applications.append(
            {
                "id": application_id,
                "template": metal_template,
                "arguments": {"member": member},
                "formula": member,
                "premise_ids": all_premises(),
            }
        )

    components = []
    for m in range(1, metal_count + 1):
        components.append(
            {"label": f"metal{m}", "atoms": [atom("metal", parameter_value(), charge, 0, 0)], "bonds": [], "groups": []}
        )
    if product_kind == "normal":
        for o in range(1, oxygen_count + 1):
            components.append({"label": f"oxide{o}", "atoms": [atom("o", "O", -2, 8, 0)], "bonds": [], "groups": []})
    else:
        deloc = (
            {"domain": "oxygen.resonance", "effective_order": {"numerator": 3, "denominator": 2}}
            if product_kind == "superoxide"
            else None
        )
        o1_charge = -1
        o2_charge = 0 if product_kind == "superoxide" else -1
        o2_nb = 5 if product_kind == "superoxide" else 6
        o2_u = 1 if product_kind == "superoxide" else 0
        components.append(
            {
                "label": product_kind,
                "atoms": [atom("o1", "O", o1_charge, 6, 0), atom("o2", "O", o2_charge, o2_nb, o2_u)],
                "bonds": [bond("o1", "o2", "single", deloc)],
                "groups": [],
            }
        )
    component_labels = [component["label"] for component in components]
    templates.append(
        {
            "representation": "ionic",
            "id": product_template,
            "parameters": {"member": parameter(category)},
            "components": components,
            "associations": [{"label": "ionic", "components": component_labels}],
            "premise_ids": all_premises(),
        }
    )
    return {"metal_template": metal_template, "product_template": product_template, "metal_pattern": metal_pattern}


patterns.append(
    {
        "id": "Patterns.Oxygen",
        "variables": {"o1": {"atom": {"element": "O"}}, "o2": {"atom": {"element": "O"}}},
        "relationships": [{"kind": "covalent", "bond": "oo", "left": "o1", "right": "o2", "order": "double"}],
        "premise_ids": all_premises(),
    }
)


def new_base_rule(rule_id, category, roles, reactants, products, patterns_for_case, mapping, rewrite):
    parameters = {} if category is None else {"member": parameter(category)}
    return {
        "id": rule_id,
        "parameters": parameters,
        "roles": roles,
        "reactants": reactants,
        "cases": [
            {
                "status": "supported",
                "id": "standard",
                "when": {"kind": "always"},
                "products": products,
                "patterns": patterns_for_case,
                "correspondence": mapping,
                "rewrite": rewrite,
                "observation_compatibility": observation_compatibility(),
                "premise_ids": [RULE_PREMISE],
            }
        ],
        "applicability": {
            "premise_id": RULE_PREMISE,
            "request_relation": "contact",
            "required_context": "representative theoretical oxidation outcome selected by the reviewed oxygen catalogue",
        },
        "model_assumptions": {"event": "representative", "sequence": "explanatory", "premise_ids": [RULE_PREMISE]},
        "premise_ids": [
            "premise.elements.iupac-periodic-table",
            RULE_PREMISE,
            STRUCTURE_PREMISE,
            VALENCE_PREMISE,
            OBSERVATION_PREMISE,
        ],
    }


def release_metal(ref, charge):
    return {
        "kind": "release_metallic",
        "premise_ids": all_premises(),
        "site": f"{ref}.metal",
        "domain": f"{ref}.metallic",
        "allocation": "retain_electron",
        "before": {"site": [charge, 0, 0], "domain_electrons": charge},
        "after": {"site": [0, charge, charge], "domain_electrons": 0},
    }


def cleave_oxygen(ref):
    return {
        "kind": "cleave_covalent",
        "premise_ids": all_premises(),
        "edge": [f"{ref}.o1", f"{ref}.o2", "double"],
        "allocation": "homolytic",
        "before": binary_state([0, 4, 0], [0, 4, 0]),
        "after": binary_state([0, 6, 2], [0, 6, 2]),
    }


def change_oxygen_to_single(ref):
    return {
        "kind": "change_covalent",
        "premise_ids": all_premises(),
        "edge": [f"{ref}.o1", f"{ref}.o2"],
        "old_order": "double",
        "new_order": "single",
        "allocation": "homolytic",
        "before": binary_state([0, 4, 0], [0, 4, 0]),
        "after": binary_state([0, 5, 1], [0, 5, 1]),
    }


def add_normal_oxide_family(
    name, category, members, charge, metal_per_product, oxygen_per_product,
    metal_coefficient, oxygen_coefficient, product_coefficient, formulas,
):
    scaffold = add_metal_family_scaffold(name, category, members, charge, "normal", metal_per_product, oxygen_per_product)
    for member in members:
        applications.append(
            {
                "id": f"{member}{name}",
                "template": scaffold["product_template"],
                "arguments": {"member": member},
                "formula": formulas[member],
                "premise_ids": all_premises(),
            }
        )
    roles = {
        "subject": role("reactant", "metallic", metal_coefficient),
        "oxygen": role("reactant", "molecular", oxygen_coefficient),
        "oxide": role("product", "ionic", product_coefficient),
    }
    reactants = {"subject": template_ref(scaffold["metal_template"]), "oxygen": exact("Oxygen")}
    products = {"oxide": template_ref(scaffold["product_template"])}
    case_patterns = {"subject": scaffold["metal_pattern"], "oxygen": "Patterns.Oxygen"}
    mapping = []
    rewrite = []
    for m in range(1, metal_coefficient + 1):
        unit = (m - 1) // metal_per_product + 1
        slot = (m - 1) % metal_per_product + 1
        mapping.append(mapping_entry(f"subject[{m}].metal", f"oxide[{unit}].metal{slot}.metal"))
        rewrite.append(release_metal(f"subject[{m}]", charge))
    oxygen_atoms = []
    for o in range(1, oxygen_coefficient + 1):
        rewrite.append(cleave_oxygen(f"oxygen[{o}]"))
        oxygen_atoms.extend([f"oxygen[{o}].o1", f"oxygen[{o}].o2"])
    for i, oxygen_atom in enumerate(oxygen_atoms):
        unit = i // oxygen_per_product + 1
        slot = i % oxygen_per_product + 1
        mapping.append(mapping_entry(oxygen_atom, f"oxide[{unit}].oxide{slot}.o"))
    donor_remaining = [charge] * metal_coefficient
    accept_remaining = [2] * len(oxygen_atoms)
    di = 0
    ai = 0
    while di < len(donor_remaining) and ai < len(accept_remaining):
        count = min(donor_remaining[di], accept_remaining[ai])
        db = donor_remaining[di]
        ab = 2 - accept_remaining[ai]
        da = db - count
        aa = ab + count
        rewrite.append(
            {
                "kind": "transfer_electron",
                "premise_ids": all_premises(),
                "count": count,
                "donor": f"subject[{di + 1}].metal",
                "acceptor": oxygen_atoms[ai],
                "before": {"donor": [charge - db, db, db], "acceptor": [-ab, 6 + ab, 2 - ab]},
                "after": {"donor": [charge - da, da, da], "acceptor": [-aa, 6 + aa, 2 - aa]},
            }
        )
        donor_remaining[di] -= count
        accept_remaining[ai] -= count
        if donor_remaining[di] == 0:
            di += 1
        if accept_remaining[ai] == 0:
            ai += 1
    for u in range(1, product_coefficient + 1):
        atoms = []
        components = []
        charges = []
        for m in range(1, metal_per_product + 1):
            idx = (u - 1) * metal_per_product + m
            atoms.append(f"subject[{idx}].metal")
            components.append([f"subject[{idx}].metal"])
            charges.append(charge)
        for o in range(1, oxygen_per_product + 1):
            idx = (u - 1) * oxygen_per_product + o
            oxygen_atom = oxygen_atoms[idx - 1]
            atoms.append(oxygen_atom)
            components.append([oxygen_atom])
            charges.append(-2)
        rewrite.append(
            {
                "kind": "associate_ionic",
                "premise_ids": all_premises(),
                "label": f"ionic.product{u}",
                "components": components,
                "component_charges": charges,
            }
        )
        rewrite.append(assignment(atoms, f"oxide[{u}]"))
    rules.append(new_base_rule(f"Rules.{name}", category, roles, reactants, products, case_patterns, mapping, rewrite))


def add_oxygen_anion_family(name, category, members, charge, kind, metal_count, formulas):
    scaffold = add_metal_family_scaffold(name, category, members, charge, kind, metal_count, 2)
    for member in members:
        applications.append(
            {
                "id": f"{member}{name}",
                "template": scaffold["product_template"],
                "arguments": {"member": member},
                "formula": formulas[member],
                "premise_ids": all_premises(),
            }
        )
    roles = {
        "subject": role("reactant", "metallic", metal_count),
        "oxygen": role("reactant", "molecular", 1),
        "oxide": role("product", "ionic", 1),
    }
    mapping = []
    rewrite = []
    for m in range(1, metal_count + 1):
        mapping.append(mapping_entry(f"subject[{m}].metal", f"oxide[1].metal{m}.metal"))
        rewrite.append(release_metal(f"subject[{m}]", charge))
    mapping.append(mapping_entry("oxygen[1].o1", f"oxide[1].{kind}.o1"))
    mapping.append(mapping_entry("oxygen[1].o2", f"oxide[1].{kind}.o2"))
    rewrite.append(change_oxygen_to_single("oxygen[1]"))
    donor_remaining = [charge] * metal_count
    oxygen_refs = ["oxygen[1].o1", "oxygen[1].o2"]
    di = 0
    electron_targets = 1 if kind == "superoxide" else 2
    for oi in range(electron_targets):
        db = donor_remaining[di]
        rewrite.append(
            {
                "kind": "transfer_electron",
                "premise_ids": all_premises(),
                "count": 1,
                "donor": f"subject[{di + 1}].metal",
                "acceptor": oxygen_refs[oi],
                "before": {"donor": [charge - db, db, db], "acceptor": [0, 5, 1]},
                "after": {"donor": [charge - (db - 1), db - 1, db - 1], "acceptor": [-1, 6, 0]},
            }
        )
        donor_remaining[di] -= 1
        if donor_remaining[di] == 0:
            di += 1
    if kind == "superoxide":
        rewrite.append(
            {
                "kind": "change_covalent_delocalization",
                "premise_ids": all_premises(),
                "edge": ["oxygen[1].o1", "oxygen[1].o2"],
                "expected": None,
                "replacement": {"domain": "oxygen.resonance", "effective_order": {"numerator": 3, "denominator": 2}},
            }
        )
    components = []
    charges = []
    atoms = []
    for m in range(1, metal_count + 1):
        components.append([f"subject[{m}].metal"])
        charges.append(charge)
        atoms.append(f"subject[{m}].metal")
    components.append(["oxygen[1].o1", "oxygen[1].o2"])
    charges.append(-1 * charge * metal_count)
    atoms.extend(["oxygen[1].o1", "oxygen[1].o2"])
    rewrite.append(
        {
            "kind": "associate_ionic",
            "premise_ids": all_premises(),
            "label": "ionic.product1",
            "components": components,
            "component_charges": charges,
        }
    )
    rewrite.append(assignment(atoms, "oxide[1]"))
    rules.append(
        new_base_rule(
            f"Rules.{name}",
            category,
            roles,
            {"subject": template_ref(scaffold["metal_template"]), "oxygen": exact("Oxygen")},
            {"oxide": template_ref(scaffold["product_template"])},
            {"subject": scaffold["metal_pattern"], "oxygen": "Patterns.Oxygen"},
            mapping,
            rewrite,
        )
    )


def add_covalent_dioxide_family(name, category, members, initial_non_bonding, formulas):
    add_category(category, members)
    subject_template = f"Templates.{name}Element"
    product_template = f"Templates.{name}Dioxide"
    subject_pattern = f"Patterns.{name}Element"
    templates.append(
        {
            "representation": "molecular",
            "id": subject_template,
            "parameters": {"member": parameter(category)},
            "atoms": [atom("x", parameter_value(), 0, initial_non_bonding, 4)],
            "bonds": [],
            "groups": [],
            "premise_ids": all_premises(),
        }
    )
    templates.append(
        {
            "representation": "molecular",
            "id": product_template,
            "parameters": {"member": parameter(category)},
            "atoms": [
                atom("x", parameter_value(), 0, initial_non_bonding - 4, 0),
                atom("o1", "O", 0, 4, 0),
                atom("o2", "O", 0, 4, 0),
            ],
            "bonds": [bond("x", "o1", "double"), bond("x", "o2", "double")],
            "groups": [],
            "premise_ids": all_premises(),
        }
    )
    patterns.append(
        {
            "id": subject_pattern,
            "variables": {"x": {"atom": {"element": parameter_value()}}},
            "relationships": [],
            "premise_ids": all_premises(),
        }
    )
    for member in members:
        applications.append(
            {
                "id": f"{member}ForOxygen",
                "template": subject_template,
                "arguments": {"member": member},
                "formula": member,
                "premise_ids": all_premises(),
            }
        )
        applications.append(
            {
                "id": f"{member}{name}",
                "template": product_template,
                "arguments": {"member": member},
                "formula": formulas[member],
                "premise_ids": all_premises(),
            }
        )
    mapping = [
        mapping_entry("subject[1].x", "oxide[1].x"),
        mapping_entry("oxygen[1].o1", "oxide[1].o1"),
        mapping_entry("oxygen[1].o2", "oxide[1].o2"),
    ]
    rewrite = [cleave_oxygen("oxygen[1]")]
    x_before = initial_non_bonding
    for _ in ["o1", "o2"]:
        rewrite.append(
            {
                "kind": "form_covalent",
                "premise_ids": all_premises(),
                "edge": ["subject[1].x", f"oxygen[1].{_}", "double"],
                "electron_contribution": {"left": 2, "right": 2},
                "before": binary_state(
                    [0, x_before, x_before - (initial_non_bonding - 4)], [0, 6, 2]
                ),
                "after": binary_state(
                    [0, x_before - 2, max(0, (x_before - (initial_non_bonding - 4)) - 2)], [0, 4, 0]
                ),
            }
        )
        x_before -= 2
    rewrite.append(assignment(["subject[1].x", "oxygen[1].o1", "oxygen[1].o2"], "oxide[1]"))
    rules.append(
        new_base_rule(
            f"Rules.{name}",
            category,
            {
                "subject": role("reactant", "molecular", 1),
                "oxygen": role("reactant", "molecular", 1),
                "oxide": role("product", "molecular", 1),
            },
            {"subject": template_ref(subject_template), "oxygen": exact("Oxygen")},
            {"oxide": template_ref(product_template)},
            {"subject": subject_pattern, "oxygen": "Patterns.Oxygen"},
            mapping,
            rewrite,
        )
    )


add_normal_oxide_family(
    "MonovalentNormalOxide", "Categories.MonovalentNormalOxideMetal", ["Li"], 1, 2, 1, 4, 1, 2, {"Li": "Li2O"}
)
add_normal_oxide_family(
    "DivalentNormalOxide", "Categories.DivalentNormalOxideMetal", ["Be", "Mg", "Ca", "Sr", "Ba"], 2, 1, 1, 2, 1, 2,
    {"Be": "BeO", "Mg": "MgO", "Ca": "CaO", "Sr": "SrO", "Ba": "BaO"},
)
add_normal_oxide_family(
    "TrivalentNormalOxide", "Categories.TrivalentNormalOxideMetal", ["Al"], 3, 2, 3, 4, 3, 2, {"Al": "Al2O3"}
)
add_oxygen_anion_family(
    "MonovalentPeroxide", "Categories.MonovalentPeroxideMetal", ["Na"], 1, "peroxide", 2, {"Na": "Na2O2"}
)
add_oxygen_anion_family(
    "Superoxide", "Categories.SuperoxideMetal", ["K", "Rb", "Cs"], 1, "superoxide", 1,
    {"K": "KO2", "Rb": "RbO2", "Cs": "CsO2"},
)
add_covalent_dioxide_family(
    "Group14Dioxide", "Categories.Group14DioxideElement", ["C", "Si"], 4, {"C": "CO2", "Si": "SiO2"}
)
add_covalent_dioxide_family("SulfurDioxide", "Categories.SulfurDioxideElement", ["S"], 6, {"S": "SO2"})

# Transition-metal oxidation is encoded by periodic-group source families and
# oxide-stoichiometry product families. A structure application is still made
# for each selected element, but neither the reaction rule nor its electron
# process is authored as an element-specific experience.
transition_experiences: list[list] = []
transition_metallic_states: list[dict] = []
TRANSITION_ATOMIC_NUMBERS = {
    "Sc": 21, "Ti": 22, "V": 23, "Cr": 24, "Mn": 25, "Fe": 26, "Co": 27, "Ni": 28, "Cu": 29, "Zn": 30,
    "Y": 39, "Zr": 40, "Nb": 41, "Mo": 42, "Tc": 43, "Ru": 44, "Rh": 45, "Pd": 46, "Ag": 47, "Cd": 48,
    "Hf": 72, "Ta": 73, "W": 74, "Re": 75, "Os": 76, "Ir": 77, "Pt": 78, "Au": 79, "Hg": 80,
}


def hund_unpaired(local_electrons):
    if local_electrons <= 5:
        return local_electrons
    if local_electrons <= 10:
        return 10 - local_electrons
    return 0


def oxide_formula(member, metal_count, oxygen_count):
    m = "" if metal_count == 1 else str(metal_count)
    o = "O" if oxygen_count == 1 else f"O{oxygen_count}"
    return f"{member}{m}{o}"


def transition_slug(member, oxidations, oxygen_count):
    oxidation_key = "-".join(str(value) for value in oxidations)
    return f"{member.lower()}-oxide-{oxidation_key}-o{oxygen_count}"


def register_transition_metallic_state(member, domain_electrons):
    if not any(state["element"] == member for state in transition_metallic_states):
        transition_metallic_states.append(
            {
                "element": member,
                "site_formal_charge": domain_electrons,
                "site_local_electrons": 0,
                "delocalized_electrons_per_site": domain_electrons,
            }
        )


def add_transition_oxide_family(name, members, domain_electrons, oxidations, oxygen_per_product):
    category = f"Categories.{name}Element"
    metal_template = f"Templates.{name}Metal"
    product_template = f"Templates.{name}Oxide"
    metal_pattern = f"Patterns.{name}Metal"
    metal_per_product = len(oxidations)
    product_coefficient = 1 if oxygen_per_product % 2 == 0 else 2
    metal_coefficient = metal_per_product * product_coefficient
    oxygen_coefficient = (oxygen_per_product * product_coefficient) // 2
    expanded_oxidations = []
    for _ in range(product_coefficient):
        expanded_oxidations.extend(oxidations)

    add_category(category, members)
    templates.append(
        {
            "representation": "metallic",
            "id": metal_template,
            "parameters": {"member": parameter(category)},
            "sites": [atom("metal", parameter_value(), domain_electrons, 0, 0)],
            "domains": [
                {"label": "metallic", "sites": ["metal"], "delocalized_electrons": domain_electrons}
            ],
            "premise_ids": all_premises(),
        }
    )
    patterns.append(
        {
            "id": metal_pattern,
            "variables": {"metal": {"atom": {"element": parameter_value()}}},
            "relationships": [
                {
                    "kind": "metallic_domain",
                    "domain": "metallic",
                    "sites": ["metal"],
                    "delocalized_electrons": domain_electrons,
                }
            ],
            "premise_ids": all_premises(),
        }
    )

    components = []
    for m in range(1, metal_per_product + 1):
        charge = oxidations[m - 1]
        local = domain_electrons - charge
        unpaired = hund_unpaired(local)
        components.append(
            {
                "label": f"metal{m}",
                "atoms": [atom("metal", parameter_value(), charge, local, unpaired)],
                "bonds": [],
                "groups": [],
            }
        )
    for o in range(1, oxygen_per_product + 1):
        components.append({"label": f"oxide{o}", "atoms": [atom("o", "O", -2, 8, 0)], "bonds": [], "groups": []})
    component_labels = [component["label"] for component in components]
    templates.append(
        {
            "representation": "ionic",
            "id": product_template,
            "parameters": {"member": parameter(category)},
            "components": components,
            "associations": [{"label": "ionic", "components": component_labels}],
            "premise_ids": all_premises(),
        }
    )

    for member in members:
        source_id = f"{member}{name}MetalForOxygen"
        product_id = f"{member}{name}Oxide"
        formula = oxide_formula(member, metal_per_product, oxygen_per_product)
        applications.append(
            {
                "id": source_id,
                "template": metal_template,
                "arguments": {"member": member},
                "formula": member,
                "premise_ids": all_premises(),
            }
        )
        applications.append(
            {
                "id": product_id,
                "template": product_template,
                "arguments": {"member": member},
                "formula": formula,
                "premise_ids": all_premises(),
            }
        )
        add_state(member, domain_electrons, 0, 0, 0)
        add_state(member, 0, domain_electrons, domain_electrons, 0)
        for charge in range(1, max(oxidations) + 1):
            add_state(member, charge, domain_electrons - charge, domain_electrons - charge, 0)
        for charge in oxidations:
            local = domain_electrons - charge
            add_state(member, charge, local, hund_unpaired(local), 0)
        register_transition_metallic_state(member, domain_electrons)
        formula_left = member if metal_coefficient == 1 else f"{metal_coefficient} {member}"
        oxygen_left = "O2" if oxygen_coefficient == 1 else f"{oxygen_coefficient} O2"
        formula_right = formula if product_coefficient == 1 else f"{product_coefficient} {formula}"
        equation = f"{formula_left} + {oxygen_left} -> {formula_right}"
        slug = transition_slug(member, oxidations, oxygen_per_product)
        transition_experiences.append(
            [
                slug, f"{member}AndOxygenTo{name}", str(metal_coefficient), source_id, member, "metallic",
                str(product_coefficient), product_id, formula, "ionic", equation, f"Rules.{name}",
                TRANSITION_ATOMIC_NUMBERS[member],
            ]
        )

    mapping = []
    rewrite = []
    for m in range(1, metal_coefficient + 1):
        unit = (m - 1) // metal_per_product + 1
        slot = (m - 1) % metal_per_product + 1
        mapping.append(mapping_entry(f"subject[{m}].metal", f"oxide[{unit}].metal{slot}.metal"))
        rewrite.append(
            {
                "kind": "release_metallic",
                "premise_ids": all_premises(),
                "site": f"subject[{m}].metal",
                "domain": f"subject[{m}].metallic",
                "allocation": "retain_electron",
                "before": {"site": [domain_electrons, 0, 0], "domain_electrons": domain_electrons},
                "after": {"site": [0, domain_electrons, domain_electrons], "domain_electrons": 0},
            }
        )
    oxygen_atoms = []
    for o in range(1, oxygen_coefficient + 1):
        rewrite.append(cleave_oxygen(f"oxygen[{o}]"))
        oxygen_atoms.extend([f"oxygen[{o}].o1", f"oxygen[{o}].o2"])
    for i, oxygen_atom in enumerate(oxygen_atoms):
        unit = i // oxygen_per_product + 1
        slot = i % oxygen_per_product + 1
        mapping.append(mapping_entry(oxygen_atom, f"oxide[{unit}].oxide{slot}.o"))
    donors = [
        {"ref": f"subject[{m}].metal", "target": expanded_oxidations[m - 1], "sent": 0}
        for m in range(1, metal_coefficient + 1)
    ]
    di = 0
    for oi, oxygen_atom in enumerate(oxygen_atoms):
        received = 0
        while received < 2:
            donor = donors[di]
            remaining = donor["target"] - donor["sent"]
            count = min(remaining, 2 - received)
            before_sent = donor["sent"]
            after_sent = before_sent + count
            rewrite.append(
                {
                    "kind": "transfer_electron",
                    "premise_ids": all_premises(),
                    "count": count,
                    "donor": donor["ref"],
                    "acceptor": oxygen_atom,
                    "before": {
                        "donor": [before_sent, domain_electrons - before_sent, domain_electrons - before_sent],
                        "acceptor": [-received, 6 + received, 2 - received],
                    },
                    "after": {
                        "donor": [after_sent, domain_electrons - after_sent, domain_electrons - after_sent],
                        "acceptor": [-(received + count), 6 + received + count, 2 - received - count],
                    },
                }
            )
            donor["sent"] = after_sent
            received += count
            if donor["sent"] == donor["target"]:
                local = domain_electrons - donor["target"]
                hund = hund_unpaired(local)
                if hund != local:
                    rewrite.append(
                        {
                            "kind": "reconfigure_electrons",
                            "premise_ids": all_premises(),
                            "atom": donor["ref"],
                            "before": [donor["target"], local, local],
                            "after": [donor["target"], local, hund],
                        }
                    )
                di += 1
    for u in range(1, product_coefficient + 1):
        atoms = []
        groups = []
        charges = []
        for m in range(1, metal_per_product + 1):
            idx = (u - 1) * metal_per_product + m
            charge = expanded_oxidations[idx - 1]
            atoms.append(f"subject[{idx}].metal")
            groups.append([f"subject[{idx}].metal"])
            charges.append(charge)
        for o in range(1, oxygen_per_product + 1):
            idx = (u - 1) * oxygen_per_product + o
            oxygen_atom = oxygen_atoms[idx - 1]
            atoms.append(oxygen_atom)
            groups.append([oxygen_atom])
            charges.append(-2)
        rewrite.append(
            {
                "kind": "associate_ionic",
                "premise_ids": all_premises(),
                "label": f"ionic.product{u}",
                "components": groups,
                "component_charges": charges,
            }
        )
        rewrite.append(assignment(atoms, f"oxide[{u}]"))
    rules.append(
        new_base_rule(
            f"Rules.{name}",
            category,
            {
                "subject": role("reactant", "metallic", metal_coefficient),
                "oxygen": role("reactant", "molecular", oxygen_coefficient),
                "oxide": role("product", "ionic", product_coefficient),
            },
            {"subject": template_ref(metal_template), "oxygen": exact("Oxygen")},
            {"oxide": template_ref(product_template)},
            {"subject": metal_pattern, "oxygen": "Patterns.Oxygen"},
            mapping,
            rewrite,
        )
    )


def add_transition_covalent_oxide_family(name, members, domain_electrons, metal_per_product, oxygen_per_product):
    category = f"Categories.{name}Element"
    metal_template = f"Templates.{name}Metal"
    product_template = f"Templates.{name}Oxide"
    metal_pattern = f"Patterns.{name}Metal"
    product_coefficient = 1 if oxygen_per_product % 2 == 0 else 2
    metal_coefficient = metal_per_product * product_coefficient
    oxygen_coefficient = (oxygen_per_product * product_coefficient) // 2
    add_category(category, members)
    templates.append(
        {
            "representation": "metallic",
            "id": metal_template,
            "parameters": {"member": parameter(category)},
            "sites": [atom("metal", parameter_value(), domain_electrons, 0, 0)],
            "domains": [
                {"label": "metallic", "sites": ["metal"], "delocalized_electrons": domain_electrons}
            ],
            "premise_ids": all_premises(),
        }
    )
    patterns.append(
        {
            "id": metal_pattern,
            "variables": {"metal": {"atom": {"element": parameter_value()}}},
            "relationships": [
                {
                    "kind": "metallic_domain",
                    "domain": "metallic",
                    "sites": ["metal"],
                    "delocalized_electrons": domain_electrons,
                }
            ],
            "premise_ids": all_premises(),
        }
    )
    product_atoms = [atom(f"metal{m}", parameter_value(), 0, 0, 0) for m in range(1, metal_per_product + 1)]
    product_atoms.extend(atom(f"o{o}", "O", 0, 4, 0) for o in range(1, oxygen_per_product + 1))
    product_bonds = []
    if metal_per_product == 1:
        for o in range(1, oxygen_per_product + 1):
            product_bonds.append(bond("metal1", f"o{o}", "double"))
    else:
        for m in range(1, metal_per_product + 1):
            for slot in range(1, 4):
                o = (m - 1) * 3 + slot
                product_bonds.append(bond(f"metal{m}", f"o{o}", "double"))
        bridge = oxygen_per_product
        product_bonds.append(bond("metal1", f"o{bridge}", "single"))
        product_bonds.append(bond("metal2", f"o{bridge}", "single"))
    templates.append(
        {
            "representation": "molecular",
            "id": product_template,
            "parameters": {"member": parameter(category)},
            "atoms": product_atoms,
            "bonds": product_bonds,
            "groups": [],
            "premise_ids": all_premises(),
        }
    )
    for member in members:
        source_id = f"{member}{name}MetalForOxygen"
        product_id = f"{member}{name}Oxide"
        formula = oxide_formula(member, metal_per_product, oxygen_per_product)
        applications.append(
            {
                "id": source_id,
                "template": metal_template,
                "arguments": {"member": member},
                "formula": member,
                "premise_ids": all_premises(),
            }
        )
        applications.append(
            {
                "id": product_id,
                "template": product_template,
                "arguments": {"member": member},
                "formula": formula,
                "premise_ids": all_premises(),
            }
        )
        add_state(member, domain_electrons, 0, 0, 0)
        remaining = domain_electrons
        while remaining >= 1:
            add_state(member, 0, remaining, remaining, domain_electrons - remaining)
            remaining -= 2
        add_state(member, 0, 0, 0, domain_electrons)
        register_transition_metallic_state(member, domain_electrons)
        oxidations = [(2 * oxygen_per_product) // metal_per_product] * metal_per_product
        slug = transition_slug(member, oxidations, oxygen_per_product)
        left = member if metal_coefficient == 1 else f"{metal_coefficient} {member}"
        o_left = "O2" if oxygen_coefficient == 1 else f"{oxygen_coefficient} O2"
        right = formula if product_coefficient == 1 else f"{product_coefficient} {formula}"
        equation = f"{left} + {o_left} -> {right}"
        transition_experiences.append(
            [
                slug, f"{member}AndOxygenTo{name}", str(metal_coefficient), source_id, member, "metallic",
                str(product_coefficient), product_id, formula, "molecular", equation, f"Rules.{name}",
                TRANSITION_ATOMIC_NUMBERS[member],
            ]
        )
    mapping = []
    rewrite = []
    for m in range(1, metal_coefficient + 1):
        unit = (m - 1) // metal_per_product + 1
        slot = (m - 1) % metal_per_product + 1
        mapping.append(mapping_entry(f"subject[{m}].metal", f"oxide[{unit}].metal{slot}"))
        rewrite.append(
            {
                "kind": "release_metallic",
                "premise_ids": all_premises(),
                "site": f"subject[{m}].metal",
                "domain": f"subject[{m}].metallic",
                "allocation": "retain_electron",
                "before": {"site": [domain_electrons, 0, 0], "domain_electrons": domain_electrons},
                "after": {"site": [0, domain_electrons, domain_electrons], "domain_electrons": 0},
            }
        )
    oxygen_atoms = []
    for o in range(1, oxygen_coefficient + 1):
        rewrite.append(cleave_oxygen(f"oxygen[{o}]"))
        oxygen_atoms.extend([f"oxygen[{o}].o1", f"oxygen[{o}].o2"])
    for i, oxygen_atom in enumerate(oxygen_atoms):
        unit = i // oxygen_per_product + 1
        slot = i % oxygen_per_product + 1
        mapping.append(mapping_entry(oxygen_atom, f"oxide[{unit}].o{slot}"))
    for u in range(1, product_coefficient + 1):
        metal_refs = [
            f"subject[{(u - 1) * metal_per_product + m}].metal" for m in range(1, metal_per_product + 1)
        ]
        unit_oxygen = oxygen_atoms[(u - 1) * oxygen_per_product : u * oxygen_per_product]
        if metal_per_product == 1:
            remaining = domain_electrons
            for oxygen_atom in unit_oxygen:
                rewrite.append(
                    {
                        "kind": "form_covalent",
                        "premise_ids": all_premises(),
                        "edge": [metal_refs[0], oxygen_atom, "double"],
                        "electron_contribution": {"left": 2, "right": 2},
                        "before": binary_state([0, remaining, remaining], [0, 6, 2]),
                        "after": binary_state([0, remaining - 2, remaining - 2], [0, 4, 0]),
                    }
                )
                remaining -= 2
        else:
            for m in range(2):
                remaining = domain_electrons
                for slot in range(3):
                    oxygen_atom = unit_oxygen[m * 3 + slot]
                    rewrite.append(
                        {
                            "kind": "form_covalent",
                            "premise_ids": all_premises(),
                            "edge": [metal_refs[m], oxygen_atom, "double"],
                            "electron_contribution": {"left": 2, "right": 2},
                            "before": binary_state([0, remaining, remaining], [0, 6, 2]),
                            "after": binary_state([0, remaining - 2, remaining - 2], [0, 4, 0]),
                        }
                    )
                    remaining -= 2
            bridge = unit_oxygen[oxygen_per_product - 1]
            rewrite.append(
                {
                    "kind": "form_covalent",
                    "premise_ids": all_premises(),
                    "edge": [metal_refs[0], bridge, "single"],
                    "electron_contribution": {"left": 1, "right": 1},
                    "before": binary_state([0, 1, 1], [0, 6, 2]),
                    "after": binary_state([0, 0, 0], [0, 5, 1]),
                }
            )
            rewrite.append(
                {
                    "kind": "form_covalent",
                    "premise_ids": all_premises(),
                    "edge": [metal_refs[1], bridge, "single"],
                    "electron_contribution": {"left": 1, "right": 1},
                    "before": binary_state([0, 1, 1], [0, 5, 1]),
                    "after": binary_state([0, 0, 0], [0, 4, 0]),
                }
            )
        rewrite.append(assignment(metal_refs + unit_oxygen, f"oxide[{u}]"))
    rules.append(
        new_base_rule(
            f"Rules.{name}",
            category,
            {
                "subject": role("reactant", "metallic", metal_coefficient),
                "oxygen": role("reactant", "molecular", oxygen_coefficient),
                "oxide": role("product", "molecular", product_coefficient),
            },
            {"subject": template_ref(metal_template), "oxygen": exact("Oxygen")},
            {"oxide": template_ref(product_template)},
            {"subject": metal_pattern, "oxygen": "Patterns.Oxygen"},
            mapping,
            rewrite,
        )
    )


# Each call is a family: members share the same metallic electron pool and the
# same oxide stoichiometry/oxidation-state process. Multiple calls for a member
# become selectable product outcomes in the app.
add_transition_oxide_family("TransitionG3Sesquioxide", ["Sc", "Y"], 3, [3, 3], 3)
add_transition_oxide_family("TransitionG4Monoxide", ["Ti"], 4, [2], 1)
add_transition_oxide_family("TransitionG4Sesquioxide", ["Ti"], 4, [3, 3], 3)
add_transition_oxide_family("TransitionG4Dioxide", ["Ti", "Zr", "Hf"], 4, [4], 2)
add_transition_oxide_family("TransitionG5Monoxide", ["V"], 5, [2], 1)
add_transition_oxide_family("TransitionG5Sesquioxide", ["V"], 5, [3, 3], 3)
add_transition_oxide_family("TransitionG5Dioxide", ["V", "Nb"], 5, [4], 2)
add_transition_oxide_family("TransitionG5Pentoxide", ["V", "Nb", "Ta"], 5, [5, 5], 5)
add_transition_oxide_family("TransitionG6Monoxide", ["Cr"], 6, [2], 1)
add_transition_oxide_family("TransitionG6Sesquioxide", ["Cr"], 6, [3, 3], 3)
add_transition_oxide_family("TransitionG6Dioxide", ["Cr", "Mo", "W"], 6, [4], 2)
add_transition_oxide_family("TransitionG7Monoxide", ["Mn"], 7, [2], 1)
add_transition_oxide_family("TransitionG7Sesquioxide", ["Mn"], 7, [3, 3], 3)
add_transition_oxide_family("TransitionG7Dioxide", ["Mn", "Tc", "Re"], 7, [4], 2)
add_transition_oxide_family("TransitionG7MixedOxide", ["Mn"], 7, [2, 3, 3], 4)
add_transition_oxide_family("TransitionG8Monoxide", ["Fe"], 8, [2], 1)
add_transition_oxide_family("TransitionG8Sesquioxide", ["Fe"], 8, [3, 3], 3)
add_transition_oxide_family("TransitionG8MixedOxide", ["Fe"], 8, [2, 3, 3], 4)
add_transition_oxide_family("TransitionG8Dioxide", ["Ru", "Os"], 8, [4], 2)
add_transition_oxide_family("TransitionG9Monoxide", ["Co"], 9, [2], 1)
add_transition_oxide_family("TransitionG9Sesquioxide", ["Co", "Rh"], 9, [3, 3], 3)
add_transition_oxide_family("TransitionG9MixedOxide", ["Co"], 9, [2, 3, 3], 4)
add_transition_oxide_family("TransitionG9Dioxide", ["Rh", "Ir"], 9, [4], 2)
add_transition_oxide_family("TransitionG10Monoxide", ["Ni", "Pd"], 10, [2], 1)
add_transition_oxide_family("TransitionG11Hemioxide", ["Cu"], 11, [1, 1], 1)
add_transition_oxide_family("TransitionG11Monoxide", ["Cu"], 11, [2], 1)
add_transition_oxide_family("TransitionG12Monoxide", ["Zn", "Cd", "Hg"], 12, [2], 1)
add_transition_covalent_oxide_family("TransitionG6Trioxide", ["Cr", "Mo", "W"], 6, 1, 3)
add_transition_covalent_oxide_family("TransitionG7Heptoxide", ["Mn", "Tc", "Re"], 7, 2, 7)
add_transition_covalent_oxide_family("TransitionG8Tetroxide", ["Ru", "Os"], 8, 1, 4)

# Boron oxide is a five-atom representative network fragment with three bridging oxygens.
add_category("Categories.BoronOxideElement", ["B"])
templates.append(
    {
        "representation": "molecular",
        "id": "Templates.BoronElement",
        "parameters": {"member": parameter("Categories.BoronOxideElement")},
        "atoms": [atom("b", parameter_value(), 0, 3, 3)],
        "bonds": [],
        "groups": [],
        "premise_ids": all_premises(),
    }
)
templates.append(
    {
        "representation": "molecular",
        "id": "Templates.BoronOxide",
        "parameters": {"member": parameter("Categories.BoronOxideElement")},
        "atoms": [
            atom("b1", parameter_value(), 0, 0, 0),
            atom("b2", parameter_value(), 0, 0, 0),
            atom("o1", "O", 0, 4, 0),
            atom("o2", "O", 0, 4, 0),
            atom("o3", "O", 0, 4, 0),
        ],
        "bonds": [
            bond("b1", "o1", "single"),
            bond("b1", "o2", "single"),
            bond("b1", "o3", "single"),
            bond("b2", "o1", "single"),
            bond("b2", "o2", "single"),
            bond("b2", "o3", "single"),
        ],
        "groups": [],
        "premise_ids": all_premises(),
    }
)
patterns.append(
    {
        "id": "Patterns.BoronElement",
        "variables": {"b": {"atom": {"element": parameter_value()}}},
        "relationships": [],
        "premise_ids": all_premises(),
    }
)
applications.append(
    {
        "id": "BForOxygen",
        "template": "Templates.BoronElement",
        "arguments": {"member": "B"},
        "formula": "B",
        "premise_ids": all_premises(),
    }
)
applications.append(
    {
        "id": "BBoronOxide",
        "template": "Templates.BoronOxide",
        "arguments": {"member": "B"},
        "formula": "B2O3",
        "premise_ids": all_premises(),
    }
)
mapping = []
rewrite = []
for b in range(1, 5):
    unit = (b - 1) // 2 + 1
    slot = (b - 1) % 2 + 1
    mapping.append(mapping_entry(f"subject[{b}].b", f"oxide[{unit}].b{slot}"))
oxygen_atoms = []
for o in range(1, 4):
    rewrite.append(cleave_oxygen(f"oxygen[{o}]"))
    oxygen_atoms.extend([f"oxygen[{o}].o1", f"oxygen[{o}].o2"])
for i in range(6):
    unit = i // 3 + 1
    slot = i % 3 + 1
    mapping.append(mapping_entry(oxygen_atoms[i], f"oxide[{unit}].o{slot}"))
b_state = {b: 3 for b in range(1, 5)}
o_state = {o: 2 for o in range(6)}
for u in range(1, 3):
    for slot in range(1, 3):
        for os_index in range(1, 4):
            bi = (u - 1) * 2 + slot
            oi = (u - 1) * 3 + os_index
            bb = b_state[bi]
            ob = o_state[oi - 1]
            rewrite.append(
                {
                    "kind": "form_covalent",
                    "premise_ids": all_premises(),
                    "edge": [f"subject[{bi}].b", oxygen_atoms[oi - 1], "single"],
                    "electron_contribution": {"left": 1, "right": 1},
                    "before": binary_state([0, bb, bb], [0, 4 + ob, ob]),
                    "after": binary_state([0, bb - 1, bb - 1], [0, 3 + ob, ob - 1]),
                }
            )
            b_state[bi] -= 1
            o_state[oi - 1] -= 1
    atoms = [f"subject[{(u - 1) * 2 + 1}].b", f"subject[{(u - 1) * 2 + 2}].b"] + oxygen_atoms[
        (u - 1) * 3 : (u - 1) * 3 + 3
    ]
    rewrite.append(assignment(atoms, f"oxide[{u}]"))
rules.append(
    new_base_rule(
        "Rules.BoronOxide",
        "Categories.BoronOxideElement",
        {
            "subject": role("reactant", "molecular", 4),
            "oxygen": role("reactant", "molecular", 3),
            "oxide": role("product", "molecular", 2),
        },
        {"subject": template_ref("Templates.BoronElement"), "oxygen": exact("Oxygen")},
        {"oxide": template_ref("Templates.BoronOxide")},
        {"subject": "Patterns.BoronElement", "oxygen": "Patterns.Oxygen"},
        mapping,
        rewrite,
    )
)

# Hydrogen oxidation reuses the already authored Hydrogen and Water structures.
add_category("Categories.HydrogenOxideElement", ["H"])
templates.append(
    {
        "representation": "molecular",
        "id": "Templates.HydrogenForOxygen",
        "parameters": {"member": parameter("Categories.HydrogenOxideElement")},
        "atoms": [atom("h1", parameter_value(), 0, 0, 0), atom("h2", parameter_value(), 0, 0, 0)],
        "bonds": [bond("h1", "h2", "single")],
        "groups": [],
        "premise_ids": all_premises(),
    }
)
applications.append(
    {
        "id": "HydrogenForOxygen",
        "template": "Templates.HydrogenForOxygen",
        "arguments": {"member": "H"},
        "formula": "H2",
        "premise_ids": all_premises(),
    }
)
patterns.append(
    {
        "id": "Patterns.Hydrogen",
        "variables": {"h1": {"atom": {"element": "H"}}, "h2": {"atom": {"element": "H"}}},
        "relationships": [{"kind": "covalent", "bond": "hh", "left": "h1", "right": "h2", "order": "single"}],
        "premise_ids": all_premises(),
    }
)
mapping = [
    mapping_entry("subject[1].h1", "oxide[1].h1"),
    mapping_entry("subject[1].h2", "oxide[1].h2"),
    mapping_entry("oxygen[1].o1", "oxide[1].o"),
    mapping_entry("subject[2].h1", "oxide[2].h1"),
    mapping_entry("subject[2].h2", "oxide[2].h2"),
    mapping_entry("oxygen[1].o2", "oxide[2].o"),
]
rewrite = [cleave_oxygen("oxygen[1]")]
for h in range(1, 3):
    rewrite.append(
        {
            "kind": "cleave_covalent",
            "premise_ids": all_premises(),
            "edge": [f"subject[{h}].h1", f"subject[{h}].h2", "single"],
            "allocation": "homolytic",
            "before": binary_state([0, 0, 0], [0, 0, 0]),
            "after": binary_state([0, 1, 1], [0, 1, 1]),
        }
    )
for u in range(1, 3):
    oxygen_ref = f"oxygen[1].o{u}"
    for h in range(1, 3):
        before = [0, 6, 2] if h == 1 else [0, 5, 1]
        after = [0, 5, 1] if h == 1 else [0, 4, 0]
        rewrite.append(
            {
                "kind": "form_covalent",
                "premise_ids": all_premises(),
                "edge": [oxygen_ref, f"subject[{u}].h{h}", "single"],
                "electron_contribution": {"left": 1, "right": 1},
                "before": binary_state(before, [0, 1, 1]),
                "after": binary_state(after, [0, 0, 0]),
            }
        )
    rewrite.append(assignment([oxygen_ref, f"subject[{u}].h1", f"subject[{u}].h2"], f"oxide[{u}]"))
hydrogen_rule = new_base_rule(
    "Rules.HydrogenOxide",
    "Categories.HydrogenOxideElement",
    {
        "subject": role("reactant", "molecular", 2),
        "oxygen": role("reactant", "molecular", 1),
        "oxide": role("product", "molecular", 2),
    },
    {"subject": template_ref("Templates.HydrogenForOxygen"), "oxygen": exact("Oxygen")},
    {"oxide": exact("Water")},
    {"subject": "Patterns.Hydrogen", "oxygen": "Patterns.Oxygen"},
    mapping,
    rewrite,
)
hydrogen_rule["premise_ids"] = hydrogen_rule["premise_ids"] + ["premise.structure.water"]
for item in hydrogen_rule["cases"][0]["correspondence"]:
    item["premise_ids"] = item["premise_ids"] + ["premise.structure.water"]
for item in hydrogen_rule["cases"][0]["rewrite"]:
    item["premise_ids"] = item["premise_ids"] + ["premise.structure.water"]
rules.append(hydrogen_rule)

# Phosphorus(V) oxide uses one explicit P4O10 molecule; six oxygens bridge P atoms
# and four terminal P=O bonds complete the representative Lewis structure.
add_state("P", 0, 2, 0, 3)
add_state("P", 0, 2, 2, 3)
add_state("P", 0, 3, 1, 2)
add_state("P", 0, 4, 2, 1)
add_state("P", 0, 5, 3, 0)
add_state("P", 0, 0, 0, 5)
add_category("Categories.PhosphorusOxideElement", ["P"])
p_atoms = [atom(f"p{p}", parameter_value(), 0, 2, 0) for p in range(1, 5)]
P_EDGES = [(1, 2), (1, 3), (1, 4), (2, 3), (2, 4), (3, 4)]
p_bonds = [bond(f"p{edge[0]}", f"p{edge[1]}", "single") for edge in P_EDGES]
templates.append(
    {
        "representation": "molecular",
        "id": "Templates.Phosphorus4",
        "parameters": {"member": parameter("Categories.PhosphorusOxideElement")},
        "atoms": p_atoms,
        "bonds": p_bonds,
        "groups": [],
        "premise_ids": all_premises(),
    }
)
po_atoms = [atom(f"p{p}", parameter_value(), 0, 0, 0) for p in range(1, 5)]
po_atoms.extend(atom(f"o{o}", "O", 0, 4, 0) for o in range(1, 11))
po_bonds = []
for i in range(6):
    edge = P_EDGES[i]
    o = i + 1
    po_bonds.append(bond(f"p{edge[0]}", f"o{o}", "single"))
    po_bonds.append(bond(f"p{edge[1]}", f"o{o}", "single"))
for p in range(1, 5):
    po_bonds.append(bond(f"p{p}", f"o{p + 6}", "double"))
templates.append(
    {
        "representation": "molecular",
        "id": "Templates.Phosphorus5Oxide",
        "parameters": {"member": parameter("Categories.PhosphorusOxideElement")},
        "atoms": po_atoms,
        "bonds": po_bonds,
        "groups": [],
        "premise_ids": all_premises(),
    }
)
applications.append(
    {
        "id": "Phosphorus4ForOxygen",
        "template": "Templates.Phosphorus4",
        "arguments": {"member": "P"},
        "formula": "P4",
        "premise_ids": all_premises(),
    }
)
applications.append(
    {
        "id": "Phosphorus5Oxide",
        "template": "Templates.Phosphorus5Oxide",
        "arguments": {"member": "P"},
        "formula": "P4O10",
        "premise_ids": all_premises(),
    }
)
p_variables = {f"p{p}": {"atom": {"element": "P"}} for p in range(1, 5)}
p_relationships = [
    {"kind": "covalent", "bond": f"pp{i + 1}", "left": f"p{P_EDGES[i][0]}", "right": f"p{P_EDGES[i][1]}", "order": "single"}
    for i in range(6)
]
patterns.append(
    {"id": "Patterns.Phosphorus4", "variables": p_variables, "relationships": p_relationships, "premise_ids": all_premises()}
)
mapping = [mapping_entry(f"subject[1].p{p}", f"oxide[1].p{p}") for p in range(1, 5)]
oxygen_atoms = []
for o in range(1, 6):
    oxygen_atoms.extend([f"oxygen[{o}].o1", f"oxygen[{o}].o2"])
for o in range(1, 11):
    mapping.append(mapping_entry(oxygen_atoms[o - 1], f"oxide[1].o{o}"))
rewrite = []
p_nb = {p: 2 for p in range(1, 5)}
p_unpaired = {p: 0 for p in range(1, 5)}
for edge in P_EDGES:
    l, r = edge
    rewrite.append(
        {
            "kind": "cleave_covalent",
            "premise_ids": all_premises(),
            "edge": [f"subject[1].p{l}", f"subject[1].p{r}", "single"],
            "allocation": "homolytic",
            "before": binary_state([0, p_nb[l], p_unpaired[l]], [0, p_nb[r], p_unpaired[r]]),
            "after": binary_state([0, p_nb[l] + 1, p_unpaired[l] + 1], [0, p_nb[r] + 1, p_unpaired[r] + 1]),
        }
    )
    p_nb[l] += 1
    p_nb[r] += 1
    p_unpaired[l] += 1
    p_unpaired[r] += 1
for o in range(1, 6):
    rewrite.append(cleave_oxygen(f"oxygen[{o}]"))
oxygen_use = {o: 2 for o in range(1, 11)}
for i in range(6):
    edge = P_EDGES[i]
    o = i + 1
    for p in edge:
        pb = p_unpaired[p]
        ob = oxygen_use[o]
        rewrite.append(
            {
                "kind": "form_covalent",
                "premise_ids": all_premises(),
                "edge": [f"subject[1].p{p}", oxygen_atoms[o - 1], "single"],
                "electron_contribution": {"left": 1, "right": 1},
                "before": binary_state([0, p_nb[p], pb], [0, 4 + ob, ob]),
                "after": binary_state([0, p_nb[p] - 1, pb - 1], [0, 3 + ob, ob - 1]),
            }
        )
        p_nb[p] -= 1
        p_unpaired[p] -= 1
        oxygen_use[o] -= 1
for p in range(1, 5):
    o = p + 6
    rewrite.append(
        {
            "kind": "reconfigure_electrons",
            "premise_ids": all_premises(),
            "atom": f"subject[1].p{p}",
            "before": [0, 2, 0],
            "after": [0, 2, 2],
        }
    )
    rewrite.append(
        {
            "kind": "form_covalent",
            "premise_ids": all_premises(),
            "edge": [f"subject[1].p{p}", oxygen_atoms[o - 1], "double"],
            "electron_contribution": {"left": 2, "right": 2},
            "before": binary_state([0, 2, 2], [0, 6, 2]),
            "after": binary_state([0, 0, 0], [0, 4, 0]),
        }
    )
rewrite.append(
    assignment(["subject[1].p1", "subject[1].p2", "subject[1].p3", "subject[1].p4"] + oxygen_atoms, "oxide[1]")
)
rules.append(
    new_base_rule(
        "Rules.Phosphorus5Oxide",
        "Categories.PhosphorusOxideElement",
        {
            "subject": role("reactant", "molecular", 1),
            "oxygen": role("reactant", "molecular", 5),
            "oxide": role("product", "molecular", 1),
        },
        {"subject": template_ref("Templates.Phosphorus4"), "oxygen": exact("Oxygen")},
        {"oxide": template_ref("Templates.Phosphorus5Oxide")},
        {"subject": "Patterns.Phosphorus4", "oxygen": "Patterns.Oxygen"},
        mapping,
        rewrite,
    )
)

# Fixed-charge main-group ion pairs are generated from charge families.  The
# code below is deliberately independent of oxide identity: elemental source
# topology, charge balancing, electron transfer and ionic association are data.
ion_pair_experiences: list[dict] = []
FIXED_CATIONS = {1: ["Li", "Na", "K", "Rb", "Cs"], 2: ["Be", "Mg", "Ca", "Sr", "Ba"], 3: ["Al"]}
ATOMIC_NUMBERS = {
    "Li": 3, "Be": 4, "N": 7, "O": 8, "F": 9, "Na": 11, "Mg": 12, "Al": 13, "P": 15, "S": 16,
    "Cl": 17, "K": 19, "Ca": 20, "Br": 35, "Rb": 37, "Sr": 38, "I": 53, "Cs": 55, "Ba": 56,
}


def formula_for(cation, cation_count, anion, anion_count):
    c = "" if cation_count == 1 else str(cation_count)
    a = "" if anion_count == 1 else str(anion_count)
    return f"{cation}{c}{anion}{a}"


def add_fixed_cation_scaffold(charge, members):
    category = f"Categories.FixedCation{charge}"
    template = f"Templates.FixedCation{charge}Metal"
    pattern = f"Patterns.FixedCation{charge}Metal"
    categories.append(
        {
            "id": category,
            "subject": "element",
            "membership": {"kind": "explicit", "members": list(members)},
            "premise_ids": [ION_RULE_PREMISE],
        }
    )
    templates.append(
        {
            "representation": "metallic",
            "id": template,
            "parameters": {"member": parameter(category)},
            "sites": [atom("metal", parameter_value(), charge, 0, 0)],
            "domains": [{"label": "metallic", "sites": ["metal"], "delocalized_electrons": charge}],
            "premise_ids": ion_premises(),
        }
    )
    patterns.append(
        {
            "id": pattern,
            "variables": {"metal": {"atom": {"element": parameter_value()}}},
            "relationships": [
                {"kind": "metallic_domain", "domain": "metallic", "sites": ["metal"], "delocalized_electrons": charge}
            ],
            "premise_ids": ion_premises(),
        }
    )
    for member in members:
        for remaining in range(charge, -1, -1):
            add_state(member, charge - remaining, remaining, remaining, 0)
        applications.append(
            {
                "id": f"{member}FixedCation{charge}Metal",
                "template": template,
                "arguments": {"member": member},
                "formula": member,
                "premise_ids": ion_premises(),
            }
        )


for charge in (1, 2, 3):
    add_fixed_cation_scaffold(charge, FIXED_CATIONS[charge])


def add_elemental_anion(anion_id, symbol, count, neutral_nb, bonds):
    atoms = [atom(f"a{i}", symbol, 0, neutral_nb, 0) for i in range(1, count + 1)]
    bond_records = []
    relationships = []
    index = 0
    bond_sums = {i: 0 for i in range(1, count + 1)}
    for source_bond in bonds:
        index += 1
        delta = 3 if source_bond[2] == "triple" else 2 if source_bond[2] == "double" else 1
        bond_sums[source_bond[0]] += delta
        bond_sums[source_bond[1]] += delta
        bond_records.append(bond(f"a{source_bond[0]}", f"a{source_bond[1]}", source_bond[2]))
        relationships.append(
            {
                "kind": "covalent",
                "bond": f"bond{index}",
                "left": f"a{source_bond[0]}",
                "right": f"a{source_bond[1]}",
                "order": source_bond[2],
            }
        )
    for i in range(1, count + 1):
        add_state(symbol, 0, neutral_nb, 0, bond_sums[i])
    structures.append(
        {
            "representation": "molecular",
            "id": anion_id,
            "premise_id": ION_STRUCTURE_PREMISE,
            "formula": symbol if count == 1 else f"{symbol}{count}",
            "atoms": atoms,
            "bonds": bond_records,
            "groups": [],
        }
    )
    variables = {f"a{i}": {"atom": {"element": symbol}} for i in range(1, count + 1)}
    patterns.append(
        {"id": f"Patterns.{anion_id}", "variables": variables, "relationships": relationships, "premise_ids": ion_premises()}
    )


SINGLE = [(1, 2, "single")]
TRIPLE = [(1, 2, "triple")]
DOUBLE = [(1, 2, "double")]
add_elemental_anion("ElementalFluorine", "F", 2, 6, SINGLE)
add_elemental_anion("ElementalChlorine", "Cl", 2, 6, SINGLE)
add_elemental_anion("ElementalBromine", "Br", 2, 6, SINGLE)
add_elemental_anion("ElementalIodine", "I", 2, 6, SINGLE)
add_elemental_anion("ElementalNitrogen", "N", 2, 2, TRIPLE)
SULFUR_EDGES = [(i, i % 8 + 1, "single") for i in range(1, 9)]
add_elemental_anion("ElementalSulfur", "S", 8, 4, SULFUR_EDGES)
PHOSPHORUS_EDGES = [(1, 2, "single"), (1, 3, "single"), (1, 4, "single"), (2, 3, "single"), (2, 4, "single"), (3, 4, "single")]
add_elemental_anion("ElementalPhosphorus", "P", 4, 2, PHOSPHORUS_EDGES)


def new_ion_rule(rule_id, charge, roles, reactants, products, case_patterns, mapping, rewrite):
    return {
        "id": rule_id,
        "parameters": {"member": parameter(f"Categories.FixedCation{charge}")},
        "roles": roles,
        "reactants": reactants,
        "cases": [
            {
                "status": "supported",
                "id": "charge-balanced",
                "when": {"kind": "always"},
                "products": products,
                "patterns": case_patterns,
                "correspondence": mapping,
                "rewrite": rewrite,
                "observation_compatibility": [
                    {
                        "subject_role": "salt",
                        "predicate": "forms",
                        "evidence_subject": "salt",
                        "premise_id": ION_OBSERVATION_PREMISE,
                    },
                    {
                        "subject_role": "cation",
                        "predicate": "disappears",
                        "evidence_subject": "metal",
                        "premise_id": ION_OBSERVATION_PREMISE,
                    },
                ],
                "premise_ids": [ION_RULE_PREMISE],
            }
        ],
        "applicability": {
            "premise_id": ION_RULE_PREMISE,
            "request_relation": "contact",
            "required_context": "selected theoretical fixed-charge binary ionic outcome",
        },
        "model_assumptions": {"event": "representative", "sequence": "explanatory", "premise_ids": [ION_RULE_PREMISE]},
        "premise_ids": [
            "premise.elements.iupac-periodic-table",
            ION_RULE_PREMISE,
            ION_STRUCTURE_PREMISE,
            ION_VALENCE_PREMISE,
            ION_OBSERVATION_PREMISE,
        ]
        + all_premises(),
    }


def add_ion_pair_family(name, anion_symbol, anion_charge, source_id, source_count, neutral_nb, source_bonds, charges=None):
    if charges is None:
        charges = [1, 2, 3]
    anion_atomic_number = ATOMIC_NUMBERS[anion_symbol]
    for charge in charges:
        members = FIXED_CATIONS[charge]
        g = math.gcd(charge, anion_charge)
        cation_per_product = anion_charge // g
        anion_per_product = charge // g
        product_coefficient = source_count // math.gcd(source_count, anion_per_product)
        source_coefficient = (product_coefficient * anion_per_product) // source_count
        metal_coefficient = product_coefficient * cation_per_product
        category = f"Categories.FixedCation{charge}"
        metal_template = f"Templates.FixedCation{charge}Metal"
        metal_pattern = f"Patterns.FixedCation{charge}Metal"
        product_template = f"Templates.FixedCation{charge}{name}Product"
        rule_id = f"Rules.FixedCation{charge}{name}"
        components = []
        for m in range(1, cation_per_product + 1):
            components.append(
                {"label": f"cation{m}", "atoms": [atom("metal", parameter_value(), charge, 0, 0)], "bonds": [], "groups": []}
            )
        for a in range(1, anion_per_product + 1):
            components.append(
                {
                    "label": f"anion{a}",
                    "atoms": [atom("anion", anion_symbol, -1 * anion_charge, 8, 0)],
                    "bonds": [],
                    "groups": [],
                }
            )
        templates.append(
            {
                "representation": "ionic",
                "id": product_template,
                "parameters": {"member": parameter(category)},
                "components": components,
                "associations": [{"label": "ionic", "components": [component["label"] for component in components]}],
                "premise_ids": ion_premises(),
            }
        )
        for member in members:
            formula = formula_for(member, cation_per_product, anion_symbol, anion_per_product)
            product_id = f"{member}FixedCation{charge}{name}"
            applications.append(
                {
                    "id": product_id,
                    "template": product_template,
                    "arguments": {"member": member},
                    "formula": formula,
                    "premise_ids": ion_premises(),
                }
            )
            source_formula = anion_symbol if source_count == 1 else f"{anion_symbol}{source_count}"
            metal_part = "" if metal_coefficient == 1 else f"{metal_coefficient} "
            source_part = "" if source_coefficient == 1 else f"{source_coefficient} "
            product_part = "" if product_coefficient == 1 else f"{product_coefficient} "
            equation = f"{metal_part}{member} + {source_part}{source_formula} -> {product_part}{formula}"
            ion_pair_experiences.append(
                {
                    "slug": f"{member.lower()}-{anion_symbol.lower()}",
                    "reaction": f"{member}And{name}",
                    "atomic_number": ATOMIC_NUMBERS[member],
                    "co_atoms": [anion_atomic_number] * source_count,
                    "subject_coefficient": metal_coefficient,
                    "subject_structure": f"{member}FixedCation{charge}Metal",
                    "subject_formula": member,
                    "anion_coefficient": source_coefficient,
                    "anion_structure": source_id,
                    "anion_formula": source_formula,
                    "product_coefficient": product_coefficient,
                    "product_structure": product_id,
                    "product_formula": formula,
                    "equation": equation,
                    "rule": rule_id,
                }
            )
        mapping = []
        rewrite = []
        for m in range(1, metal_coefficient + 1):
            unit = (m - 1) // cation_per_product + 1
            slot = (m - 1) % cation_per_product + 1
            mapping.append(
                {"reactant": f"cation[{m}].metal", "product": f"salt[{unit}].cation{slot}.metal", "premise_ids": ion_premises()}
            )
            rewrite.append(
                {
                    "kind": "release_metallic",
                    "premise_ids": ion_premises(),
                    "site": f"cation[{m}].metal",
                    "domain": f"cation[{m}].metallic",
                    "allocation": "retain_electron",
                    "before": {"site": [charge, 0, 0], "domain_electrons": charge},
                    "after": {"site": [0, charge, charge], "domain_electrons": 0},
                }
            )
        anion_atoms = []
        for s in range(1, source_coefficient + 1):
            for a in range(1, source_count + 1):
                label = f"o{a}" if source_id == "Oxygen" else f"a{a}"
                anion_atoms.append(f"anion[{s}].{label}")
        for s in range(1, source_coefficient + 1):
            nb = {a: neutral_nb for a in range(1, source_count + 1)}
            unpaired = {a: 0 for a in range(1, source_count + 1)}
            bond_sum = {a: 0 for a in range(1, source_count + 1)}
            for source_bond in source_bonds:
                delta = 3 if source_bond[2] == "triple" else 2 if source_bond[2] == "double" else 1
                bond_sum[source_bond[0]] += delta
                bond_sum[source_bond[1]] += delta
            for source_bond in source_bonds:
                l, r, order = source_bond
                left_label = f"o{l}" if source_id == "Oxygen" else f"a{l}"
                right_label = f"o{r}" if source_id == "Oxygen" else f"a{r}"
                delta = 3 if order == "triple" else 2 if order == "double" else 1
                add_state(anion_symbol, 0, nb[l], unpaired[l], bond_sum[l])
                add_state(anion_symbol, 0, nb[r], unpaired[r], bond_sum[r])
                rewrite.append(
                    {
                        "kind": "cleave_covalent",
                        "premise_ids": ion_premises(),
                        "edge": [f"anion[{s}].{left_label}", f"anion[{s}].{right_label}", order],
                        "allocation": "homolytic",
                        "before": binary_state([0, nb[l], unpaired[l]], [0, nb[r], unpaired[r]]),
                        "after": binary_state([0, nb[l] + delta, unpaired[l] + delta], [0, nb[r] + delta, unpaired[r] + delta]),
                    }
                )
                nb[l] += delta
                nb[r] += delta
                unpaired[l] += delta
                unpaired[r] += delta
                bond_sum[l] -= delta
                bond_sum[r] -= delta
                add_state(anion_symbol, 0, nb[l], unpaired[l], bond_sum[l])
                add_state(anion_symbol, 0, nb[r], unpaired[r], bond_sum[r])
        for accepted in range(anion_charge + 1):
            add_state(anion_symbol, -1 * accepted, 8 - anion_charge + accepted, anion_charge - accepted, 0)
        for i, anion_atom in enumerate(anion_atoms):
            unit = i // anion_per_product + 1
            slot = i % anion_per_product + 1
            mapping.append({"reactant": anion_atom, "product": f"salt[{unit}].anion{slot}.anion", "premise_ids": ion_premises()})
        donor_remaining = [charge] * metal_coefficient
        accept_remaining = [anion_charge] * len(anion_atoms)
        di = 0
        ai = 0
        while di < len(donor_remaining) and ai < len(accept_remaining):
            count = min(donor_remaining[di], accept_remaining[ai])
            db = donor_remaining[di]
            ab = anion_charge - accept_remaining[ai]
            da = db - count
            aa = ab + count
            rewrite.append(
                {
                    "kind": "transfer_electron",
                    "premise_ids": ion_premises(),
                    "count": count,
                    "donor": f"cation[{di + 1}].metal",
                    "acceptor": anion_atoms[ai],
                    "before": {
                        "donor": [charge - db, db, db],
                        "acceptor": [-1 * ab, 8 - anion_charge + ab, anion_charge - ab],
                    },
                    "after": {
                        "donor": [charge - da, da, da],
                        "acceptor": [-1 * aa, 8 - anion_charge + aa, anion_charge - aa],
                    },
                }
            )
            donor_remaining[di] -= count
            accept_remaining[ai] -= count
            if donor_remaining[di] == 0:
                di += 1
            if accept_remaining[ai] == 0:
                ai += 1
        for unit in range(1, product_coefficient + 1):
            atoms = []
            groups = []
            component_charges = []
            for m in range(1, cation_per_product + 1):
                idx = (unit - 1) * cation_per_product + m
                atoms.append(f"cation[{idx}].metal")
                groups.append([f"cation[{idx}].metal"])
                component_charges.append(charge)
            for a in range(1, anion_per_product + 1):
                idx = (unit - 1) * anion_per_product + a
                atoms.append(anion_atoms[idx - 1])
                groups.append([anion_atoms[idx - 1]])
                component_charges.append(-1 * anion_charge)
            rewrite.append(
                {
                    "kind": "associate_ionic",
                    "premise_ids": ion_premises(),
                    "label": f"ionic.product{unit}",
                    "components": groups,
                    "component_charges": component_charges,
                }
            )
            rewrite.append(
                {"kind": "assign_product", "premise_ids": ion_premises(), "atoms": atoms, "product": f"salt[{unit}]"}
            )
        roles = {
            "cation": role("reactant", "metallic", metal_coefficient),
            "anion": role("reactant", "molecular", source_coefficient),
            "salt": role("product", "ionic", product_coefficient),
        }
        rules.append(
            new_ion_rule(
                rule_id,
                charge,
                roles,
                {"cation": template_ref(metal_template), "anion": exact(source_id)},
                {"salt": template_ref(product_template)},
                {"cation": metal_pattern, "anion": f"Patterns.{source_id}"},
                mapping,
                rewrite,
            )
        )


for anion in [
    ("Fluoride", "F", "ElementalFluorine", 2, 6, SINGLE),
    ("Chloride", "Cl", "ElementalChlorine", 2, 6, SINGLE),
    ("Bromide", "Br", "ElementalBromine", 2, 6, SINGLE),
    ("Iodide", "I", "ElementalIodine", 2, 6, SINGLE),
]:
    add_ion_pair_family(anion[0], anion[1], 1, anion[2], anion[3], anion[4], anion[5])
add_ion_pair_family("Sulfide", "S", 2, "ElementalSulfur", 8, 4, SULFUR_EDGES)
add_ion_pair_family("Nitride", "N", 3, "ElementalNitrogen", 2, 2, TRIPLE)
add_ion_pair_family("Phosphide", "P", 3, "ElementalPhosphorus", 4, 2, PHOSPHORUS_EDGES)
# Normal oxides already exist for Li, all +2 metals and Al.  This adds the
# missing +1 normal-oxide alternatives without duplicating those experiences.
add_ion_pair_family("NormalOxide", "O", 2, "Oxygen", 2, 4, DOUBLE, [1])
ion_pair_experiences = [x for x in ion_pair_experiences if x["product_formula"] != "Li2O"]

MAIN_GROUP_METALS = ["Li", "Be", "Na", "Mg", "Al", "K", "Ca", "Rb", "Sr", "Cs", "Ba"]
ION_ELEMENTS = ["Li", "Na", "K", "Rb", "Cs", "Be", "Mg", "Ca", "Sr", "Ba", "Al", "F", "Cl", "Br", "I", "O", "S", "N", "P"]


def main_group_charge(element):
    if element in ("Li", "Na", "K", "Rb", "Cs"):
        return 1
    if element == "Al":
        return 3
    return 2


def metallic_domain_state(element, charge):
    return {
        "element": element,
        "site_formal_charge": charge,
        "site_local_electrons": 0,
        "delocalized_electrons_per_site": charge,
    }


# Reviewed ambient standard-state phases so the presentation layer can stage
# these species (see premise.material.oxygen-family.standard-phase).
MACROSCOPIC_GAS = ["Oxygen", "HydrogenForOxygen", "CGroup14Dioxide", "SSulfurDioxide"]
MACROSCOPIC_LIQUID = ["Water", "MnTransitionG7HeptoxideOxide"]
MACROSCOPIC_SOLID = [
    "BForOxygen", "CForOxygen", "SiForOxygen", "Phosphorus4ForOxygen", "SForOxygen",
    "CrTransitionG6TrioxideMetalForOxygen", "MoTransitionG6TrioxideMetalForOxygen",
    "WTransitionG6TrioxideMetalForOxygen", "MnTransitionG7HeptoxideMetalForOxygen",
    "TcTransitionG7HeptoxideMetalForOxygen", "ReTransitionG7HeptoxideMetalForOxygen",
    "RuTransitionG8TetroxideMetalForOxygen", "OsTransitionG8TetroxideMetalForOxygen",
    "BBoronOxide", "SiGroup14Dioxide", "Phosphorus5Oxide",
    "CrTransitionG6TrioxideOxide", "MoTransitionG6TrioxideOxide", "WTransitionG6TrioxideOxide",
    "TcTransitionG7HeptoxideOxide", "ReTransitionG7HeptoxideOxide",
    "RuTransitionG8TetroxideOxide", "OsTransitionG8TetroxideOxide",
]


def macroscopic_material(structure, phase, colour=None):
    record = {
        "structure": structure,
        "context": {"kind": "standard"},
        "phase": {"kind": phase},
        "premise_ids": ["premise.material.oxygen-family.standard-phase"],
    }
    if colour is not None:
        record["colour"] = colour
    return record


# Ion-pair anion sources at ambient standard state; colours mirror the
# reviewed values the covalent-combinations package uses for the same
# elements.  "Oxygen" already carries a record above.
ION_ANION_SOURCE_MATERIALS = {
    "ElementalFluorine": ("gas", [228, 232, 150]),
    "ElementalChlorine": ("gas", [202, 220, 112]),
    "ElementalBromine": ("liquid", [142, 57, 47]),
    "ElementalIodine": ("solid", [62, 53, 70]),
    "ElementalNitrogen": ("gas", None),
    "ElementalSulfur": ("solid", [232, 196, 55]),
    "ElementalPhosphorus": ("solid", [236, 224, 190]),
}


def ion_pair_macroscopic_materials():
    records = []
    seen = set()

    def add(structure, phase, colour=None):
        if structure in seen:
            return
        seen.add(structure)
        records.append(macroscopic_material(structure, phase, colour))

    for source_id, (phase, colour) in ION_ANION_SOURCE_MATERIALS.items():
        add(source_id, phase, colour)
    for experience in ion_pair_experiences:
        add(experience["subject_structure"], "solid")
        add(experience["product_structure"], "solid")
    return records


macroscopic_materials = (
    [macroscopic_material(s, "gas") for s in MACROSCOPIC_GAS]
    + [macroscopic_material(s, "liquid") for s in MACROSCOPIC_LIQUID]
    + [macroscopic_material(s, "solid") for s in MACROSCOPIC_SOLID]
    + ion_pair_macroscopic_materials()
)

candidate = {
    "schema_version": 1,
    "id": "oxygen-reactions",
    "evidence": [
        {
            "id": "evidence.openstax.oxygen-compounds",
            "title": "Chemistry: Atoms First 2e",
            "publisher": "OpenStax",
            "locator": "Occurrence, Preparation, and Compounds of Oxygen",
            "reference": "https://openstax.org/books/chemistry-atoms-first-2e/pages/18-9-occurrence-preparation-and-compounds-of-oxygen",
            "retrieved_on": "2026-07-15",
            "usage": "Representative normal oxide, peroxide, superoxide, and covalent oxide outcomes",
        },
        {
            "id": "evidence.openstax.ionic-compounds",
            "title": "Chemistry 2e",
            "publisher": "OpenStax",
            "locator": "Ionic Bonding",
            "reference": "https://openstax.org/books/chemistry-2e/pages/7-1-ionic-bonding",
            "retrieved_on": "2026-07-15",
            "usage": "Charge neutrality, electron transfer, fixed-charge monatomic ions, and binary ionic formula units",
        },
        {
            "id": "evidence.nist.webbook.standard-phases",
            "title": "NIST Chemistry WebBook",
            "publisher": "National Institute of Standards and Technology",
            "locator": "Standard state phase data for the recorded species",
            "reference": "https://webbook.nist.gov/chemistry/",
            "retrieved_on": "2026-07-19",
            "usage": "Ambient standard-state phases for oxygen-family reactants and oxides",
        },
    ],
    "premises": [
        premise(
            RULE_PREMISE,
            "The listed element families have the representative balanced oxygen outcomes encoded by their supported cases.",
        ),
        premise(
            STRUCTURE_PREMISE,
            "Oxygen and oxide products use explicit localized or delocalized bonds, formal charges, ionic components, and representative network fragments.",
        ),
        premise(
            VALENCE_PREMISE,
            "The listed electron states are the closed valence domain used by the oxygen reaction operations.",
        ),
        premise(
            OBSERVATION_PREMISE,
            "Formation of the oxide product and disappearance of the reactant are compatible generic observations for these representative theoretical experiences.",
        ),
        {
            "id": ION_RULE_PREMISE,
            "statement": "Fixed-charge binary ionic formula units use the smallest whole-number cation-to-anion ratio whose component charges sum to zero.",
            "evidence": ["evidence.openstax.ionic-compounds"],
            "review": {"status": "provisional", "reviewers": []},
            "rule_version": "1",
        },
        {
            "id": ION_STRUCTURE_PREMISE,
            "statement": "A binary ionic formula unit is represented by explicitly charged monatomic components in a charge-aware ionic association.",
            "evidence": ["evidence.openstax.ionic-compounds"],
            "review": {"status": "provisional", "reviewers": []},
            "rule_version": "1",
        },
        {
            "id": ION_VALENCE_PREMISE,
            "statement": "The fixed-charge ion-pair domain transfers the cation charge to anion valence vacancies after explanatory elemental-bond cleavage.",
            "evidence": ["evidence.openstax.ionic-compounds"],
            "review": {"status": "provisional", "reviewers": []},
            "rule_version": "1",
        },
        {
            "id": ION_OBSERVATION_PREMISE,
            "statement": "Formation of the binary salt and disappearance of the selected elemental reactants are compatible generic theoretical observations.",
            "evidence": ["evidence.openstax.ionic-compounds"],
            "review": {"status": "provisional", "reviewers": []},
            "rule_version": "1",
        },
        {
            "id": "premise.material.oxygen-family.standard-phase",
            "statement": "Each recorded oxygen-family species is presented in its ambient standard-state phase: gases O2, H2, CO2, and SO2; liquids H2O and Mn2O7; all other recorded elemental subjects and oxides as solids.",
            "evidence": ["evidence.nist.webbook.standard-phases"],
            "review": {"status": "provisional", "reviewers": []},
            "rule_version": "1",
        },
    ],
    "valence_premises": [
        {
            "premise_id": VALENCE_PREMISE,
            "neutral_valence": [
                {"element": element, "neutral_valence_electrons": electrons} for element, electrons in ELEMENTS.items()
            ],
            "supported_states": list(states.values()),
            "metallic_domain_states": [
                metallic_domain_state(element, main_group_charge(element)) for element in MAIN_GROUP_METALS
            ]
            + transition_metallic_states,
        },
        {
            "premise_id": ION_VALENCE_PREMISE,
            "neutral_valence": [
                {"element": element, "neutral_valence_electrons": electrons}
                for element, electrons in ELEMENTS.items()
                if element in ION_ELEMENTS
            ],
            "supported_states": [state for state in states.values() if state["element"] in ION_ELEMENTS],
            "metallic_domain_states": [
                metallic_domain_state(element, main_group_charge(element))
                for element in ["Li", "Na", "K", "Rb", "Cs", "Be", "Mg", "Ca", "Sr", "Ba", "Al"]
            ],
        },
    ],
    "structures": structures,
    "rules": [],
    "elements": [],
    "element_categories": categories,
    "structural_traits": [],
    "structure_templates": templates,
    "structure_applications": applications,
    "graph_patterns": patterns,
    "generalized_rules": rules,
    "macroscopic_materials": macroscopic_materials,
}

evidence_packet = {
    "schema_version": 1,
    "id": "Evidence.OxygenReaction@1",
    "claims": [
        {"id": "R1", "subject_role": "product", "subject": "oxide", "predicate": "forms", "sources": ["S1"]},
        {"id": "R2", "subject_role": "reactant", "subject": "element", "predicate": "disappears", "sources": ["S1"]},
    ],
    "sources": [
        {
            "id": "S1",
            "title": "Occurrence, Preparation, and Compounds of Oxygen",
            "publisher": "OpenStax",
            "url": "https://openstax.org/books/chemistry-atoms-first-2e/pages/18-9-occurrence-preparation-and-compounds-of-oxygen",
            "supports": ["R1", "R2"],
        }
    ],
}

write_json(CANDIDATE_DIR / "candidate.json", candidate)
write_json(CANDIDATE_DIR / "evidence.json", evidence_packet)

MAIN_GROUP_EXPERIENCES = [
    ("hydrogen-oxygen", "HydrogenAndOxygen", "2", "HydrogenForOxygen", "H2", "molecular", "2", "Water", "H2O", "molecular", "2 H2 + O2 -> 2 H2O", "Rules.HydrogenOxide"),
    ("lithium-oxygen", "LithiumAndOxygen", "4", "LiMetalForOxygen", "Li", "metallic", "2", "LiMonovalentNormalOxide", "Li2O", "ionic", "4 Li + O2 -> 2 Li2O", "Rules.MonovalentNormalOxide"),
    ("beryllium-oxygen", "BerylliumAndOxygen", "2", "BeMetalForOxygen", "Be", "metallic", "2", "BeDivalentNormalOxide", "BeO", "ionic", "2 Be + O2 -> 2 BeO", "Rules.DivalentNormalOxide"),
    ("boron-oxygen", "BoronAndOxygen", "4", "BForOxygen", "B", "molecular", "2", "BBoronOxide", "B2O3", "molecular", "4 B + 3 O2 -> 2 B2O3", "Rules.BoronOxide"),
    ("carbon-oxygen", "CarbonAndOxygen", "1", "CForOxygen", "C", "molecular", "1", "CGroup14Dioxide", "CO2", "molecular", "C + O2 -> CO2", "Rules.Group14Dioxide"),
    ("sodium-oxygen", "SodiumAndOxygen", "2", "NaMetalForOxygen", "Na", "metallic", "1", "NaMonovalentPeroxide", "Na2O2", "ionic", "2 Na + O2 -> Na2O2", "Rules.MonovalentPeroxide"),
    ("magnesium-oxygen", "MagnesiumAndOxygen", "2", "MgMetalForOxygen", "Mg", "metallic", "2", "MgDivalentNormalOxide", "MgO", "ionic", "2 Mg + O2 -> 2 MgO", "Rules.DivalentNormalOxide"),
    ("aluminium-oxygen", "AluminiumAndOxygen", "4", "AlMetalForOxygen", "Al", "metallic", "2", "AlTrivalentNormalOxide", "Al2O3", "ionic", "4 Al + 3 O2 -> 2 Al2O3", "Rules.TrivalentNormalOxide"),
    ("silicon-oxygen", "SiliconAndOxygen", "1", "SiForOxygen", "Si", "molecular", "1", "SiGroup14Dioxide", "SiO2", "molecular", "Si + O2 -> SiO2", "Rules.Group14Dioxide"),
    ("phosphorus-oxygen", "PhosphorusAndOxygen", "1", "Phosphorus4ForOxygen", "P4", "molecular", "1", "Phosphorus5Oxide", "P4O10", "molecular", "P4 + 5 O2 -> P4O10", "Rules.Phosphorus5Oxide"),
    ("sulfur-oxygen", "SulfurAndOxygen", "1", "SForOxygen", "S", "molecular", "1", "SSulfurDioxide", "SO2", "molecular", "S + O2 -> SO2", "Rules.SulfurDioxide"),
    ("potassium-oxygen", "PotassiumAndOxygen", "1", "KMetalForOxygen", "K", "metallic", "1", "KSuperoxide", "KO2", "ionic", "K + O2 -> KO2", "Rules.Superoxide"),
    ("calcium-oxygen", "CalciumAndOxygen", "2", "CaMetalForOxygen", "Ca", "metallic", "2", "CaDivalentNormalOxide", "CaO", "ionic", "2 Ca + O2 -> 2 CaO", "Rules.DivalentNormalOxide"),
    ("rubidium-oxygen", "RubidiumAndOxygen", "1", "RbMetalForOxygen", "Rb", "metallic", "1", "RbSuperoxide", "RbO2", "ionic", "Rb + O2 -> RbO2", "Rules.Superoxide"),
    ("strontium-oxygen", "StrontiumAndOxygen", "2", "SrMetalForOxygen", "Sr", "metallic", "2", "SrDivalentNormalOxide", "SrO", "ionic", "2 Sr + O2 -> 2 SrO", "Rules.DivalentNormalOxide"),
    ("caesium-oxygen", "CaesiumAndOxygen", "1", "CsMetalForOxygen", "Cs", "metallic", "1", "CsSuperoxide", "CsO2", "ionic", "Cs + O2 -> CsO2", "Rules.Superoxide"),
    ("barium-oxygen", "BariumAndOxygen", "2", "BaMetalForOxygen", "Ba", "metallic", "2", "BaDivalentNormalOxide", "BaO", "ionic", "2 Ba + O2 -> 2 BaO", "Rules.DivalentNormalOxide"),
]
experiences = MAIN_GROUP_EXPERIENCES + [tuple(x[:12]) for x in transition_experiences]
for x in experiences:
    (slug, reaction, subject_coeff, subject_structure, subject_formula, subject_rep,
     product_coeff, product_structure, product_formula, product_rep, equation, rule_name) = x
    match = re.search(r"\+ (?:(\d+) )?O2", equation)
    if not match:
        raise SystemExit(f"Cannot read oxygen coefficient from {equation}")
    oxygen_coeff = int(match.group(1)) if match.group(1) else 1
    source = (
        "chems 1\n"
        "use catalog ChemSpec.Theoretical@1\n"
        f"reaction {reaction} where\n"
        "  reactants\n"
        f"    subject := {subject_coeff} of {subject_structure}\n"
        f"    oxygen := {oxygen_coeff} of Oxygen\n"
        "  products\n"
        f"    oxide := {product_coeff} of {product_structure}\n"
        "  equation\n"
        f"    {subject_coeff} {subject_formula}[{subject_rep}] + {oxygen_coeff} O2[molecular]\n"
        f"    -> {product_coeff} {product_formula}[{product_rep}]\n"
        "  model\n"
        "    event := representative\n"
        "    sequence := explanatory\n"
        "  observe from Evidence.OxygenReaction@1\n"
        "    product oxide forms claim R1\n"
        "    reactant subject disappears claim R2\n"
        "  by\n"
        f"    apply {rule_name}\n"
        "      subject := subject\n"
        "      oxygen := oxygen\n"
        "      oxide := oxide\n"
    )
    write_utf8(EXPERIENCE_DIR / f"oxygen-{slug}-001.chems", source)
    write_json(OBSERVATION_DIR / f"oxygen-{slug}-001.evidence.json", evidence_packet)

ion_evidence = {
    "schema_version": 1,
    "id": "Evidence.IonPairReaction@1",
    "claims": [
        {"id": "R1", "subject_role": "product", "subject": "salt", "predicate": "forms", "sources": ["S1"]},
        {"id": "R2", "subject_role": "reactant", "subject": "metal", "predicate": "disappears", "sources": ["S1"]},
    ],
    "sources": [
        {
            "id": "S1",
            "title": "Ionic Bonding",
            "publisher": "OpenStax",
            "url": "https://openstax.org/books/chemistry-2e/pages/7-1-ionic-bonding",
            "supports": ["R1", "R2"],
        }
    ],
}
for x in ion_pair_experiences:
    source = (
        "chems 1\n"
        "use catalog ChemSpec.Theoretical@1\n"
        f"reaction {x['reaction']} where\n"
        "  reactants\n"
        f"    cation := {x['subject_coefficient']} of {x['subject_structure']}\n"
        f"    anion := {x['anion_coefficient']} of {x['anion_structure']}\n"
        "  products\n"
        f"    salt := {x['product_coefficient']} of {x['product_structure']}\n"
        "  equation\n"
        f"    {x['subject_coefficient']} {x['subject_formula']}[metallic] + {x['anion_coefficient']} {x['anion_formula']}[molecular]\n"
        f"    -> {x['product_coefficient']} {x['product_formula']}[ionic]\n"
        "  model\n"
        "    event := representative\n"
        "    sequence := explanatory\n"
        "  observe from Evidence.IonPairReaction@1\n"
        "    product salt forms claim R1\n"
        "    reactant cation disappears claim R2\n"
        "  by\n"
        f"    apply {x['rule']}\n"
        "      cation := cation\n"
        "      anion := anion\n"
        "      salt := salt\n"
    )
    write_utf8(EXPERIENCE_DIR / f"ionpair-{x['slug']}-001.chems", source)
    write_json(OBSERVATION_DIR / f"ionpair-{x['slug']}-001.evidence.json", ion_evidence)

# Register generated experiences through typed participant identities. Runtime
# availability is decided by kernel validation, never an approval field.
registry_path = ROOT / "catalogue/experience-registry.json"
registry = json.loads(registry_path.read_text(encoding="utf-8"))
base_experiences = [
    experience
    for experience in registry["experiences"]
    if not experience["id"].startswith("oxygen-") and not experience["id"].startswith("ionpair-")
]
for record in base_experiences:
    record.pop("status", None)
    record.pop("name", None)
SLUGS = {
    1: "hydrogen-oxygen", 3: "lithium-oxygen", 4: "beryllium-oxygen", 5: "boron-oxygen", 6: "carbon-oxygen",
    11: "sodium-oxygen", 12: "magnesium-oxygen", 13: "aluminium-oxygen", 14: "silicon-oxygen",
    15: "phosphorus-oxygen", 16: "sulfur-oxygen", 19: "potassium-oxygen", 20: "calcium-oxygen",
    37: "rubidium-oxygen", 38: "strontium-oxygen", 55: "caesium-oxygen", 56: "barium-oxygen",
}
element_catalogue = json.loads(
    (ROOT / "catalogue/candidates/periodic-table-and-alkali-water/candidate.json").read_text(encoding="utf-8")
)
element_names = {record["atomic_number"]: record["name"] for record in element_catalogue["elements"]}
screening = json.loads((ROOT / "catalogue/oxygen-screening/oxygen.json").read_text(encoding="utf-8"))


def registry_record(experience_id, source_stem, request, equation, subject_name, atomic_number, co_atomic_number, family):
    return {
        "id": experience_id,
        "source_path": f"conformance/end-to-end/{source_stem}-001.chems",
        "evidence_path": f"conformance/observations/{source_stem}-001.evidence.json",
        "request": request,
        "equation": equation,
        "subject_name": subject_name,
        "family": family,
        "participants": [
            {"kind": "element", "atomic_number": atomic_number},
            {"kind": "element", "atomic_number": co_atomic_number},
        ],
    }


candidate_experiences = []
for screened in screening["element_outcomes"]:
    if screened["outcome"]["kind"] != "representative":
        continue
    atomic_number = screened["atomic_number"]
    slug = SLUGS[atomic_number]
    name = element_names[atomic_number].lower()
    candidate_experiences.append(
        registry_record(
            f"oxygen-{slug}",
            f"oxygen-{slug}",
            f"What happens when {name} reacts with oxygen?",
            screened["outcome"]["equation"],
            name,
            atomic_number,
            8,
            "oxygen",
        )
    )
for transition in transition_experiences:
    slug = transition[0]
    atomic_number = transition[12]
    equation = transition[10]
    name = element_names[atomic_number].lower()
    candidate_experiences.append(
        registry_record(
            f"oxygen-{slug}",
            f"oxygen-{slug}",
            f"What happens when {name} reacts with oxygen for this reviewed product outcome?",
            equation,
            name,
            atomic_number,
            8,
            "oxygen",
        )
    )
ion_experiences = []
for x in ion_pair_experiences:
    name = element_names[x["atomic_number"]].lower()
    ion_experiences.append(
        registry_record(
            f"ionpair-{x['slug']}",
            f"ionpair-{x['slug']}",
            f"What fixed-charge ionic compound forms when {name} reacts with {x['anion_formula']}?",
            x["equation"],
            name,
            x["atomic_number"],
            x["co_atoms"][0],
            "fixed_charge_ion_pair",
        )
    )
registry["schema_version"] = 2
# The retired PowerShell generator emitted base experiences first, but the
# covalent generator historically ran afterwards and re-appended its records,
# leaving the promoted order oxygen -> ion pair -> base. Registry order is
# positional identity for `ReactionRequest::registry`, so preserve it.
registry["experiences"] = candidate_experiences + ion_experiences + base_experiences
write_json(registry_path, registry)
shutil.copyfile(EXPERIENCE_DIR / "oxygen-potassium-oxygen-001.chems", CANDIDATE_DIR / "example.chems")

print(
    f"Generated {len(rules)} reusable oxygen/ion-pair rules, "
    f"{len(experiences)} oxygen experiences, and {len(ion_pair_experiences)} ion-pair experiences."
)
