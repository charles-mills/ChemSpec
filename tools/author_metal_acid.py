"""Author fixed-charge metal + non-oxidising monoprotic acid families."""

from __future__ import annotations

import copy
import json
from math import gcd
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "catalogue/candidates/acid-base-neutralization/candidate.json"
PREMISE = "premise.rule.metal-acid.non-oxidising"
TRAIT_PREMISE = "premise.traits.reaction-family-classification"
MEMBERS = {1: ["Li", "Na", "K", "Rb", "Cs"], 2: ["Mg", "Ca", "Sr", "Ba"], 3: ["Al"]}
HALIDES = {"Cl": "Chloride", "Br": "Bromide", "I": "Iodide"}
NAMES = {"Li": "Lithium", "Na": "Sodium", "K": "Potassium", "Rb": "Rubidium", "Cs": "Caesium", "Mg": "Magnesium", "Ca": "Calcium", "Sr": "Strontium", "Ba": "Barium", "Al": "Aluminium"}
ATOMIC_NUMBERS = {"Li": 3, "Na": 11, "K": 19, "Rb": 37, "Cs": 55, "Mg": 12, "Ca": 20, "Sr": 38, "Ba": 56, "Al": 13}


def op(kind: str, **values):
    return {"kind": kind, "premise_ids": [PREMISE, "premise.valence.fixed-charge-ion-pairs"], **values}


