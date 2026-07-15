"""Author reproducible, context-specific reaction facts into the element catalogue.

This deliberately stores separate activity series.  It must never be replaced
with one context-free `reactivity` score.
"""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOGUE = ROOT / "catalogue/candidates/periodic-table-and-alkali-water/candidate.json"

ACTIVITY = {
    # rank is intentionally ordinal and meaningful only inside this series.
    "hydrogen_displacement": {
        "Cs": 150, "Rb": 145, "K": 140, "Ba": 135, "Sr": 130,
        "Ca": 125, "Na": 120, "Mg": 110, "Al": 100, "Mn": 90,
        "Zn": 80, "Cr": 75, "Fe": 70, "Co": 65, "Ni": 60,
        "Sn": 50, "Pb": 40, "H": 0, "Cu": -10, "Hg": -20,
        "Ag": -30, "Pt": -40, "Au": -50,
    },
    "metal_displacement": {
        "Cs": 150, "Rb": 145, "K": 140, "Ba": 135, "Sr": 130,
        "Ca": 125, "Na": 120, "Mg": 110, "Al": 100, "Mn": 90,
        "Zn": 80, "Cr": 75, "Fe": 70, "Co": 65, "Ni": 60,
        "Sn": 50, "Pb": 40, "H": 0, "Cu": -10, "Hg": -20,
        "Ag": -30, "Pt": -40, "Au": -50,
    },
    "halogen_displacement": {"F": 40, "Cl": 30, "Br": 20, "I": 10},
}

COMMON_CHARGES = {
    "Li": [1], "Na": [1], "K": [1], "Rb": [1], "Cs": [1],
    "Be": [2], "Mg": [2], "Ca": [2], "Sr": [2], "Ba": [2],
    "Al": [3], "Zn": [2], "Ag": [1],
    "F": [-1], "Cl": [-1], "Br": [-1], "I": [-1],
    "O": [-2], "S": [-2], "N": [-3], "P": [-3],
}

WATER = {
    "Li": "cold_water", "Na": "cold_water", "K": "cold_water",
    "Rb": "cold_water", "Cs": "cold_water",
    "Be": "no_modelled_reaction", "Mg": "steam_only",
    "Ca": "cold_water", "Sr": "cold_water", "Ba": "cold_water",
    # Francium is deliberately not inferred from periodic-table membership.
    "Fr": "no_modelled_reaction",
}

PREMISE = "premise.elements.context-specific-reaction-facts"


def main() -> None:
    document = json.loads(CATALOGUE.read_text(encoding="utf-8"))
    premise = {
        "id": PREMISE,
        "statement": (
            "The recorded ionic charges, water-reaction classes, and separate "
            "metal, hydrogen, and halogen displacement ranks are reviewed "
            "closed-world screening facts; a rank is comparable only within "
            "its named activity series."
        ),
        "evidence": ["evidence.openstax.chemistry-2e"],
        "review": {"status": "provisional", "reviewers": []},
        "rule_version": "1",
    }
    document["premises"] = [p for p in document["premises"] if p["id"] != PREMISE]
    document["premises"].append(premise)

    for element in document["elements"]:
        symbol = element["symbol"]
        facts: dict[str, object] = {}
        if symbol in COMMON_CHARGES:
            facts["common_ionic_charges"] = COMMON_CHARGES[symbol]
        ranks = [
            {"series": series, "rank": table[symbol], "premise_ids": [PREMISE]}
            for series, table in ACTIVITY.items()
            if symbol in table
        ]
        if ranks:
            facts["activity_ranks"] = ranks
        if symbol in WATER:
            facts["water_reactivity"] = WATER[symbol]
        if facts:
            element["reaction_facts"] = facts
        else:
            element.pop("reaction_facts", None)

    CATALOGUE.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
