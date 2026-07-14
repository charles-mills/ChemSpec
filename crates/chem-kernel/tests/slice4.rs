use std::{collections::BTreeSet, fs, path::PathBuf};

use chem_catalogue::{
    AssumptionGoalKind, AssumptionKindRecord, AssumptionPropositionKind, AssumptionStageScope,
    AssumptionTargetKind, CatalogueEnvelope, ConditionDomain, EvidenceSource, FactProposition,
    FactRecord, PublicationKind, ReviewMetadata, ReviewStatus, SafetyClassification,
    ValidatedCatalogue,
};
use chem_domain::{
    Dimension, ElementId, EvidenceSourceId, MaterialForm, Phase, Quantity, SourceDecimal,
    UnitExpression, UnitProduct,
};
use chem_kernel::{
    AssumptionApplicability, AssumptionTarget, AssumptionUsage, ElaborationStatus, elaborate,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct ConstructorFixture {
    constructors: Vec<ConstructorCase>,
}

#[derive(Deserialize)]
struct ConstructorCase {
    declaration: String,
    expected_form: String,
}

#[derive(Deserialize)]
struct SpeciesFixture {
    cases: Vec<SpeciesCase>,
}

#[derive(Deserialize)]
struct SpeciesCase {
    declaration: String,
    expected_species: String,
    requires_molar_mass: bool,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalogue() -> ValidatedCatalogue {
    ValidatedCatalogue::from_json(
        &fs::read(
            workspace_root().join("conformance/catalogue/silver-chloride-001.catalogue.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn canonical_source() -> String {
    fs::read_to_string(workspace_root().join("conformance/materials/initial-materials-001.chems"))
        .unwrap()
}

#[expect(
    clippy::too_many_lines,
    reason = "the synthetic catalogue fixture keeps all test-only provenance and premises visible"
)]
fn extended_catalogue() -> ValidatedCatalogue {
    let mut envelope: CatalogueEnvelope = serde_json::from_slice(
        &fs::read(
            workspace_root().join("conformance/catalogue/silver-chloride-001.catalogue.json"),
        )
        .unwrap(),
    )
    .unwrap();
    envelope.bundle.publication = PublicationKind::Working;
    let synthetic_evidence_id: EvidenceSourceId = "synthetic.slice4".parse().unwrap();
    envelope.bundle.evidence.push(EvidenceSource {
        id: synthetic_evidence_id.clone(),
        title: "Synthetic Slice 4 test premises".to_owned(),
        publisher: "ChemSpec test suite".to_owned(),
        locator: "Generated test fixture".to_owned(),
        reference: "urn:chemspec:test:slice4".to_owned(),
        publication_date: None,
        retrieved_on: "2026-07-14".to_owned(),
        usage: "Synthetic values used only to exercise elaboration paths.".to_owned(),
        review_notes: Some("Not scientific evidence; fixture data only.".to_owned()),
    });
    let evidence = BTreeSet::from([synthetic_evidence_id]);
    let review = ReviewMetadata {
        status: ReviewStatus::Provisional,
        reviewers: Vec::new(),
    };
    for (id, element, value) in [
        ("atomic-mass.h", 1_u8, "1.008"),
        ("atomic-mass.o", 8, "15.999"),
        ("atomic-mass.na", 11, "22.990"),
        ("atomic-mass.cl", 17, "35.45"),
    ] {
        envelope.bundle.facts.push(FactRecord {
            id: id.parse().unwrap(),
            proposition: FactProposition::HasAtomicMass {
                element: ElementId::new(element.into()).unwrap(),
                relative_atomic_mass: SourceDecimal::parse(value).unwrap(),
            },
            condition: ConditionDomain::default(),
            evidence: evidence.clone(),
            review: review.clone(),
            rule_version: "test-atomic-mass-1".to_owned(),
        });
    }
    let density = Quantity::new(
        SourceDecimal::parse("1.000").unwrap(),
        UnitExpression::quotient(
            UnitProduct::new(vec![chem_domain::UnitPower::parse("kg", 1).unwrap()]),
            vec![UnitProduct::new(vec![
                chem_domain::UnitPower::parse("L", 1).unwrap(),
            ])],
        ),
    )
    .unwrap();
    envelope.bundle.facts.push(FactRecord {
        id: "density.water".parse().unwrap(),
        proposition: FactProposition::HasDensity {
            substance: "water".parse().unwrap(),
            density: Box::new(density),
        },
        condition: ConditionDomain::default(),
        evidence: evidence.clone(),
        review: review.clone(),
        rule_version: "test-density-1".to_owned(),
    });
    let mut gas = envelope.bundle.species[0].clone();
    gas.id = "water.gas".parse().unwrap();
    gas.phase = Phase::Gas;
    gas.provenance.id = "identity.species.water-gas".parse().unwrap();
    gas.provenance.evidence.clone_from(&evidence);
    gas.provenance.review.clone_from(&review);
    "synthetic-slice4-1".clone_into(&mut gas.provenance.rule_version);
    envelope.bundle.species.push(gas);
    envelope.bundle.media[0].supported_phases.insert(Phase::Gas);
    envelope.bundle.assumption_kinds.push(AssumptionKindRecord {
        id: "idealGas".parse().unwrap(),
        version: "1".to_owned(),
        proposition: AssumptionPropositionKind::IdealGasBehaviour,
        required_target: AssumptionTargetKind::Species,
        stage_scope: AssumptionStageScope::Initial,
        condition: ConditionDomain {
            phases: Some(BTreeSet::from([Phase::Gas])),
            ..ConditionDomain::default()
        },
        permitted_goals: BTreeSet::from([AssumptionGoalKind::GasState]),
        explanation: "Synthetic test-only ideal-gas admission.".to_owned(),
        safety: SafetyClassification::EducationalModel,
        evidence: evidence.clone(),
        review: review.clone(),
    });
    for (id, proposition, goal, target, stage_scope) in [
        (
            "phaseVessel",
            AssumptionPropositionKind::IdealFiltration,
            AssumptionGoalKind::PhasePartition,
            AssumptionTargetKind::Vessel,
            AssumptionStageScope::Initial,
        ),
        (
            "phaseStage",
            AssumptionPropositionKind::IdealFiltration,
            AssumptionGoalKind::PhasePartition,
            AssumptionTargetKind::Stage,
            AssumptionStageScope::SingleStage,
        ),
        (
            "phaseMaterial",
            AssumptionPropositionKind::NegligibleVolumeChange,
            AssumptionGoalKind::VolumeComposition,
            AssumptionTargetKind::Material,
            AssumptionStageScope::Initial,
        ),
    ] {
        envelope.bundle.assumption_kinds.push(AssumptionKindRecord {
            id: id.parse().unwrap(),
            version: "1".to_owned(),
            proposition,
            required_target: target,
            stage_scope,
            condition: ConditionDomain {
                phases: Some(BTreeSet::from([Phase::Liquid])),
                ..ConditionDomain::default()
            },
            permitted_goals: BTreeSet::from([goal]),
            explanation: "Synthetic phase-constrained assumption.".to_owned(),
            safety: SafetyClassification::EducationalModel,
            evidence: evidence.clone(),
            review: review.clone(),
        });
    }
    let mut unconstrained = envelope.bundle.assumption_kinds.last().unwrap().clone();
    unconstrained.id = "unconstrainedMaterial".parse().unwrap();
    unconstrained.condition = ConditionDomain::default();
    envelope.bundle.assumption_kinds.push(unconstrained);
    envelope.bundle.assumption_kinds.push(AssumptionKindRecord {
        id: "idealGasEnvironment".parse().unwrap(),
        version: "1".to_owned(),
        proposition: AssumptionPropositionKind::IdealGasBehaviour,
        required_target: AssumptionTargetKind::Environment,
        stage_scope: AssumptionStageScope::Initial,
        condition: ConditionDomain {
            phases: Some(BTreeSet::from([Phase::Gas])),
            ..ConditionDomain::default()
        },
        permitted_goals: BTreeSet::from([AssumptionGoalKind::GasState]),
        explanation: "Synthetic test-only environment ideal-gas admission.".to_owned(),
        safety: SafetyClassification::EducationalModel,
        evidence: evidence.clone(),
        review: review.clone(),
    });
    envelope.bundle.facts.push(FactRecord {
        id: "phase.water-liquid".parse().unwrap(),
        proposition: FactProposition::HasPhase {
            substance: "water".parse().unwrap(),
            phase: Phase::Liquid,
        },
        condition: ConditionDomain::default(),
        evidence,
        review,
        rule_version: "synthetic-phase-1".to_owned(),
    });
    envelope.digest = envelope.computed_digest().unwrap();
    envelope.validate().unwrap()
}

fn replace_first_material(source: &str, declaration: &str) -> String {
    source.replace(
        "    silverNitrate := 50 mL of 0.100 mol/L AgNO3(aq)",
        declaration,
    )
}

#[test]
fn canonical_source_elaborates_to_fully_resolved_solution_hir() {
    let result = elaborate(&canonical_source(), &catalogue());
    assert!(result.source_diagnostics.is_empty());
    assert!(
        result.diagnostics.iter().all(|diagnostic| {
            diagnostic.code == "CHEMS-T016" && diagnostic.severity == chems_lang::Severity::Warning
        }),
        "{:#?}",
        result.diagnostics
    );
    let typed = result.typed.unwrap();
    assert_eq!(typed.language_version, 1);
    assert_eq!(typed.catalogue.name, "ChemSpec.Aqueous");
    assert_eq!(typed.environment.pressure.dimension(), Dimension::PRESSURE);
    assert_eq!(typed.materials.len(), 2);
    assert_eq!(typed.vessels.len(), 1);
    assert_eq!(typed.procedure.len(), 3);
    assert!(
        typed
            .procedure
            .iter()
            .all(|step| step.resulting_stage.to_string().len() == 64)
    );
    assert!(
        typed
            .source_origins
            .keys()
            .any(|key| key.starts_with("stage:"))
    );
    for material in &typed.materials {
        assert_eq!(material.required_premises.len(), 2);
        let MaterialForm::Solution {
            analytical_species,
            total_volume,
            analytical_concentration,
            analytical_amount,
            ..
        } = &material.form
        else {
            panic!("canonical input must select Solution");
        };
        assert_eq!(analytical_species.phase, chem_domain::Phase::Aqueous);
        assert_eq!(total_volume.dimension(), Dimension::VOLUME);
        assert_eq!(
            analytical_concentration.dimension(),
            Dimension::CONCENTRATION
        );
        assert_eq!(analytical_amount.dimension, Dimension::AMOUNT);
        assert_eq!(
            analytical_amount.canonical_value,
            chem_domain::ExactScalar::new(5.into(), 1000.into()).unwrap()
        );
    }
}

#[test]
fn canonical_source_matches_the_checked_in_typed_hir() {
    let typed = elaborate(&canonical_source(), &catalogue()).typed.unwrap();
    let mut actual = serde_json::to_string_pretty(&typed).unwrap();
    actual.push('\n');
    let expected = fs::read_to_string(
        workspace_root().join("conformance/materials/initial-materials-001.hir.json"),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn typed_quantity_and_species_component_sources_elaborate_without_holes() {
    for path in [
        "conformance/quantities-types/typed-conditions-001.chems",
        "conformance/formula-species/species-resolution-001.chems",
    ] {
        let source = fs::read_to_string(workspace_root().join(path)).unwrap();
        let result = elaborate(&source, &catalogue());
        assert!(result.source_diagnostics.is_empty(), "{path}");
        assert!(result.typed.is_some(), "{path}: {:#?}", result.diagnostics);
    }
}

#[test]
fn source_and_catalogue_failures_keep_distinct_semantic_classes() {
    let source = canonical_source();
    let cases = [
        (
            source.replace("50 mL of 0.100 mol/L", "50 g of 0.100 mol/L"),
            "CHEMS-T010",
            ElaborationStatus::IllTyped,
        ),
        (
            source.replace("50 mL of 0.100 mol/L", "0 mL of 0.100 mol/L"),
            "CHEMS-T011",
            ElaborationStatus::IllTyped,
        ),
        (
            source.replace("AgNO3(aq)", "AgBr(aq)"),
            "CHEMS-T007",
            ElaborationStatus::IllTyped,
        ),
        (
            source.replace("AgNO3(aq)", "Ag2O(aq)"),
            "CHEMS-T008",
            ElaborationStatus::Unsupported,
        ),
        (
            source.replace(
                "use catalog ChemSpec.Aqueous@1",
                "use catalog Other.Bundle@1",
            ),
            "CHEMS-C018",
            ElaborationStatus::IllTyped,
        ),
    ];
    for (source, code, status) in cases {
        let result = elaborate(&source, &catalogue());
        assert!(result.typed.is_none());
        assert!(result.source_diagnostics.is_empty());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code && diagnostic.status == Some(status)),
            "{code}: {:#?}",
            result.diagnostics
        );
    }

    for declaration in [
        "    silverNitrate := 1 g of NaCl(aq)",
        "    silverNitrate := 1 mL of H2O(l)",
    ] {
        let result = elaborate(
            &replace_first_material(&canonical_source(), declaration),
            &catalogue(),
        );
        assert!(result.typed.is_none());
        assert!(result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "CHEMS-T012"
                && diagnostic.status == Some(ElaborationStatus::Unsupported)
        }));
    }
}

#[test]
fn shared_namespace_and_operand_kinds_are_checked_before_execution() {
    let duplicate = canonical_source().replace(
        "reaction := open vessel 250 mL",
        "silverNitrate := open vessel 250 mL",
    );
    let result = elaborate(&duplicate, &catalogue());
    assert!(result.typed.is_none());
    assert_eq!(result.diagnostics[0].code, "CHEMS-T003");
    assert_eq!(result.diagnostics[0].related_spans.len(), 1);

    let wrong_operand = canonical_source().replace(
        "place silverNitrate in reaction",
        "place reaction in reaction",
    );
    let result = elaborate(&wrong_operand, &catalogue());
    assert!(result.typed.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-T004")
    );
}

#[test]
fn every_initial_material_constructor_is_dimension_directed_and_exact() {
    let catalogue = extended_catalogue();
    let fixture: ConstructorFixture = serde_json::from_slice(
        &fs::read(workspace_root().join("conformance/materials/initial-materials-001.input.json"))
            .unwrap(),
    )
    .unwrap();
    for case in fixture.constructors {
        let result = elaborate(
            &replace_first_material(&canonical_source(), &case.declaration),
            &catalogue,
        );
        assert!(result.typed.is_some(), "{:#?}", result.diagnostics);
        let value = serde_json::to_value(&result.typed.unwrap().materials[0]).unwrap();
        assert_eq!(value["form"]["kind"], case.expected_form);
    }

    let prepared = elaborate(
        &replace_first_material(
            &canonical_source(),
            "    silverNitrate := prepared\n      1 mmol of NaCl(aq)\n      2 mmol of NaCl(aq)",
        ),
        &catalogue,
    )
    .typed
    .unwrap();
    let MaterialForm::Prepared { components } = &prepared.materials[0].form else {
        panic!("prepared syntax must select the prepared constructor");
    };
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].source_component_indices, [0, 1]);
    assert_eq!(
        components[0]
            .analytical
            .amount
            .as_ref()
            .unwrap()
            .canonical_value,
        chem_domain::ExactScalar::new(3.into(), 1000.into()).unwrap()
    );

    let liquid = elaborate(
        &replace_first_material(&canonical_source(), "    silverNitrate := 1 mL of H2O(l)"),
        &catalogue,
    )
    .typed
    .unwrap();
    let MaterialForm::LiquidSampleByVolume { amount, .. } = &liquid.materials[0].form else {
        unreachable!()
    };
    assert_eq!(amount.derivation.premises.len(), 3);
    assert!(
        amount
            .derivation
            .premises
            .iter()
            .any(|id| id.to_string() == "density.water")
    );

    let distinct = elaborate(
        &replace_first_material(
            &canonical_source(),
            "    silverNitrate := prepared\n      1 mL of H2O(l)\n      2 mmol of NaCl(aq)",
        ),
        &catalogue,
    )
    .typed
    .unwrap();
    let material_id = distinct.materials[0].id;
    let first =
        &distinct.source_origins[&format!("material:{material_id}:component:0:derived:amount")];
    let second =
        &distinct.source_origins[&format!("material:{material_id}:component:1:derived:amount")];
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_ne!(first, second);
}

