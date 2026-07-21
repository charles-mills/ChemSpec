"""Generate the reviewed Group 2 metal/water catalogue candidate.

Ca, Sr, and Ba use ordinary liquid-water contact. Magnesium is deliberately a
separate steam rule producing magnesium oxide and hydrogen; beryllium remains
outside the supported family.
"""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CANDIDATE_DIR = ROOT / "catalogue/candidates/alkaline-earth-water"
RULE_PREMISE = "premise.rule.alkaline-earth-water.standard-outcome"
METAL_PREMISE = "premise.structure.alkaline-earth-metal"
HYDROXIDE_PREMISE = "premise.structure.alkaline-earth-metal-hydroxide"
VALENCE_PREMISE = "premise.valence.alkaline-earth-h-o.initial-domain"
STEAM_RULE_PREMISE = "premise.rule.magnesium-steam.standard-outcome"
STEAM_OXIDE_PREMISE = "premise.structure.magnesium-oxide"
MATERIAL_PREMISE = "premise.material.alkaline-earth-water.presentation"


def review() -> dict:
    return {"status": "provisional", "reviewers": []}


def premise(identifier: str, statement: str, evidence: list[str]) -> dict:
    return {
        "id": identifier,
        "statement": statement,
        "evidence": evidence,
        "review": review(),
        "rule_version": "1",
    }


def metal_template() -> dict:
    return {
        "representation": "metallic",
        "id": "Templates.AlkalineEarthWaterMetal",
        "parameters": {
            "member": {"kind": "element", "category": "Categories.WaterReactiveAlkalineEarth"}
        },
        "sites": [
            {
                "label": "metal",
                "element": {"parameter": "member"},
                "formal_charge": 2,
                "non_bonding_electrons": 0,
                "unpaired_electrons": 0,
            }
        ],
        "domains": [
            {"label": "metallic", "sites": ["metal"], "delocalized_electrons": 2}
        ],
        "premise_ids": [RULE_PREMISE, METAL_PREMISE],
    }


def hydroxide_component(label: str) -> dict:
    return {
        "label": label,
        "atoms": [
            {
                "label": "o",
                "element": "O",
                "formal_charge": -1,
                "non_bonding_electrons": 6,
                "unpaired_electrons": 0,
            },
            {
                "label": "h",
                "element": "H",
                "formal_charge": 0,
                "non_bonding_electrons": 0,
                "unpaired_electrons": 0,
            },
        ],
        "bonds": [{"left": "o", "right": "h", "order": "single"}],
        "groups": [],
    }


def hydroxide_template() -> dict:
    return {
        "representation": "ionic",
        "id": "Templates.AlkalineEarthWaterHydroxide",
        "parameters": {
            "member": {"kind": "element", "category": "Categories.WaterReactiveAlkalineEarth"}
        },
        "components": [
            {
                "label": "metal",
                "atoms": [
                    {
                        "label": "metal",
                        "element": {"parameter": "member"},
                        "formal_charge": 2,
                        "non_bonding_electrons": 0,
                        "unpaired_electrons": 0,
                    }
                ],
                "bonds": [],
                "groups": [],
            },
            hydroxide_component("hydroxide1"),
            hydroxide_component("hydroxide2"),
        ],
        "associations": [
            {
                "label": "ionic",
                "components": ["metal", "hydroxide1", "hydroxide2"],
            }
        ],
        "premise_ids": [RULE_PREMISE, HYDROXIDE_PREMISE],
    }


def water_pattern() -> dict:
    source = ROOT / "catalogue/candidates/periodic-table-and-alkali-water/candidate.json"
    base = json.loads(source.read_text(encoding="utf-8"))
    pattern = next(item for item in base["graph_patterns"] if item["id"] == "Patterns.Water")
    pattern["id"] = "Patterns.AlkalineEarthWater"
    pattern["premise_ids"] = [RULE_PREMISE, "premise.structure.water"]
    return pattern


def steam_water_pattern() -> dict:
    pattern = water_pattern()
    pattern["id"] = "Patterns.MagnesiumSteamWater"
    pattern["premise_ids"] = [STEAM_RULE_PREMISE, "premise.structure.water"]
    return pattern


