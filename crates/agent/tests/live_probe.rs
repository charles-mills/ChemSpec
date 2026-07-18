//! Temporary live probe for the mechanism escalation invocation path.

use agent::{
    ClaimMode, CodexProvider, CodexProviderConfig, CompiledClaimOutcome,
    MechanismEscalationOutcome, MechanismProvider, ReactantInput, ReactionBuildRequest,
    ReactionClaim, compile_claim_outcome, compile_mechanism_request, derive_mechanism,
    resolve_request_identities_with_catalogue, reviewed_species_registry,
};
use chem_catalogue::TrustedCatalogue;

fn trusted() -> TrustedCatalogue {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    TrustedCatalogue::from_canonical_json(
        &std::fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json")).unwrap(),
    )
    .unwrap()
}

#[test]
#[ignore = "live probe; consumes Codex subscription"]
fn live_mechanism_probe() {
    let trusted = trusted();
    let identities = reviewed_species_registry(&trusted).unwrap();
    let claim = serde_json::json!({
        "schema_version": 1,
        "disposition": "reaction",
        "products": [
            {"name":"carbon","formula":"C","phase":"solid","identity_hints":[]},
            {"name":"magnesium oxide","formula":"MgO","phase":"solid","identity_hints":[]}
        ],
        "required_context": "Burning magnesium in a carbon dioxide atmosphere",
        "observations": [], "sources": [], "ambiguity": null
    });
    let claim =
        ReactionClaim::from_json(&serde_json::to_vec(&claim).unwrap(), ClaimMode::Fast).unwrap();
    let mut request = ReactionBuildRequest {
        reactants: [
            ReactantInput {
                display: "Mg".into(),
                atomic_numbers: vec![12],
                species_id: None,
            },
            ReactantInput {
                display: "CO2".into(),
                atomic_numbers: vec![6, 8, 8],
                species_id: None,
            },
        ]
        .to_vec(),
        selected_context: None,
    };
    if let agent::RequestIdentityResolution::Resolved(resolved) =
        resolve_request_identities_with_catalogue(&request, &identities, &trusted).unwrap()
    {
        for (input, species) in request.reactants.iter_mut().zip(resolved) {
            if let agent::OutcomeSpecies::Resolved(species) = species {
                input.species_id = Some(species.id);
            }
        }
    }
    let CompiledClaimOutcome::Static(outcome) =
        compile_claim_outcome(&request, claim, &identities).unwrap()
    else {
        panic!("static outcome expected")
    };
    let context = compile_mechanism_request(&outcome, &trusted)
        .unwrap()
        .expect("all species structured");
    eprintln!(
        "request bytes: {}",
        serde_json::to_vec(context.request()).unwrap().len()
    );
    let mut provider = CodexProvider::new(CodexProviderConfig::from_environment());
    match provider.propose(context.request(), None) {
        Ok(response) => {
            eprintln!(
                "OK: {} mappings, {} operations",
                response.mapping.len(),
                response.operations.len()
            );
            match agent::validate_escalated_response(outcome.clone(), &response, &trusted) {
                Ok(animated) => eprintln!("KERNEL OK: {} frames", animated.frames().frames().len()),
                Err(error) => panic!("KERNEL ERR: {error}"),
            }
        }
        Err(error) => panic!(
            "ERR: [{:?}/{}] {:?}",
            error.kind(),
            error.context(),
            error.to_string()
        ),
    }
}

#[test]
#[ignore = "live probe; consumes Codex subscription"]
fn live_reactant_structure_escalation_probe() {
    let trusted = trusted();
    let identities = reviewed_species_registry(&trusted).unwrap();
    let claim = ReactionClaim::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"carbon dioxide","formula":"CO2","phase":"gas","identity_hints":[]},
                {"name":"water","formula":"H2O","phase":"gas","identity_hints":[]}
            ],
            "required_context": "Complete combustion in oxygen",
            "observations": [], "sources": [], "ambiguity": null
        }))
        .unwrap(),
        ClaimMode::Fast,
    )
    .unwrap();
    let request = ReactionBuildRequest {
        reactants: [
            ReactantInput {
                display: "CH4".into(),
                atomic_numbers: vec![6, 1, 1, 1, 1],
                species_id: None,
            },
            ReactantInput {
                display: "O2".into(),
                atomic_numbers: vec![8, 8],
                species_id: None,
            },
        ]
        .to_vec(),
        selected_context: None,
    };
    let CompiledClaimOutcome::Static(outcome) =
        compile_claim_outcome(&request, claim, &identities).unwrap()
    else {
        panic!("static outcome expected")
    };
    let mut provider = CodexProvider::new(CodexProviderConfig::from_environment());
    match derive_mechanism(outcome, &trusted, &mut provider) {
        MechanismEscalationOutcome::Animated(animated) => eprintln!(
            "KERNEL OK: reactant structure escalation produced {} frames",
            animated.frames().frames().len()
        ),
        MechanismEscalationOutcome::Unavailable {
            attempts,
            diagnostic,
            ..
        } => panic!("KERNEL ERR after {attempts} attempts: {diagnostic}"),
    }
}