def rule_for(charge: int) -> dict:
    metals = 2 // gcd(2, charge)
    acids = metals * charge
    gases = acids // 2
    category = f"Categories.MetalAcidCation{charge}"
    roles = {
        "metal": {"side": "reactant", "representation": "metallic", "coefficient": metals},
        "acid": {"side": "reactant", "representation": "molecular", "coefficient": acids},
        "salt": {"side": "product", "representation": "ionic", "coefficient": metals},
        "hydrogen": {"side": "product", "representation": "molecular", "coefficient": gases},
    }
    correspondence = []
    rewrite = []
    for m in range(1, metals + 1):
        correspondence.append({"reactant": f"metal[{m}].metal", "product": f"salt[{m}].cation1.metal", "premise_ids": [PREMISE]})
        rewrite.append(op("release_metallic", site=f"metal[{m}].metal", domain=f"metal[{m}].metallic", allocation="retain_electron", before={"site": [charge, 0, 0], "domain_electrons": charge}, after={"site": [0, charge, charge], "domain_electrons": 0}))
    for a in range(1, acids + 1):
        salt = (a - 1) // charge + 1
        anion = (a - 1) % charge + 1
        gas = (a - 1) // 2 + 1
        h = (a - 1) % 2 + 1
        correspondence.extend((
            {"reactant": f"acid[{a}].x", "product": f"salt[{salt}].anion{anion}.anion", "premise_ids": [PREMISE]},
            {"reactant": f"acid[{a}].h", "product": f"hydrogen[{gas}].h{h}", "premise_ids": [PREMISE]},
        ))
        rewrite.append(op("cleave_covalent", edge=[f"acid[{a}].h", f"acid[{a}].x", "single"], allocation={"heterolytic_to": f"acid[{a}].x"}, before={"left": [0, 0, 0], "right": [0, 6, 0]}, after={"left": [1, 0, 0], "right": [-1, 8, 0]}))
    for m in range(1, metals + 1):
        for offset in range(charge):
            a = (m - 1) * charge + offset + 1
            rewrite.append(op("transfer_electron", count=1, donor=f"metal[{m}].metal", acceptor=f"acid[{a}].h", before={"donor": [offset, charge - offset, charge - offset], "acceptor": [1, 0, 0]}, after={"donor": [offset + 1, charge - offset - 1, charge - offset - 1], "acceptor": [0, 1, 1]}))
    for gas in range(1, gases + 1):
        a1, a2 = gas * 2 - 1, gas * 2
        rewrite.append(op("form_covalent", edge=[f"acid[{a1}].h", f"acid[{a2}].h", "single"], electron_contribution={"left": 1, "right": 1}, before={"left": [0, 1, 1], "right": [0, 1, 1]}, after={"left": [0, 0, 0], "right": [0, 0, 0]}))
    for m in range(1, metals + 1):
        acid_indices = list(range((m - 1) * charge + 1, m * charge + 1))
        atoms = [f"metal[{m}].metal"] + [f"acid[{a}].x" for a in acid_indices]
        rewrite.extend((
            op("associate_ionic", label=f"ionic.metal-acid-{m}", components=[[atoms[0]]] + [[atom] for atom in atoms[1:]], component_charges=[charge] + [-1] * charge),
            op("assign_product", atoms=atoms, product=f"salt[{m}]"),
        ))
    for gas in range(1, gases + 1):
        a1, a2 = gas * 2 - 1, gas * 2
        rewrite.append(op("assign_product", atoms=[f"acid[{a1}].h", f"acid[{a2}].h"], product=f"hydrogen[{gas}]"))

    cases = []
    for halide, name in HALIDES.items():
        cases.append({
            "status": "supported", "id": halide.lower(),
            "when": {"kind": "parameter_equals", "parameter": "halide", "value": halide},
            "products": {
                "salt": {"kind": "template", "template": f"Templates.FixedCation{charge}{name}Product", "arguments": {"member": {"parameter": "member"}}},
                "hydrogen": {"kind": "exact", "structure": "Hydrogen"},
            },
            "patterns": {"metal": f"Patterns.FixedCation{charge}Metal", "acid": "Patterns.HydrogenHalide"},
            "correspondence": copy.deepcopy(correspondence), "rewrite": copy.deepcopy(rewrite),
            "observation_compatibility": [{"subject_role": "hydrogen", "predicate": "evolves", "evidence_subject": "hydrogen", "premise_id": PREMISE}],
            "premise_ids": [PREMISE],
        })
    cases.append({"status": "unsupported", "id": "weak-hf", "when": {"kind": "parameter_equals", "parameter": "halide", "value": "F"}, "required_feature": "Features.WeakAcidEquilibrium", "explanation": "HF requires an equilibrium model; this family is limited to reviewed non-oxidising strong monoprotic acids.", "premise_ids": [PREMISE]})
    bound = [PREMISE, TRAIT_PREMISE, "premise.elements.iupac-periodic-table", "premise.elements.context-specific-reaction-facts", "premise.category.halide", "premise.rule.fixed-charge-ion-pairs", "premise.structure.fixed-charge-ion-pairs", "premise.valence.fixed-charge-ion-pairs", "premise.structure.hydrogen-halide", "premise.structure.hydrogen", "premise.valence.acid-base.initial-domain"]
    return {
        "id": f"Rules.FixedCation{charge}WithNonOxidisingAcid",
        "parameters": {"member": {"kind": "element", "category": category}, "halide": {"kind": "element", "category": "Categories.Halide"}},
        "roles": roles,
        "reactants": {"metal": {"kind": "template", "template": f"Templates.FixedCation{charge}Metal", "arguments": {"member": {"parameter": "member"}}}, "acid": {"kind": "template", "template": "Templates.HydrogenHalide", "arguments": {"halide": {"parameter": "halide"}}}},
        "cases": cases,
        "applicability": {"premise_id": PREMISE, "request_relation": "contact", "required_context": "theoretical contact with a reviewed non-oxidising strong monoprotic acid; passivation and oxidising-acid pathways are outside this family"},
        "model_assumptions": {"event": "representative", "sequence": "explanatory", "premise_ids": [PREMISE]},
        "premise_ids": bound,
    }


