use std::{fs, path::PathBuf, str::FromStr};

use chem_catalogue::{
    BoundaryKind, CatalogueEnvelope, CatalogueErrorCode, ConditionPoint, FactProposition,
    ValidatedCatalogue,
};
use chem_domain::{
    ContentDigest, EvidenceSourceId, ExactScalar, FactId, MediumId, Phase, Quantity, SourceDecimal,
    SpeciesId, SubstanceId, UnitExpression, UnitSymbol,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct CorruptionFixture {
    base: String,
    corruptions: Vec<CorruptionExpectation>,
}

#[derive(Deserialize)]
struct CorruptionExpectation {
    mutation: String,
    expected_code: String,
    rebind_digest: bool,
    operations: Vec<MutationOperation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewAttestation {
    schema_version: u32,
    id: String,
    catalogue_digest: ContentDigest,
    reviewer: String,
    reviewed_on: String,
    scope: String,
    method: String,
    sources: std::collections::BTreeSet<EvidenceSourceId>,
    coverage_conclusion: String,
    limitation: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum MutationOperation {
    Replace { path: String, value: Value },
    Add { path: String, value: Value },
    Append { path: String, value: Value },
    AppendCopy { path: String, from: String },
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_bytes() -> Vec<u8> {
    fs::read(workspace_root().join("conformance/catalogue/silver-chloride-001.catalogue.json"))
        .expect("silver-chloride fixture should be readable")
}

fn envelope() -> CatalogueEnvelope {
    serde_json::from_slice(&fixture_bytes()).expect("fixture should deserialize")
}

fn apply_operations(document: &mut Value, operations: &[MutationOperation]) {
    for operation in operations {
        match operation {
            MutationOperation::Replace { path, value } => {
                *document
                    .pointer_mut(path)
                    .unwrap_or_else(|| panic!("replace path `{path}` should exist")) =
                    value.clone();
            }
            MutationOperation::Add { path, value } => {
                let (parent, key) = path
                    .rsplit_once('/')
                    .unwrap_or_else(|| panic!("add path `{path}` should have a parent"));
                document
                    .pointer_mut(parent)
                    .and_then(Value::as_object_mut)
                    .unwrap_or_else(|| panic!("add parent `{parent}` should be an object"))
                    .insert(key.to_owned(), value.clone());
            }
            MutationOperation::Append { path, value } => document
                .pointer_mut(path)
                .and_then(Value::as_array_mut)
                .unwrap_or_else(|| panic!("append path `{path}` should be an array"))
                .push(value.clone()),
            MutationOperation::AppendCopy { path, from } => {
                let value = document
                    .pointer(from)
                    .unwrap_or_else(|| panic!("copy source `{from}` should exist"))
                    .clone();
                document
                    .pointer_mut(path)
                    .and_then(Value::as_array_mut)
                    .unwrap_or_else(|| panic!("append-copy path `{path}` should be an array"))
                    .push(value);
            }
        }
    }
}

fn rebind_json(document: &mut Value) {
    let envelope: CatalogueEnvelope = serde_json::from_value(document.clone())
        .expect("digest-rebound mutation should preserve the wire schema");
    let digest = envelope
        .computed_digest()
        .expect("digest-rebound mutation should be canonicalizable");
    document["digest"] = Value::String(digest.to_string());
}

fn id<T: FromStr>(source: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    source.parse().expect("test identifier should be valid")
}

#[test]
fn silver_chloride_fixture_matches_schema_and_bound_digest() {
    let schema: Value = serde_json::from_slice(
        &fs::read(workspace_root().join("schemas/chem-catalogue-1.schema.json"))
            .expect("catalogue schema should be readable"),
    )
    .expect("catalogue schema should parse");
    let fixture: Value = serde_json::from_slice(&fixture_bytes()).expect("fixture should be JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("schema should compile");
    assert!(
        validator.is_valid(&fixture),
        "fixture did not satisfy catalogue schema: {:?}",
        validator.iter_errors(&fixture).collect::<Vec<_>>()
    );

    let envelope = envelope();
    assert_eq!(
        envelope.computed_digest().expect("digest should compute"),
        envelope.digest,
        "computed digest is {}",
        envelope.computed_digest().unwrap()
    );
    let loaded = ValidatedCatalogue::from_json(&fixture_bytes()).expect("fixture should validate");
    assert_eq!(loaded.digest(), envelope.digest);
}

#[test]
fn external_review_attestation_binds_the_exact_catalogue_and_every_reviewer() {
    let root = workspace_root();
    let review_bytes =
        fs::read(root.join("conformance/catalogue/silver-chloride-001.review.json")).unwrap();
    let review_json: Value = serde_json::from_slice(&review_bytes).unwrap();
    let review_schema: Value = serde_json::from_slice(
        &fs::read(root.join("schemas/chem-catalogue-review-1.schema.json")).unwrap(),
    )
    .unwrap();
    let review_validator = jsonschema::draft202012::new(&review_schema).unwrap();
    assert!(
        review_validator.is_valid(&review_json),
        "review attestation must satisfy its schema: {:?}",
        review_validator
            .iter_errors(&review_json)
            .collect::<Vec<_>>()
    );
    let review: ReviewAttestation = serde_json::from_value(review_json).unwrap();
    let envelope = envelope();
    let digest_golden =
        fs::read_to_string(root.join("conformance/catalogue/silver-chloride-001.catalogue.digest"))
            .unwrap();
    assert_eq!(review.schema_version, 1);
    assert_eq!(review.catalogue_digest, envelope.digest);
    assert_eq!(review.catalogue_digest.to_string(), digest_golden.trim());
    assert!(!review.reviewer.is_empty());
    assert_eq!(review.reviewed_on, "2026-07-13");
    assert!(!review.scope.is_empty());
    assert!(!review.method.is_empty());
    assert!(!review.coverage_conclusion.is_empty());
    assert!(!review.limitation.is_empty());
    for source in &review.sources {
        assert!(
            envelope
                .bundle
                .evidence
                .iter()
                .any(|evidence| &evidence.id == source),
            "review source `{source}` must resolve in the reviewed catalogue"
        );
    }

    let identity_reviews = envelope
        .bundle
        .elements
        .iter()
        .map(|record| &record.provenance.review)
        .chain(
            envelope
                .bundle
                .substances
                .iter()
                .map(|record| &record.provenance.review),
        )
        .chain(
            envelope
                .bundle
                .species
                .iter()
                .map(|record| &record.provenance.review),
        )
        .chain(
            envelope
                .bundle
                .media
                .iter()
                .map(|record| &record.provenance.review),
        );
    let reviewed_records = identity_reviews
        .chain(envelope.bundle.facts.iter().map(|fact| &fact.review))
        .chain(
            envelope
                .bundle
                .assumption_kinds
                .iter()
                .map(|assumption| &assumption.review),
        )
        .chain(
            envelope
                .bundle
                .coverage
                .iter()
                .map(|coverage| &coverage.review),
        );
    for metadata in reviewed_records {
        for reviewer in &metadata.reviewers {
            assert_eq!(reviewer.reference.as_deref(), Some(review.id.as_str()));
        }
    }
}

#[test]
fn quantity_schema_accepts_domain_wire_values_and_rejects_loose_objects() {
    let schema: Value = serde_json::from_slice(
        &fs::read(workspace_root().join("schemas/chem-catalogue-1.schema.json")).unwrap(),
    )
    .unwrap();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    let quantity = Quantity::new(
        SourceDecimal::parse("1.0").unwrap(),
        UnitExpression::single(UnitSymbol::from_str("kg").unwrap()),
    )
    .unwrap();
    let mut document: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    let mut density_fact = document["bundle"]["facts"][7].clone();
    density_fact["id"] = Value::String("density.water".to_owned());
    density_fact["proposition"] = serde_json::json!({
        "kind": "hasDensity",
        "substance": "water",
        "density": quantity,
    });
    document["bundle"]["facts"]
        .as_array_mut()
        .unwrap()
        .push(density_fact);
    assert!(
        validator.is_valid(&document),
        "domain Quantity serialization must satisfy the catalogue schema: {:?}",
        validator.iter_errors(&document).collect::<Vec<_>>()
    );

    let valid_density_fact = document["bundle"]["facts"][8].clone();
    document["bundle"]["facts"][8]["proposition"]["density"] = serde_json::json!({"value": 1});
    assert!(!validator.is_valid(&document));

    for (pointer, overflow) in [
        (
            "/proposition/density/dimension/mass",
            serde_json::json!(2_147_483_648_i64),
        ),
        (
            "/proposition/density/source_decimal/scale",
            serde_json::json!(4_294_967_296_u64),
        ),
        (
            "/proposition/density/source_decimal/precision/written_digits",
            serde_json::json!(4_294_967_296_u64),
        ),
        (
            "/proposition/density/source_unit/dividend/factors/0/exponent",
            serde_json::json!(-2_147_483_649_i64),
        ),
        (
            "/proposition/density/conversion_derivation/steps/0/exponent",
            serde_json::json!(2_147_483_648_i64),
        ),
    ] {
        let mut overflow_document: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
        let mut overflow_fact = valid_density_fact.clone();
        *overflow_fact.pointer_mut(pointer).unwrap() = overflow;
        overflow_document["bundle"]["facts"]
            .as_array_mut()
            .unwrap()
            .push(overflow_fact);
        assert!(!validator.is_valid(&overflow_document), "{pointer}");
    }
}

#[test]
fn silver_chloride_fixture_resolves_the_complete_teaching_domain() {
    let catalogue =
        ValidatedCatalogue::from_json(&fixture_bytes()).expect("fixture should validate");
    for symbol in ["H", "N", "O", "Na", "Cl", "Ag"] {
        assert!(
            catalogue.element(&symbol.parse().unwrap()).is_some(),
            "{symbol}"
        );
    }
    for substance in [
        "water",
        "sodium-chloride",
        "silver-nitrate",
        "sodium-nitrate",
        "silver-chloride",
        "sodium-ion",
        "chloride-ion",
        "silver-ion",
        "nitrate-ion",
    ] {
        assert!(catalogue.substance(&id::<SubstanceId>(substance)).is_some());
    }
    for species in [
        "sodium-chloride.aq",
        "silver-nitrate.aq",
        "sodium-nitrate.aq",
        "silver-chloride.s",
        "sodium.aq.plus",
        "chloride.aq.minus",
        "silver.aq.plus",
        "nitrate.aq.minus",
    ] {
        assert!(catalogue.species(&id::<SpeciesId>(species)).is_some());
    }
    assert_eq!(
        catalogue
            .medium_by_alias("aqueous")
            .map(|medium| medium.id.as_str()),
        Some("water")
    );
    assert_eq!(
        catalogue
            .substance_by_alias("AgCl")
            .map(|substance| substance.id.as_str()),
        Some("silver-chloride")
    );

    let chloride = id::<SpeciesId>("chloride.aq.minus");
    let room = ConditionPoint {
        temperature_kelvin: ExactScalar::new(5963.into(), 20.into()).unwrap(),
        pressure_pascal: ExactScalar::from_integer(101_325),
        medium: id::<MediumId>("water"),
        phase: Some(Phase::Aqueous),
    };
    let applicable = catalogue.applicable_facts_for_species(&chloride, &room);
    assert!(
        applicable
            .iter()
            .any(|fact| matches!(fact.proposition, FactProposition::Dissociates { .. }))
    );
    assert!(
        catalogue
            .fact(&id::<FactId>("solubility.silver-chloride"))
            .is_some()
    );
    assert!(
        catalogue
            .fact(&id::<FactId>("observation.silver-chloride-colour"))
            .is_some()
    );
    for evidence in [
        "iupac.periodic-table",
        "openstax.reaction-classification",
        "openstax.solubility",
        "openstax.precipitation",
        "pubchem.silver-chloride",
    ] {
        assert!(
            catalogue
                .evidence(&id::<EvidenceSourceId>(evidence))
                .is_some()
        );
    }
    assert!(catalogue.document().coverage.is_empty());
}

#[test]
fn every_identity_and_fact_is_a_resolvable_provenance_premise() {
    let catalogue =
        ValidatedCatalogue::from_json(&fixture_bytes()).expect("fixture should validate");
    let document = catalogue.document();
    let identity_count = document.elements.len()
        + document.substances.len()
        + document.species.len()
        + document.media.len();

    let identity_premises = document
        .elements
        .iter()
        .map(|record| &record.provenance)
        .chain(document.substances.iter().map(|record| &record.provenance))
        .chain(document.species.iter().map(|record| &record.provenance))
        .chain(document.media.iter().map(|record| &record.provenance));
    for provenance in identity_premises {
        let premise = catalogue
            .premise(&provenance.id)
            .expect("identity premise should resolve by stable fact ID");
        assert_eq!(premise.id(), &provenance.id);
        assert!(!premise.evidence().is_empty());
        assert_eq!(
            premise.review().status,
            chem_catalogue::ReviewStatus::Reviewed
        );
        assert!(!premise.rule_version().is_empty());
    }
    for fact in &document.facts {
        assert_eq!(catalogue.premise(&fact.id).unwrap().id(), &fact.id);
    }
    assert_eq!(identity_count, 25);
}

#[test]
fn condition_domains_do_not_extrapolate_across_boundaries() {
    let catalogue =
        ValidatedCatalogue::from_json(&fixture_bytes()).expect("fixture should validate");
    let fact = catalogue
        .fact(&id::<FactId>("solubility.silver-chloride"))
        .unwrap();
    let at_boundary = ConditionPoint {
        temperature_kelvin: ExactScalar::new(5963.into(), 20.into()).unwrap(),
        pressure_pascal: ExactScalar::from_integer(101_325),
        medium: id::<MediumId>("water"),
        phase: Some(Phase::Solid),
    };
    assert!(fact.condition.contains(&at_boundary));
    let mut outside = at_boundary;
    outside.temperature_kelvin = ExactScalar::new(5964.into(), 20.into()).unwrap();
    assert!(!fact.condition.contains(&outside));
    let range = fact.condition.temperature_kelvin.as_ref().unwrap();
    assert_eq!(range.minimum_bound, BoundaryKind::Inclusive);
    assert_eq!(range.maximum_bound, BoundaryKind::Inclusive);
}

#[test]
fn species_fact_lookup_intersects_the_species_implicit_phase() {
    let mut envelope = envelope();
    envelope.bundle.facts[0].condition.phases = None;
    envelope.digest = envelope.computed_digest().unwrap();
    let catalogue = envelope.validate().unwrap();
    let species = id::<SpeciesId>("sodium-chloride.aq");
    let wrong_phase = ConditionPoint {
        temperature_kelvin: ExactScalar::new(5963.into(), 20.into()).unwrap(),
        pressure_pascal: ExactScalar::from_integer(101_325),
        medium: id::<MediumId>("water"),
        phase: Some(Phase::Gas),
    };
    assert!(
        catalogue
            .applicable_facts_for_species(&species, &wrong_phase)
            .is_empty()
    );

    let mut implicit_phase = wrong_phase;
    implicit_phase.phase = None;
    assert!(
        catalogue
            .applicable_facts_for_species(&species, &implicit_phase)
            .iter()
            .any(|fact| fact.id.as_str() == "dissociation.sodium-chloride")
    );
}

#[test]
fn every_corrupt_catalogue_is_a_typed_system_error() {
    let expected: CorruptionFixture = serde_json::from_slice(
        &fs::read(
            workspace_root().join("conformance/catalogue/catalogue-corruptions-001.input.json"),
        )
        .expect("corruption fixture should be readable"),
    )
    .expect("corruption fixture should parse");
    assert_eq!(expected.base, "silver-chloride-001.catalogue.json");
    let all_codes = expected
        .corruptions
        .iter()
        .map(|fixture| fixture.expected_code.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        all_codes.len(),
        17,
        "every catalogue error code needs an oracle"
    );

    for fixture in &expected.corruptions {
        let mut corrupt: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
        apply_operations(&mut corrupt, &fixture.operations);
        if fixture.rebind_digest {
            rebind_json(&mut corrupt);
        }
        let bytes = serde_json::to_vec(&corrupt).unwrap();
        let error = ValidatedCatalogue::from_json(&bytes).expect_err("corrupt bundle must fail");
        assert!(error.is_system_error());
        assert_eq!(
            error.diagnostic_code(),
            fixture.expected_code,
            "{}: {error}",
            fixture.mutation
        );
    }
}

#[test]
fn semantic_mutation_changes_the_digest_and_stale_binding_fails() {
    let original = envelope();
    let mut mutated = original.clone();
    mutated.bundle.created.notes = Some("semantically changed".to_owned());
    let changed = mutated.computed_digest().unwrap();
    assert_ne!(changed, original.digest);
    let error = mutated.validate().expect_err("stale digest must fail");
    assert_eq!(error.code(), CatalogueErrorCode::DigestMismatch);

    assert_eq!(
        ContentDigest::from_str(&original.digest.to_string()).unwrap(),
        original.digest
    );
}

#[test]
fn digest_binds_each_semantic_catalogue_record_category() {
    let original = envelope();
    let mut mutations = Vec::new();

    let mut metadata = original.clone();
    metadata.bundle.name = "chemspec.changed".to_owned();
    mutations.push(metadata);
    let mut element = original.clone();
    element.bundle.elements[0].name = "protium".to_owned();
    mutations.push(element);
    let mut substance = original.clone();
    substance.bundle.substances[0]
        .aliases
        .push("oxidane".to_owned());
    mutations.push(substance);
    let mut species = original.clone();
    species.bundle.species[0].id = id("water.liquid.changed");
    mutations.push(species);
    let mut medium = original.clone();
    medium.bundle.media[0]
        .aliases
        .push("water-medium".to_owned());
    mutations.push(medium);
    let mut fact = original.clone();
    fact.bundle.facts[7].rule_version = "observation-2".to_owned();
    mutations.push(fact);
    let mut evidence = original.clone();
    evidence.bundle.evidence[0].locator = "changed locator".to_owned();
    mutations.push(evidence);
    let mut provenance = original.clone();
    provenance.bundle.elements[0].provenance.rule_version = "element-identity-2".to_owned();
    mutations.push(provenance);

    for mutation in mutations {
        assert_ne!(mutation.computed_digest().unwrap(), original.digest);
    }
}

#[test]
fn canonical_digest_ignores_record_and_reaction_side_order() {
    let original = envelope();
    let mut reordered = original.clone();
    reordered.bundle.elements.reverse();
    reordered.bundle.substances.reverse();
    reordered.bundle.species.reverse();
    reordered.bundle.facts.reverse();
    reordered.bundle.evidence.reverse();
    let dissociation_index = reordered
        .bundle
        .facts
        .iter()
        .position(|fact| fact.id.as_str() == "dissociation.sodium-chloride")
        .unwrap();
    let FactProposition::Dissociates { products, .. } =
        &mut reordered.bundle.facts[dissociation_index].proposition
    else {
        unreachable!()
    };
    products.reverse();
    assert_eq!(reordered.computed_digest().unwrap(), original.digest);
}
