//! Audits the breadth-corpus manifest against the code that actually runs:
//! for every scenario it resolves the symbolic reactants through the reviewed
//! species registry, compiles the oracle outcome, and reports the presentation
//! tier the current build can reach locally (reviewed family, escalated
//! mechanism candidate, claim-only, or an identity/compile failure).
//!
//! Use it whenever corpus expectations are edited, so expectation data cannot
//! silently drift from the product again.

use std::{collections::BTreeMap, collections::BTreeSet, env, fs, path::Path};

use agent::{
    ClaimMode, CompiledClaimOutcome, CorpusExpectedState, CorpusManifest, CorpusPresentation,
    FamilyMatchOutcome, ReactantInput, ReactionBuildRequest, ReactionClaim,
    RequestIdentityResolution, compile_claim_outcome, match_reviewed_family,
    resolve_request_identities_with_catalogue, reviewed_species_registry,
};
use chem_catalogue::TrustedCatalogue;
use chem_domain::{FormulaComposition, SpeciesRegistry};
use serde::Serialize;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = root.join("corpus/dynamic-reactions-v1.json");
    let mut manifest =
        CorpusManifest::from_json(&fs::read(&manifest_path).expect("corpus manifest"))
            .expect("valid corpus manifest");
    let write = env::args().skip(1).any(|argument| argument == "--write");
    if write {
        normalize_context_requests(&mut manifest);
    }
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
    let mut audit_results = BTreeMap::new();
    for scenario in &manifest.scenarios {
        let result = audit(
            &scenario.category,
            &scenario.reactants[1],
            &scenario.outcome_oracle,
            &identities,
            &trusted,
            &atomic,
        );
        println!("{}\t{}\t{}", scenario.id, scenario.category, result);
        audit_results.insert(scenario.id.clone(), result);
    }
    if write {
        regenerate_expectations(&mut manifest, &audit_results);
        let mut bytes = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut serializer = serde_json::Serializer::with_formatter(&mut bytes, formatter);
        manifest
            .serialize(&mut serializer)
            .expect("serialize corpus manifest");
        bytes.push(b'\n');
        fs::write(manifest_path, bytes).expect("write regenerated corpus manifest");
    }
}

fn regenerate_expectations(
    manifest: &mut CorpusManifest,
    audit_results: &BTreeMap<String, String>,
) {
    let mut escalated_scenarios = BTreeSet::new();
    for scenario in &mut manifest.scenarios {
        let reaches_escalation = audit_results
            .get(&scenario.id)
            .is_some_and(|result| result == "escalated_mechanism_candidate");
        if reaches_escalation {
            escalated_scenarios.insert(scenario.id.clone());
        }
        if reaches_escalation
            && matches!(
                scenario.expected_state,
                CorpusExpectedState::Invalid
                    | CorpusExpectedState::Ambiguous
                    | CorpusExpectedState::Unsupported
            )
        {
            scenario.expected_state = CorpusExpectedState::ModelAsserted;
            scenario.presentation = CorpusPresentation::EscalatedMechanism;
        }
    }
    for case in &mut manifest.cases {
        if escalated_scenarios.contains(&case.scenario_id)
            && case.adversarial_mutation.is_none()
            && matches!(
                case.expected_state,
                CorpusExpectedState::Invalid
                    | CorpusExpectedState::Ambiguous
                    | CorpusExpectedState::Unsupported
            )
        {
            case.expected_state = CorpusExpectedState::ModelAsserted;
            case.expected_presentation = CorpusPresentation::EscalatedMechanism;
        }
    }
}

fn normalize_context_requests(manifest: &mut CorpusManifest) {
    for scenario in &mut manifest.scenarios {
        match scenario.category.as_str() {
            "photochemical" => "AgCl".clone_into(&mut scenario.reactants[1]),
            "electrochemical" => "H2O".clone_into(&mut scenario.reactants[1]),
            _ => {}
        }
    }
    for case in &mut manifest.cases {
        case.request = case
            .request
            .replace("silver chloride + light", "silver chloride under light")
            .replace("AgCl + photon", "AgCl under light")
            .replace("water + electrical input", "water with electricity")
            .replace("H2O + electrical-context", "H2O with electricity");
    }
}

fn audit(
    category: &str,
    symbolic: &str,
    oracle: &str,
    identities: &SpeciesRegistry,
    trusted: &TrustedCatalogue,
    atomic: &BTreeMap<String, u8>,
) -> String {
    let parts = symbolic.split(" + ").collect::<Vec<_>>();
    let selected_context = match (parts.len(), category) {
        (1, "photochemical") => Some("light".to_owned()),
        (1, "electrochemical") => Some("electricity".to_owned()),
        (2, _) => None,
        _ => return format!("identity_unresolvable(one or two reactants required: {symbolic})"),
    };
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
    let mut request = ReactionBuildRequest {
        reactants: inputs,
        selected_context,
    };
    // Mirror the learner's identity selection: an alias ambiguity resolves to
    // an explicit choice in the app, so the audit picks the first compatible
    // alternative the same dialog would offer.
    for _ in 0..request.reactants.len() {
        match resolve_request_identities_with_catalogue(&request, identities, trusted) {
            Ok(RequestIdentityResolution::Ambiguous(ambiguity)) => {
                request.reactants[ambiguity.reactant_index].species_id =
                    Some(ambiguity.alternatives[0].id.clone());
            }
            Ok(RequestIdentityResolution::Resolved(resolved)) => {
                for (input, species) in request.reactants.iter_mut().zip(resolved) {
                    if let agent::OutcomeSpecies::Resolved(species) = species {
                        input.species_id = Some(species.id);
                    }
                }
                break;
            }
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
    let required_context = request.selected_context.as_deref().unwrap_or(
        "representative educational outcome under the reviewed standard-outcome premise",
    );
    let claim = serde_json::json!({
        "schema_version": 1,
        "disposition": "reaction",
        "products": products,
        "required_context": required_context,
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