#[test]
fn formula_species_fixture_resolves_identity_inventory_and_molar_mass() {
    let fixture: SpeciesFixture = serde_json::from_slice(
        &fs::read(
            workspace_root().join("conformance/formula-species/species-resolution-001.input.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for case in fixture.cases {
        let result = elaborate(
            &replace_first_material(&canonical_source(), &case.declaration),
            &extended_catalogue(),
        );
        assert!(result.typed.is_some(), "{:#?}", result.diagnostics);
        let material = &result.typed.unwrap().materials[0];
        assert_eq!(
            material.analytical_inventory[0].species.id.to_string(),
            case.expected_species
        );
        assert_eq!(
            matches!(material.form, MaterialForm::SampleByMass { .. }),
            case.requires_molar_mass
        );
    }
}

#[test]
fn explicit_ideal_gas_assumption_can_discharge_and_trace_the_model_premise() {
    let source = replace_first_material(
        &canonical_source().replace(
            "\n  given\n",
            "\n  assuming\n    idealGas for silverNitrate at initial\n\n  given\n",
        ),
        "    silverNitrate := 1 L of H2O(g)",
    );
    let result = elaborate(&source, &extended_catalogue());
    assert!(result.typed.is_some(), "{:#?}", result.diagnostics);
    let typed = result.typed.unwrap();
    assert!(matches!(
        typed.materials[0].form,
        MaterialForm::GasSampleByVolume { .. }
    ));
    assert!(typed.materials[0].required_premises.len() == 1);
    let used_premise = typed.materials[0]
        .required_assumptions
        .iter()
        .next()
        .unwrap();
    assert_eq!(used_premise, &typed.assumptions[0].id);
    assert_eq!(
        typed.assumptions[0].usage,
        AssumptionUsage::UsedInMaterialElaboration
    );
    let MaterialForm::GasSampleByVolume { amount, .. } = &typed.materials[0].form else {
        unreachable!()
    };
    assert!(amount.derivation.assumptions.contains(used_premise));
    assert!(matches!(
        &typed.assumptions[0].target,
        AssumptionTarget::Species { species, .. } if species.to_string() == "water.gas"
    ));
    assert!(
        typed
            .source_origins
            .keys()
            .any(|key| key == "assumption:0:idealGas")
    );

    let unsupported = elaborate(
        &replace_first_material(&canonical_source(), "    silverNitrate := 1 L of H2O(g)"),
        &extended_catalogue(),
    );
    assert!(unsupported.typed.is_none());
    assert!(unsupported.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T012" && diagnostic.status == Some(ElaborationStatus::Unsupported)
    }));
}