def application(name: str, symbol: str, product: bool) -> dict:
    suffix = "WaterHydroxide" if product else "WaterMetal"
    template = (
        "Templates.AlkalineEarthWaterHydroxide"
        if product
        else "Templates.AlkalineEarthWaterMetal"
    )
    formula = f"{symbol}(OH)2" if product else symbol
    return {
        "id": f"{name}{suffix}",
        "template": template,
        "arguments": {"member": symbol},
        "formula": formula,
        "premise_ids": [RULE_PREMISE, HYDROXIDE_PREMISE if product else METAL_PREMISE],
    }


def endpoint(charge: int, local: int, unpaired: int) -> list[int]:
    return [charge, local, unpaired]


def rewrite() -> list[dict]:
    shared = [RULE_PREMISE, METAL_PREMISE, "premise.structure.water", VALENCE_PREMISE]
    operations: list[dict] = [
        {
            "kind": "release_metallic",
            "premise_ids": [RULE_PREMISE, METAL_PREMISE, VALENCE_PREMISE],
            "site": "metal[1].metal",
            "domain": "metal[1].metallic",
            "allocation": "retain_electron",
            "before": {"site": endpoint(2, 0, 0), "domain_electrons": 2},
            "after": {"site": endpoint(0, 2, 2), "domain_electrons": 0},
        }
    ]
    for instance in (1, 2):
        operations.append(
            {
                "kind": "cleave_covalent",
                "premise_ids": [RULE_PREMISE, "premise.structure.water", VALENCE_PREMISE],
                "edge": [f"water[{instance}].o", f"water[{instance}].h1", "single"],
                "allocation": {"heterolytic_to": f"water[{instance}].o"},
                "before": {"left": endpoint(0, 4, 0), "right": endpoint(0, 0, 0)},
                "after": {"left": endpoint(-1, 6, 0), "right": endpoint(1, 0, 0)},
            }
        )
    operations.extend(
        [
            {
                "kind": "transfer_electron",
                "premise_ids": shared,
                "count": 1,
                "donor": "metal[1].metal",
                "acceptor": "water[1].h1",
                "before": {"donor": endpoint(0, 2, 2), "acceptor": endpoint(1, 0, 0)},
                "after": {"donor": endpoint(1, 1, 1), "acceptor": endpoint(0, 1, 1)},
            },
            {
                "kind": "transfer_electron",
                "premise_ids": shared,
                "count": 1,
                "donor": "metal[1].metal",
                "acceptor": "water[2].h1",
                "before": {"donor": endpoint(1, 1, 1), "acceptor": endpoint(1, 0, 0)},
                "after": {"donor": endpoint(2, 0, 0), "acceptor": endpoint(0, 1, 1)},
            },
            {
                "kind": "form_covalent",
                "premise_ids": [
                    RULE_PREMISE,
                    "premise.structure.hydrogen",
                    "premise.structure.water",
                    VALENCE_PREMISE,
                ],
                "edge": ["water[1].h1", "water[2].h1", "single"],
                "electron_contribution": {"left": 1, "right": 1},
                "before": {"left": endpoint(0, 1, 1), "right": endpoint(0, 1, 1)},
                "after": {"left": endpoint(0, 0, 0), "right": endpoint(0, 0, 0)},
            },
            {
                "kind": "associate_ionic",
                "premise_ids": [
                    RULE_PREMISE,
                    METAL_PREMISE,
                    HYDROXIDE_PREMISE,
                    "premise.structure.water",
                    VALENCE_PREMISE,
                ],
                "label": "ionic.product1",
                "components": [
                    ["metal[1].metal"],
                    ["water[1].o", "water[1].h2"],
                    ["water[2].o", "water[2].h2"],
                ],
                "component_charges": [2, -1, -1],
            },
            {
                "kind": "assign_product",
                "premise_ids": [RULE_PREMISE, HYDROXIDE_PREMISE],
                "atoms": [
                    "metal[1].metal",
                    "water[1].o",
                    "water[1].h2",
                    "water[2].o",
                    "water[2].h2",
                ],
                "product": "hydroxide[1]",
            },
            {
                "kind": "assign_product",
                "premise_ids": [RULE_PREMISE, "premise.structure.hydrogen"],
                "atoms": ["water[1].h1", "water[2].h1"],
                "product": "gasProduct[1]",
            },
        ]
    )
    return operations


