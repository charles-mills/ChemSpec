"""Attach semantic reaction-family traits to reviewed structural graphs.

The traits identify chemistry from graph-backed catalogue records.  Runtime
code must query these assertions rather than switch on compound names.
"""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CANDIDATES = ROOT / "catalogue/candidates"
PREMISE = "premise.traits.reaction-family-classification"

TRAITS = (
    "Traits.BronstedAcidProtonDonor",
    "Traits.HydroxideBase",
    "Traits.CarbonateBase",
    "Traits.SolubleIonicReactant",
    "Traits.InsolubleIonicProduct",
    "Traits.ElementalMetalReactant",
    "Traits.HalogenOxidant",
    "Traits.OxygenOxidant",
    "Traits.CombustibleFuel",
    "Traits.WaterReactant",
)


def load(package: str) -> tuple[Path, dict]:
    path = CANDIDATES / package / "candidate.json"
    return path, json.loads(path.read_text(encoding="utf-8"))


def save(path: Path, document: dict) -> None:
    # Any rule consuming a trait-asserting structure/template explicitly binds
    # the semantic classification premise into its proof boundary.
    for rule in document.get("generalized_rules", []):
        rule["premise_ids"] = sorted(set(rule.get("premise_ids", [])) | {PREMISE})
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def first_atom(record: dict) -> str:
    if record.get("representation") == "ionic":
        component = record["components"][0]
        return f'{component["label"]}.{component["atoms"][0]["label"]}'
    atoms = record.get("atoms") or record.get("sites")
    return atoms[0]["label"]


def assert_trait(record: dict, trait: str) -> None:
    assertions = [a for a in record.get("traits", []) if a.get("trait") != trait]
    assertions.append({
        "trait": trait,
        "sites": {"reactive_site": first_atom(record)},
        "premise_ids": [PREMISE],
    })
    record["traits"] = assertions


def by_id(records: list[dict], identifier: str) -> dict:
    return next(record for record in records if record["id"] == identifier)


def main() -> None:
    from author_combustion import main as author_combustion
    author_combustion()
    from author_metal_displacement import main as author_metal_displacement
    author_metal_displacement()
    from author_general_precipitation import main as author_general_precipitation
    author_general_precipitation()

    # Metal/acid rules share the same semantic premise package and must be
    # authored before proof-binding is applied below.
    from author_metal_acid import main as author_metal_acid
    author_metal_acid()

    acid_path, acid = load("acid-base-neutralization")
    acid["premises"] = [p for p in acid["premises"] if p["id"] != PREMISE]
    acid["premises"].append({
        "id": PREMISE,
        "statement": (
            "Reaction-family traits are reviewed semantic assertions attached "
            "to exact structural sites. They classify a graph for screening "
            "without inferring that every member has identical strength, "
            "solubility, kinetics, or reaction conditions."
        ),
        "evidence": ["evidence.openstax.chemistry-2e.acid-base"],
        "review": {"status": "provisional", "reviewers": []},
        "rule_version": "1",
    })
    acid["structural_traits"] = [
        trait for trait in acid.get("structural_traits", [])
        if trait["id"] not in TRAITS
    ] + [{
        "id": trait,
        "sites": {"reactive_site": "atom"},
        "premise_ids": [PREMISE],
    } for trait in TRAITS]
    assert_trait(by_id(acid["structure_templates"], "Templates.HydrogenHalide"), TRAITS[0])
    save(acid_path, acid)

    periodic_path, periodic = load("periodic-table-and-alkali-water")
    hydroxide = by_id(periodic["structure_templates"], "Templates.AlkaliMetalHydroxide")
    assert_trait(hydroxide, TRAITS[1])
    metal = by_id(periodic["structure_templates"], "Templates.AlkaliMetal")
    assert_trait(metal, TRAITS[5])
    hydrogen = by_id(periodic["structures"], "Hydrogen")
    assert_trait(hydrogen, TRAITS[8])
    water = by_id(periodic["structures"], "Water")
    assert_trait(water, TRAITS[9])
    save(periodic_path, periodic)

    carbonate_path, carbonate = load("acid-carbonate-gas-evolution")
    for identifier in ("Templates.AlkaliMetalBicarbonate", "Templates.AlkaliMetalCarbonate"):
        assert_trait(by_id(carbonate["structure_templates"], identifier), TRAITS[2])
    save(carbonate_path, carbonate)

    precip_path, precip = load("precipitation-silver-halide")
    assert_trait(by_id(precip["structures"], "SilverNitrate"), TRAITS[3])
    for identifier in ("Templates.AlkaliMetalHalide", "Templates.AlkaliMetalNitrate"):
        assert_trait(by_id(precip["structure_templates"], identifier), TRAITS[3])
    assert_trait(by_id(precip["structure_templates"], "Templates.SilverHalide"), TRAITS[4])
    save(precip_path, precip)

    halogen_path, halogen = load("single-displacement-halogen")
    assert_trait(by_id(halogen["structure_templates"], "Templates.DiatomicHalogen"), TRAITS[6])
    save(halogen_path, halogen)

    oxygen_path, oxygen = load("oxygen-reactions")
    assert_trait(by_id(oxygen["structures"], "Oxygen"), TRAITS[7])
    assert_trait(by_id(oxygen["structures"], "Ozone"), TRAITS[7])
    for template in oxygen["structure_templates"]:
        if template.get("representation") == "metallic":
            assert_trait(template, TRAITS[5])
    save(oxygen_path, oxygen)

    # Covalent rules consume the shared diatomic-halogen applications.
    covalent_path, covalent = load("covalent-combinations")
    save(covalent_path, covalent)

    from author_reaction_review import main as author_reaction_review
    author_reaction_review()


if __name__ == "__main__":
    main()