#[test]
fn assumption_instances_are_distinct_and_environment_gas_admission_is_applicable() {
    let duplicated = replace_first_material(
        &canonical_source().replace(
            "\n  given\n",
            "\n  assuming\n    idealGas for silverNitrate at initial\n    idealGas for silverNitrate at initial\n\n  given\n",
        ),
        "    silverNitrate := 1 L of H2O(g)",
    );
    let result = elaborate(&duplicated, &extended_catalogue());
    let typed = result
        .typed
        .expect("duplicate declarations remain distinct premises");
    assert_ne!(typed.assumptions[0].id, typed.assumptions[1].id);
    assert_eq!(
        typed.assumptions[0].usage,
        AssumptionUsage::UsedInMaterialElaboration
    );
    assert_eq!(typed.assumptions[1].usage, AssumptionUsage::Unused);
    assert_eq!(typed.materials[0].required_assumptions.len(), 1);

    let environment = replace_first_material(
        &canonical_source().replace(
            "\n  given\n",
            "\n  assuming\n    idealGasEnvironment at initial\n\n  given\n",
        ),
        "    silverNitrate := 1 L of H2O(g)",
    );
    let result = elaborate(&environment, &extended_catalogue());
    assert!(result.typed.is_some(), "{:#?}", result.diagnostics);
    assert!(matches!(
        result.typed.unwrap().assumptions[0].target,
        AssumptionTarget::Environment
    ));

    let liquid_target = replace_first_material(
        &canonical_source().replace(
            "\n  given\n",
            "\n  assuming\n    idealGas for silverNitrate at initial\n\n  given\n",
        ),
        "    silverNitrate := 1 mL of H2O(l)",
    );
    let result = elaborate(&liquid_target, &extended_catalogue());
    assert!(result.typed.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T013" && diagnostic.status == Some(ElaborationStatus::Invalid)
    }));
}

