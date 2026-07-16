//! Audits the breadth-corpus manifest against the code that actually runs:
//! for every scenario it resolves the symbolic reactants through the reviewed
//! species registry, compiles the oracle outcome, and reports the presentation
//! tier the current build can reach locally (reviewed family, escalated
//! mechanism candidate, claim-only, or an identity/compile failure).
//!
//! Use it whenever corpus expectations are edited, so expectation data cannot
//! silently drift from the product again.

use std::{collections::BTreeMap, fs, path::Path};

use agent::{
    ClaimMode, CompiledClaimOutcome, CorpusManifest, FamilyMatchOutcome, ReactantInput,
    ReactionBuildRequest, ReactionClaim, RequestIdentityResolution, compile_claim_outcome,
    match_reviewed_family, resolve_request_identities, reviewed_species_registry,
};
use chem_catalogue::TrustedCatalogue;
use chem_domain::{FormulaComposition, SpeciesRegistry};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = CorpusManifest::from_json(
        &fs::read(root.join("corpus/dynamic-reactions-v1.json")).expect("corpus manifest"),
    )
    .expect("valid corpus manifest");
    let trusted = TrustedCatalogue::from_canonical_json(
        &fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json"))
            .expect("trusted catalogue"),
        &fs::read(root.join("catalogue/trusted/core-chemistry/review.json")).expect("review"),
    )
    .expect("trusted catalogue attestation");
    let identities = reviewed_species_registry(&trusted).expect("identity registry");
    let atomic = trusted
        .document()
        .elements
        .iter()
        .filter_map(|element| {
            u8::try_from(element.atomic_number)
                .ok()
                .map(|number| (element.symbol.to_string(), number))
        })
        .collect::<BTreeMap<_, _>>();
    for scenario in &manifest.scenarios {
        println!(
            "{}\t{}\t{}",
            scenario.id,
            scenario.category,
            audit(
                &scenario.reactants[1],
                &scenario.outcome_oracle,
                &identities,
                &trusted,
                &atomic,
            )
        );
    }
}

fn audit(
    symbolic: &str,
    oracle: &str,
    identities: &SpeciesRegistry,
    trusted: &TrustedCatalogue,
    atomic: &BTreeMap<String, u8>,
) -> String {
    let parts = symbolic.split(" + ").collect::<Vec<_>>();
    if parts.len() != 2 {
        return format!("identity_unresolvable(two reactants required: {symbolic})");
    }
    let mut inputs = Vec::new();
    for part in parts {
        let Ok(formula) = FormulaComposition::parse(part) else {
            return format!("identity_unresolvable({part})");
        };
        let mut atoms = Vec::new();
        for (symbol, count) in formula.elements() {
            let Some(number) = atomic.get(&symbol.to_string()) else {
                return format!("identity_unresolvable(unknown element in {part})");
            };
            atoms.extend(std::iter::repeat_n(
                *number,
                usize::try_from(*count).unwrap_or(0),
            ));
        }
        inputs.push(ReactantInput {
            display: part.to_owned(),
            atomic_numbers: atoms,
            species_id: None,
        });
    }
    let second = inputs.pop().expect("two reactants");
    let first = inputs.pop().expect("two reactants");
    let mut request = ReactionBuildRequest {
        reactants: [first, second],
        selected_context: None,
    };
    // Mirror the learner's identity selection: an alias ambiguity resolves to
    // an explicit choice in the app, so the audit picks the first compatible
    // alternative the same dialog would offer.
    for _ in 0..request.reactants.len() {
        match resolve_request_identities(&request, identities) {
            Ok(RequestIdentityResolution::Ambiguous(ambiguity)) => {
                request.reactants[ambiguity.reactant_index].species_id =
                    Some(ambiguity.alternatives[0].id.clone());
            }
            Ok(RequestIdentityResolution::Resolved(_)) => break,
            Err(error) => return format!("compile_error({error})"),
        }
    }
    let Some(rhs) = oracle.split("->").nth(1) else {
        return "claim_only".to_owned();
    };
    let products = rhs
        .split(" + ")
        .filter_map(|term| term.split_whitespace().last())
        .map(|formula| {
            serde_json::json!({
                "name": formula,
                "formula": formula,
                "phase": "unknown",
                "identity_hints": []
            })
        })
        .collect::<Vec<_>>();
    let claim = serde_json::json!({
        "schema_version": 1,
        "disposition": "reaction",
        "products": products,
        "required_context": "representative educational outcome under the reviewed standard-outcome premise",
        "observations": [], "sources": [], "ambiguity": null
    });
    let claim = match ReactionClaim::from_json(
        &serde_json::to_vec(&claim).expect("claim JSON"),
        ClaimMode::Fast,
    ) {
        Ok(claim) => claim,
        Err(error) => return format!("claim_error({error})"),
    };
    match compile_claim_outcome(&request, claim, identities) {
        Err(error) => format!("compile_error({error})"),
        Ok(CompiledClaimOutcome::Static(outcome)) => {
            match match_reviewed_family(&outcome, trusted) {
                Ok(FamilyMatchOutcome::Matched(family)) => {
                    format!("reviewed_family({})", family.rule_id())
                }
                Ok(FamilyMatchOutcome::Ambiguous(rules)) => format!("family_ambiguous({rules:?})"),
                Ok(FamilyMatchOutcome::NoMatch) => "escalated_mechanism_candidate".to_owned(),
                Err(error) => format!("family_error({error})"),
            }
        }
        Ok(_) => "claim_only".to_owned(),
    }
}