def main() -> None:
    doc = json.loads(PATH.read_text(encoding="utf-8"))
    doc["premises"] = [p for p in doc["premises"] if p["id"] != PREMISE]
    doc["element_categories"] = [c for c in doc.get("element_categories", []) if not c["id"].startswith("Categories.MetalAcidCation")]
    doc["generalized_rules"] = [r for r in doc["generalized_rules"] if "WithNonOxidisingAcid" not in r["id"]]
    PATH.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")

    metal_acid = {
        "schema_version": 1,
        "id": "metal-acid",
        "premises": [{"id": PREMISE, "statement": "A fixed-charge metal above hydrogen in the reviewed hydrogen-displacement series reacts in this theoretical model with HCl, HBr, or HI to form the corresponding ionic halide and H2. Oxidising acids, passivation, and HF equilibrium are separate models.", "evidence": ["evidence.openstax.chemistry-2e.acid-base"], "review": {"status": "provisional", "reviewers": []}, "rule_version": "1"}],
        "element_categories": [],
        "generalized_rules": [],
    }
    for charge, members in MEMBERS.items():
        metal_acid["element_categories"].append({"id": f"Categories.MetalAcidCation{charge}", "subject": "element", "membership": {"kind": "explicit", "members": members}, "premise_ids": [PREMISE]})
    metal_acid["generalized_rules"].extend(rule_for(charge) for charge in (1, 2, 3))
    package = ROOT / "catalogue/candidates/metal-acid"
    package.mkdir(parents=True, exist_ok=True)
    (package / "candidate.json").write_text(json.dumps(metal_acid, indent=2) + "\n", encoding="utf-8")

    registry_path = ROOT / "catalogue/experience-registry.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    registry["experiences"] = [e for e in registry["experiences"] if not e["id"].startswith("metal-acid-")]
    for charge, members in MEMBERS.items():
        metal_count = 2 // gcd(2, charge)
        acid_count = metal_count * charge
        gas_count = acid_count // 2
        for symbol in members:
            metal_name = NAMES[symbol]
            for halide, halide_name in HALIDES.items():
                acid_formula = f"H{halide}"
                salt_formula = f"{symbol}{halide if charge == 1 else halide + str(charge)}"
                salt_id = f"{symbol}FixedCation{charge}{halide_name}"
                slug = f"metal-acid-{symbol.lower()}-{halide.lower()}"
                source_path = f"conformance/end-to-end/{slug}-001.chems"
                evidence_path = f"conformance/observations/{slug}-001.evidence.json"
                equation_left = f"{metal_count if metal_count > 1 else ''}{symbol} + {acid_count if acid_count > 1 else ''}{acid_formula}"
                equation_right = f"{metal_count if metal_count > 1 else ''}{salt_formula} + {gas_count if gas_count > 1 else ''}H2"
                source = f"""chems 1
use catalog ChemSpec.Theoretical@1

reaction {metal_name}WithHydrogen{halide_name} where
  reactants
    metal := {metal_count} of {symbol}FixedCation{charge}Metal
    acid := {acid_count} of Hydrogen{halide_name}
  products
    salt := {metal_count} of {salt_id}
    hydrogen := {gas_count} of Hydrogen
  equation
    {metal_count} {symbol}[metallic] + {acid_count} {acid_formula}[molecular]
    -> {metal_count} {salt_formula}[ionic] + {gas_count} H2[molecular]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.MetalAcid{metal_name}{halide_name}@1
    gas hydrogen evolves claim R1
  by
    apply Rules.FixedCation{charge}WithNonOxidisingAcid
      metal := metal
      acid := acid
      salt := salt
      hydrogen := hydrogen
"""
                while "\n\n" in source:
                    source = source.replace("\n\n", "\n")
                evidence = {"schema_version": 1, "id": f"Evidence.MetalAcid{metal_name}{halide_name}@1", "claims": [{"id": "R1", "subject_role": "gas", "subject": "hydrogen", "predicate": "evolves", "sources": ["S1"]}], "sources": [{"id": "S1", "title": "Single-displacement reactions", "publisher": "OpenStax", "url": "https://openstax.org/books/chemistry-2e/pages/4-2-classifying-chemical-reactions", "supports": ["R1"]}]}
                (ROOT / source_path).write_text(source, encoding="utf-8", newline="\n")
                (ROOT / evidence_path).write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
                registry["experiences"].append({"id": slug, "status": "trusted", "family": "metal_acid", "participants": [{"kind": "element", "atomic_number": ATOMIC_NUMBERS[symbol]}, {"kind": "composition", "formula": acid_formula}], "source_path": source_path, "evidence_path": evidence_path, "request": f"What happens when {metal_name.lower()} reacts with hydrogen {halide_name.lower()}?", "equation": f"{equation_left} -> {equation_right}", "subject_name": metal_name.lower(), "product_structure": salt_id})
    registry_path.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")
    (package / "example.chems").write_text((ROOT / "conformance/end-to-end/metal-acid-li-cl-001.chems").read_text(encoding="utf-8"), encoding="utf-8", newline="\n")
    (package / "evidence.json").write_text((ROOT / "conformance/observations/metal-acid-li-cl-001.evidence.json").read_text(encoding="utf-8"), encoding="utf-8")


if __name__ == "__main__":
    main()
