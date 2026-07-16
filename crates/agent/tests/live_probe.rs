//! Temporary live probe for the mechanism escalation invocation path.

use agent::{
    ClaimMode, CodexProvider, CodexProviderConfig, CompiledClaimOutcome, MechanismProvider,
    ReactantInput, ReactionBuildRequest, ReactionClaim, compile_claim_outcome,
    compile_mechanism_request, reviewed_species_registry,
};
use chem_catalogue::TrustedCatalogue;

#[test]
#[ignore = "live probe; consumes Codex subscription"]
fn live_mechanism_probe() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let trusted = TrustedCatalogue::from_canonical_json(
        &std::fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json")).unwrap(),
        &std::fs::read(root.join("catalogue/trusted/core-chemistry/review.json")).unwrap(),
    )
    .unwrap();
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
        ],
        selected_context: None,
    };
    // Resolve the CO2 alias ambiguity the way the app dialog does.
    while let agent::RequestIdentityResolution::Ambiguous(ambiguity) =
        agent::resolve_request_identities(&request, &identities).unwrap()
    {
        request.reactants[ambiguity.reactant_index].species_id =
            Some(ambiguity.alternatives[0].id.clone());
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
                Err(error) => eprintln!("KERNEL ERR: {error}"),
            }
        }
        Err(error) => eprintln!("ERR: [{}] {:?}", error.stage(), error.to_string()),
    }
}