def correspondence() -> list[dict]:
    def item(reactant: str, product: str, premises: list[str]) -> dict:
        return {"reactant": reactant, "product": product, "premise_ids": premises}

    entries = [
        item(
            "metal[1].metal",
            "hydroxide[1].metal.metal",
            [RULE_PREMISE, METAL_PREMISE, HYDROXIDE_PREMISE],
        )
    ]
    for instance in (1, 2):
        component = f"hydroxide{instance}"
        entries.extend(
            [
                item(
                    f"water[{instance}].o",
                    f"hydroxide[1].{component}.o",
                    [RULE_PREMISE, HYDROXIDE_PREMISE, "premise.structure.water"],
                ),
                item(
                    f"water[{instance}].h2",
                    f"hydroxide[1].{component}.h",
                    [RULE_PREMISE, HYDROXIDE_PREMISE, "premise.structure.water"],
                ),
                item(
                    f"water[{instance}].h1",
                    f"gasProduct[1].h{instance}",
                    [RULE_PREMISE, "premise.structure.hydrogen", "premise.structure.water"],
                ),
            ]
        )
    return entries


def generalized_rule() -> dict:
    return {
        "id": "Rules.AlkalineEarthMetalWithWater",
        "parameters": {
            "member": {"kind": "element", "category": "Categories.WaterReactiveAlkalineEarth"}
        },
        "roles": {
            "gasProduct": {"side": "product", "representation": "molecular", "coefficient": 1},
            "hydroxide": {"side": "product", "representation": "ionic", "coefficient": 1},
            "metal": {"side": "reactant", "representation": "metallic", "coefficient": 1},
            "water": {"side": "reactant", "representation": "molecular", "coefficient": 2},
        },
        "reactants": {
            "metal": {
                "kind": "template",
                "template": "Templates.AlkalineEarthWaterMetal",
                "arguments": {"member": {"parameter": "member"}},
            },
            "water": {"kind": "exact", "structure": "Water"},
        },
        "cases": [
            {
                "status": "supported",
                "id": "standard",
                "when": {"kind": "always"},
                "products": {
                    "gasProduct": {"kind": "exact", "structure": "Hydrogen"},
                    "hydroxide": {
                        "kind": "template",
                        "template": "Templates.AlkalineEarthWaterHydroxide",
                        "arguments": {"member": {"parameter": "member"}},
                    },
                },
                "patterns": {
                    "metal": "Patterns.AlkalineEarthWaterMetal",
                    "water": "Patterns.AlkalineEarthWater",
                },
                "correspondence": correspondence(),
                "rewrite": rewrite(),
                "observation_compatibility": [
                    {
                        "subject_role": "gasProduct",
                        "predicate": "evolves",
                        "evidence_subject": "hydrogen",
                        "premise_id": "premise.observation.group2-hydrogen-evolves",
                    },
                    {
                        "subject_role": "metal",
                        "predicate": "disappears",
                        "evidence_subject": "alkaline earth metal",
                        "premise_id": "premise.observation.alkaline-earth-metal-disappears",
                    },
                ],
                "premise_ids": [RULE_PREMISE],
            }
        ],
        "applicability": {
            "premise_id": RULE_PREMISE,
            "request_relation": "contact",
            "required_context": "ordinary water contact for the reviewed Ca, Sr, and Ba representative outcomes",
        },
        "model_assumptions": {
            "event": "representative",
            "sequence": "explanatory",
            "premise_ids": [RULE_PREMISE],
        },
        "premise_ids": [
            "premise.elements.iupac-periodic-table",
            "premise.observation.alkaline-earth-metal-disappears",
            "premise.observation.group2-hydrogen-evolves",
            RULE_PREMISE,
            METAL_PREMISE,
            HYDROXIDE_PREMISE,
            "premise.structure.hydrogen",
            "premise.structure.water",
            VALENCE_PREMISE,
        ],
    }