#[test]
fn explicit_species_phase_contradiction_is_invalid_not_unsupported() {
    let source =
        replace_first_material(&canonical_source(), "    silverNitrate := 1 mmol of H2O(s)");
    let result = elaborate(&source, &extended_catalogue());
    assert!(result.typed.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T009" && diagnostic.status == Some(ElaborationStatus::Invalid)
    }));
}

#[test]
fn future_phase_assumptions_defer_and_mixed_material_targets_are_unsupported() {
    let source = canonical_source().replace(
        "\n  given\n",
        "\n  assuming\n    phaseVessel for reaction\n    phaseStage for mixed at mixed\n\n  given\n",
    );
    let result = elaborate(&source, &extended_catalogue());
    assert!(result.typed.is_some(), "{:#?}", result.diagnostics);
    assert!(result.typed.unwrap().assumptions.iter().all(|assumption| {
        assumption.applicability == AssumptionApplicability::DeferredToProcedure
            && assumption.usage == AssumptionUsage::DeferredToProcedure
    }));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-T015")
    );

    let mixed = replace_first_material(
        &canonical_source().replace(
            "\n  given\n",
            "\n  assuming\n    phaseMaterial for silverNitrate\n\n  given\n",
        ),
        "    silverNitrate := prepared\n      1 mL of H2O(l)\n      1 mmol of NaCl(aq)",
    );
    let result = elaborate(&mixed, &extended_catalogue());
    assert!(result.typed.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T013" && diagnostic.status == Some(ElaborationStatus::Unsupported)
    }));

    let unconstrained = mixed.replace("phaseMaterial", "unconstrainedMaterial");
    let result = elaborate(&unconstrained, &extended_catalogue());
    assert!(result.typed.is_some(), "{:#?}", result.diagnostics);
    let assumption = &result.typed.unwrap().assumptions[0];
    assert_eq!(
        assumption.applicability,
        AssumptionApplicability::Applicable
    );
    assert_eq!(assumption.usage, AssumptionUsage::DeferredToProcedure);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-T015")
    );
}

