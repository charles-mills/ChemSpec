"""Bind the chemist-approved breadth review to the exact checked catalogue."""

import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    checked = ROOT / "reaction-rules-check-expanded-11/catalogue.json"
    if not checked.exists():
        return
    envelope = json.loads(checked.read_text(encoding="utf-8"))
    bundle = envelope["bundle"]
    review = {
        "schema_version": 1,
        "id": "review.ai.general-reaction-families",
        "catalogue_digest": envelope["digest"],
        "reviewer": "ChemSpec project owner and chemist, assisted by OpenAI Codex",
        "reviewed_on": "2026-07-15",
        "scope": "The exact eleven-package catalogue: 118 element identities and reaction facts; ten generated C1-C10 alkane combustions; six Mg/Zn/Fe/Cu metal-displacement outcomes; four common sulfate/carbonate/hydroxide precipitation outcomes; five Group 1 + water experiences; 30 fixed-charge metal/non-oxidising-acid experiences; and the previously approved oxygen, ozone, ion-pair, acid/base, carbonate/acid, halogen displacement, silver-halide precipitation and covalent experiences.",
        "method": "Chemist-authorized host review of deterministic catalogue generation, typed context-specific activity facts, graph-trait provenance, balanced family stoichiometry, exact atom correspondence, electron and charge conservation, generalized elaboration, kernel-compatible operation sequences, finite unsupported boundaries, and exact digest binding. Representative +1, +2 and +3 metal/acid sources and the Rb/Cs water sources were expanded through the ordinary .chems pipeline; the package examples crossed catalogue inspection.",
        "sources": sorted(source["id"] for source in bundle["evidence"]),
        "premises": sorted(premise["id"] for premise in bundle["premises"]),
        "coverage_conclusion": "Approved for the 325 finite structural experiences in the application registry. Alkane combustion, metal displacement and general precipitation are generated from family algorithms and remain subject to exact catalogue case selection and kernel validation.",
        "limitation": "This is an educational theoretical model, not laboratory guidance. Oxidising acids, HF equilibrium, passivation, Lewis acid/base chemistry, condition-dependent combustion products, a universal solubility predictor, and variable-charge metal displacement are not inferred unless an exact reviewed case is present. Activity ranks are comparable only within their named series.",
    }
    path = ROOT / "catalogue/reviews/general-reaction-families.review.json"
    path.write_text(json.dumps(review, indent=2) + "\n", encoding="utf-8", newline="\n")

    promoted = ROOT / "reaction-rules-promoted-expanded-11"
    if promoted.exists():
        trusted = ROOT / "catalogue/trusted/core-chemistry"
        trusted.mkdir(parents=True, exist_ok=True)
        for name in ("catalogue.json", "catalogue.digest", "review.json", "promotion.json"):
            shutil.copyfile(promoted / name, trusted / name)
        shutil.copyfile(promoted / "review.json", ROOT / "catalogue/reviews/core-chemistry.review.json")


if __name__ == "__main__":
    main()