def magnesium_oxide() -> dict:
    return {
        "representation": "ionic",
        "id": "MagnesiumSteamOxide",
        "formula": "MgO",
        "components": [
            {
                "label": "metal",
                "atoms": [{"label": "metal", "element": "Mg", "formal_charge": 2, "non_bonding_electrons": 0, "unpaired_electrons": 0}],
                "bonds": [],
                "groups": [],
            },
            {
                "label": "oxide",
                "atoms": [{"label": "o", "element": "O", "formal_charge": -2, "non_bonding_electrons": 8, "unpaired_electrons": 0}],
                "bonds": [],
                "groups": [],
            },
        ],
        "associations": [{"label": "ionic", "components": ["metal", "oxide"]}],
        "premise_id": STEAM_OXIDE_PREMISE,
    }


def magnesium_metal_template() -> dict:
    return {
        "representation": "metallic",
        "id": "Templates.MagnesiumSteamMetal",
        "parameters": {"member": {"kind": "element", "category": "Categories.MagnesiumSteam"}},
        "sites": [{"label": "metal", "element": {"parameter": "member"}, "formal_charge": 2, "non_bonding_electrons": 0, "unpaired_electrons": 0}],
        "domains": [{"label": "metallic", "sites": ["metal"], "delocalized_electrons": 2}],
        "premise_ids": [STEAM_RULE_PREMISE, METAL_PREMISE],
    }


def magnesium_steam_rewrite() -> list[dict]:
    shared = [STEAM_RULE_PREMISE, METAL_PREMISE, "premise.structure.water", VALENCE_PREMISE]
    return [
        {"kind": "release_metallic", "premise_ids": shared, "site": "metal[1].metal", "domain": "metal[1].metallic", "allocation": "retain_electron", "before": {"site": endpoint(2, 0, 0), "domain_electrons": 2}, "after": {"site": endpoint(0, 2, 2), "domain_electrons": 0}},
        {"kind": "cleave_covalent", "premise_ids": shared, "edge": ["water[1].o", "water[1].h1", "single"], "allocation": {"heterolytic_to": "water[1].o"}, "before": {"left": endpoint(0, 4, 0), "right": endpoint(0, 0, 0)}, "after": {"left": endpoint(-1, 6, 0), "right": endpoint(1, 0, 0)}},
        {"kind": "cleave_covalent", "premise_ids": shared, "edge": ["water[1].o", "water[1].h2", "single"], "allocation": {"heterolytic_to": "water[1].o"}, "before": {"left": endpoint(-1, 6, 0), "right": endpoint(0, 0, 0)}, "after": {"left": endpoint(-2, 8, 0), "right": endpoint(1, 0, 0)}},
        {"kind": "transfer_electron", "premise_ids": shared, "count": 1, "donor": "metal[1].metal", "acceptor": "water[1].h1", "before": {"donor": endpoint(0, 2, 2), "acceptor": endpoint(1, 0, 0)}, "after": {"donor": endpoint(1, 1, 1), "acceptor": endpoint(0, 1, 1)}},
        {"kind": "transfer_electron", "premise_ids": shared, "count": 1, "donor": "metal[1].metal", "acceptor": "water[1].h2", "before": {"donor": endpoint(1, 1, 1), "acceptor": endpoint(1, 0, 0)}, "after": {"donor": endpoint(2, 0, 0), "acceptor": endpoint(0, 1, 1)}},
        {"kind": "form_covalent", "premise_ids": shared, "edge": ["water[1].h1", "water[1].h2", "single"], "electron_contribution": {"left": 1, "right": 1}, "before": {"left": endpoint(0, 1, 1), "right": endpoint(0, 1, 1)}, "after": {"left": endpoint(0, 0, 0), "right": endpoint(0, 0, 0)}},
        {"kind": "associate_ionic", "premise_ids": shared + [STEAM_OXIDE_PREMISE], "label": "ionic.product1", "components": [["metal[1].metal"], ["water[1].o"]], "component_charges": [2, -2]},
        {"kind": "assign_product", "premise_ids": [STEAM_RULE_PREMISE, STEAM_OXIDE_PREMISE], "atoms": ["metal[1].metal", "water[1].o"], "product": "oxide[1]"},
        {"kind": "assign_product", "premise_ids": [STEAM_RULE_PREMISE, "premise.structure.hydrogen"], "atoms": ["water[1].h1", "water[1].h2"], "product": "gasProduct[1]"},
    ]