#[test]
fn phase_facts_do_not_conflate_compositionally_identical_substances() {
    let base = extended_catalogue();
    let mut envelope = CatalogueEnvelope {
        digest: base.digest(),
        bundle: base.document().clone(),
    };
    let evidence = BTreeSet::from(["synthetic.slice4".parse().unwrap()]);
    let review = ReviewMetadata {
        status: ReviewStatus::Provisional,
        reviewers: Vec::new(),
    };
    let mut substance = envelope
        .bundle
        .substances
        .iter()
        .find(|substance| substance.id.to_string() == "water")
        .unwrap()
        .clone();
    substance.id = "water-composition-isomer".parse().unwrap();
    substance.name = "Synthetic composition isomer".to_owned();
    substance.aliases.clear();
    substance.provenance.id = "identity.substance.water-composition-isomer"
        .parse()
        .unwrap();
    substance.provenance.evidence.clone_from(&evidence);
    substance.provenance.review.clone_from(&review);
    envelope.bundle.substances.push(substance);
    let mut species = envelope
        .bundle
        .species
        .iter()
        .find(|species| species.id.to_string() == "water.gas")
        .unwrap()
        .clone();
    species.id = "water-composition-isomer.aq".parse().unwrap();
    species.substance = "water-composition-isomer".parse().unwrap();
    species.phase = Phase::Aqueous;
    species.condition.phases = Some(BTreeSet::from([Phase::Aqueous]));
    species.condition.media = Some(BTreeSet::from(["water".parse().unwrap()]));
    species.provenance.id = "identity.species.water-composition-isomer-aq"
        .parse()
        .unwrap();
    species.provenance.evidence = evidence;
    species.provenance.review = review;
    envelope.bundle.species.push(species);
    envelope.digest = envelope.computed_digest().unwrap();
    let catalogue = envelope.validate().unwrap();

    let source =
        replace_first_material(&canonical_source(), "    silverNitrate := 1 mmol of H2O(s)");
    let result = elaborate(&source, &catalogue);
    assert!(result.typed.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T008" && diagnostic.status == Some(ElaborationStatus::Unsupported)
    }));
    assert!(!result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T009" && diagnostic.status == Some(ElaborationStatus::Invalid)
    }));
}

#[test]
fn assumption_targets_and_non_blocking_warnings_remain_explicit() {
    let wrong_target = canonical_source().replace(
        "\n  given\n",
        "\n  assuming\n    idealGas for reaction at initial\n\n  given\n",
    );
    let result = elaborate(&wrong_target, &extended_catalogue());
    assert!(result.typed.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T013"
            && diagnostic.status == Some(ElaborationStatus::IllTyped)
            && diagnostic.related_spans.len() == 1
    }));

    let one_component = replace_first_material(
        &canonical_source(),
        "    silverNitrate := prepared\n      1 mmol of NaCl(aq)",
    );
    let result = elaborate(&one_component, &extended_catalogue());
    assert!(result.typed.is_some(), "{:#?}", result.diagnostics);
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-T015"
            && diagnostic.severity == chems_lang::Severity::Warning
            && diagnostic.status.is_none()
    }));
}