def magnesium_steam_rule() -> dict:
    return {
        "id": "Rules.MagnesiumWithSteam",
        "parameters": {"member": {"kind": "element", "category": "Categories.MagnesiumSteam"}},
        "roles": {
            "gasProduct": {"side": "product", "representation": "molecular", "coefficient": 1},
            "oxide": {"side": "product", "representation": "ionic", "coefficient": 1},
            "metal": {"side": "reactant", "representation": "metallic", "coefficient": 1},
            "water": {"side": "reactant", "representation": "molecular", "coefficient": 1},
        },
        "reactants": {"metal": {"kind": "template", "template": "Templates.MagnesiumSteamMetal", "arguments": {"member": {"parameter": "member"}}}, "water": {"kind": "exact", "structure": "Water"}},
        "cases": [{
            "status": "supported", "id": "steam", "when": {"kind": "always"},
            "products": {"gasProduct": {"kind": "exact", "structure": "Hydrogen"}, "oxide": {"kind": "exact", "structure": "MagnesiumSteamOxide"}},
            "patterns": {"metal": "Patterns.MagnesiumSteamMetal", "water": "Patterns.MagnesiumSteamWater"},
            "correspondence": [
                {"reactant": "metal[1].metal", "product": "oxide[1].metal.metal", "premise_ids": [STEAM_RULE_PREMISE, STEAM_OXIDE_PREMISE]},
                {"reactant": "water[1].o", "product": "oxide[1].oxide.o", "premise_ids": [STEAM_RULE_PREMISE, STEAM_OXIDE_PREMISE]},
                {"reactant": "water[1].h1", "product": "gasProduct[1].h1", "premise_ids": [STEAM_RULE_PREMISE, "premise.structure.hydrogen"]},
                {"reactant": "water[1].h2", "product": "gasProduct[1].h2", "premise_ids": [STEAM_RULE_PREMISE, "premise.structure.hydrogen"]},
            ],
            "rewrite": magnesium_steam_rewrite(),
            "observation_compatibility": [{"subject_role": "gasProduct", "predicate": "forms", "evidence_subject": "hydrogen", "premise_id": "premise.observation.magnesium-steam-products-form"}],
            "premise_ids": [STEAM_RULE_PREMISE],
        }],
        "applicability": {"premise_id": STEAM_RULE_PREMISE, "request_relation": "contact", "required_context": "steam contact with heated magnesium"},
        "model_assumptions": {"event": "representative", "sequence": "explanatory", "premise_ids": [STEAM_RULE_PREMISE]},
        "premise_ids": ["premise.elements.iupac-periodic-table", STEAM_RULE_PREMISE, METAL_PREMISE, STEAM_OXIDE_PREMISE, "premise.structure.hydrogen", "premise.structure.water", VALENCE_PREMISE, "premise.observation.magnesium-steam-products-form"],
    }


def material(structure: str, rule: str, role: str, phase: str) -> dict:
    return {"structure": structure, "context": {"kind": "reaction_role", "rule": rule, "role": role}, "phase": {"kind": phase}, "premise_ids": [MATERIAL_PREMISE]}


def candidate() -> dict:
    members = [("Calcium", "Ca"), ("Strontium", "Sr"), ("Barium", "Ba")]
    neutral = [
        {"element": symbol, "neutral_valence_electrons": 2}
        for _, symbol in [("Magnesium", "Mg"), *members]
    ] + [
        {"element": "H", "neutral_valence_electrons": 1},
        {"element": "O", "neutral_valence_electrons": 6},
    ]
    states = []
    domains = []
    for _, symbol in [("Magnesium", "Mg"), *members]:
        states.extend(
            [
                {"element": symbol, "formal_charge": 2, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 0},
                {"element": symbol, "formal_charge": 1, "non_bonding_electrons": 1, "unpaired_electrons": 1, "covalent_bond_order_sum": 0},
                {"element": symbol, "formal_charge": 0, "non_bonding_electrons": 2, "unpaired_electrons": 2, "covalent_bond_order_sum": 0},
            ]
        )
        domains.append(
            {"element": symbol, "site_formal_charge": 2, "site_local_electrons": 0, "delocalized_electrons_per_site": 2}
        )
    states.extend(
        [
            {"element": "O", "formal_charge": 0, "non_bonding_electrons": 4, "unpaired_electrons": 0, "covalent_bond_order_sum": 2},
            {"element": "O", "formal_charge": -1, "non_bonding_electrons": 6, "unpaired_electrons": 0, "covalent_bond_order_sum": 1},
            {"element": "O", "formal_charge": -2, "non_bonding_electrons": 8, "unpaired_electrons": 0, "covalent_bond_order_sum": 0},
            {"element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 1},
            {"element": "H", "formal_charge": 1, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 0},
            {"element": "H", "formal_charge": 0, "non_bonding_electrons": 1, "unpaired_electrons": 1, "covalent_bond_order_sum": 0},
        ]
    )
    applications = [application(name, symbol, product) for name, symbol in members for product in (False, True)]
    return {
        "schema_version": 1,
        "id": "alkaline-earth-water",
        "evidence": [
            {
                "id": "evidence.openstax.group2-water",
                "title": "Chemistry 2e — 18.1 Periodicity",
                "publisher": "OpenStax",
                "locator": "Group 2: The Alkaline Earth Metals",
                "reference": "https://openstax.org/books/chemistry-2e/pages/18-1-periodicity",
                "publication_date": "2019-02-14",
                "retrieved_on": "2026-07-21",
                "usage": "Representative Ca, Sr, and Ba reactions with water, hydrogen and hydroxide products, and the reactivity trend down Group 2",
            },
            {
                "id": "evidence.rsc.magnesium-steam",
                "title": "The reaction of magnesium with steam",
                "publisher": "Royal Society of Chemistry",
                "locator": "Demonstration and reaction equation",
                "reference": "https://edu.rsc.org/exhibition-chemistry/the-reaction-of-magnesium-with-steam/4012602.article",
                "retrieved_on": "2026-07-21",
                "usage": "Magnesium and steam form solid magnesium oxide and hydrogen gas",
            }
        ],
        "premises": [
            premise(
                RULE_PREMISE,
                "Ordinary water contact with the registered Ca, Sr, or Ba metallic structure has the representative outcome M + 2 H2O -> M(OH)2 + H2. Magnesium requires a hot-water or steam context and beryllium is outside this supported family.",
                ["evidence.openstax.group2-water"],
            ),
            premise(
                METAL_PREMISE,
                "ChemSpec represents each supported alkaline-earth metal fragment as an M2+ site core with two domain-owned delocalized valence electrons for explanatory execution.",
                ["evidence.chemspec.explanatory-structural-model"],
            ),
            premise(
                HYDROXIDE_PREMISE,
                "ChemSpec represents each supported alkaline-earth hydroxide formula unit as an ionic association of M2+ and two covalently bonded OH- components.",
                ["evidence.chemspec.explanatory-structural-model"],
            ),
            premise(
                "premise.observation.group2-hydrogen-evolves",
                "Hydrogen gas evolution is compatible with the supported alkaline-earth metal/water outcome.",
                ["evidence.openstax.group2-water"],
            ),
            premise(
                "premise.observation.alkaline-earth-metal-disappears",
                "Consumption of the selected Ca, Sr, or Ba metal is compatible with the supported water-contact outcome.",
                ["evidence.openstax.group2-water"],
            ),
            premise(
                VALENCE_PREMISE,
                "The listed Mg, Ca, Sr, Ba, H, and O tuples are ChemSpec's closed explanatory execution domain for these families; they are not a claim of physical mechanism.",
                ["evidence.chemspec.explanatory-structural-model"],
            ),
            premise(STEAM_RULE_PREMISE, "Steam contact with heated magnesium has the representative outcome Mg + H2O(g) -> MgO(s) + H2(g).", ["evidence.rsc.magnesium-steam"]),
            premise(STEAM_OXIDE_PREMISE, "Magnesium oxide is represented as an ionic association of Mg2+ and O2- for explanatory execution.", ["evidence.chemspec.explanatory-structural-model"]),
            premise("premise.observation.magnesium-steam-products-form", "Formation of solid magnesium oxide and hydrogen gas is compatible with the magnesium-steam outcome.", ["evidence.rsc.magnesium-steam"]),
            premise(MATERIAL_PREMISE, "Ca, Sr, and Ba are solid metals contacting liquid water and forming aqueous hydroxide plus hydrogen gas; magnesium is solid, steam is gas, magnesium oxide is solid, and hydrogen is gas.", ["evidence.openstax.group2-water", "evidence.rsc.magnesium-steam"]),
        ],
        "valence_premises": [
            {
                "premise_id": VALENCE_PREMISE,
                "neutral_valence": neutral,
                "supported_states": states,
                "metallic_domain_states": domains,
            }
        ],
        "structures": [magnesium_oxide()],
        "rules": [],
        "elements": [],
        "element_categories": [
            {
                "id": "Categories.WaterReactiveAlkalineEarth",
                "subject": "element",
                "membership": {"kind": "explicit", "members": ["Ba", "Ca", "Sr"]},
                "premise_ids": [RULE_PREMISE],
            }
            ,{
                "id": "Categories.MagnesiumSteam",
                "subject": "element",
                "membership": {"kind": "explicit", "members": ["Mg"]},
                "premise_ids": [STEAM_RULE_PREMISE],
            }
        ],
        "structural_traits": [],
        "structure_templates": [metal_template(), hydroxide_template(), magnesium_metal_template()],
        "structure_applications": applications + [{"id": "MagnesiumSteamMetal", "template": "Templates.MagnesiumSteamMetal", "arguments": {"member": "Mg"}, "formula": "Mg", "premise_ids": [STEAM_RULE_PREMISE, METAL_PREMISE]}],
        "graph_patterns": [
            {
                "id": "Patterns.AlkalineEarthWaterMetal",
                "variables": {"metal": {"atom": {"element": {"parameter": "member"}}}},
                "relationships": [
                    {"kind": "metallic_domain", "domain": "metallic", "sites": ["metal"], "delocalized_electrons": 2}
                ],
                "premise_ids": [RULE_PREMISE, METAL_PREMISE],
            },
            {
                "id": "Patterns.MagnesiumSteamMetal",
                "variables": {"metal": {"atom": {"element": {"parameter": "member"}}}},
                "relationships": [{"kind": "metallic_domain", "domain": "metallic", "sites": ["metal"], "delocalized_electrons": 2}],
                "premise_ids": [STEAM_RULE_PREMISE, METAL_PREMISE],
            },
            water_pattern(),
            steam_water_pattern(),
        ],
        "generalized_rules": [generalized_rule(), magnesium_steam_rule()],
        "macroscopic_materials": [
            *[material(f"{name}WaterMetal", "Rules.AlkalineEarthMetalWithWater", "metal", "solid") for name, _ in members],
            material("Water", "Rules.AlkalineEarthMetalWithWater", "water", "liquid"),
            *[material(f"{name}WaterHydroxide", "Rules.AlkalineEarthMetalWithWater", "hydroxide", "aqueous") for name, _ in members],
            material("Hydrogen", "Rules.AlkalineEarthMetalWithWater", "gasProduct", "gas"),
            material("MagnesiumSteamMetal", "Rules.MagnesiumWithSteam", "metal", "solid"),
            material("Water", "Rules.MagnesiumWithSteam", "water", "gas"),
            material("MagnesiumSteamOxide", "Rules.MagnesiumWithSteam", "oxide", "solid"),
            material("Hydrogen", "Rules.MagnesiumWithSteam", "gasProduct", "gas"),
        ],
    }


def main() -> None:
    CANDIDATE_DIR.mkdir(parents=True, exist_ok=True)
    output = CANDIDATE_DIR / "candidate.json"
    output.write_text(json.dumps(candidate(), indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
