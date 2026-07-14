use std::{collections::BTreeSet, fs, path::PathBuf};

use chem_catalogue::{
    AssumptionGoalKind, AssumptionKindRecord, AssumptionPropositionKind, AssumptionStageScope,
    AssumptionTargetKind, CatalogueEnvelope, ConditionDomain, CoverageDeclaration,
    CoverageExclusion, EvidenceSource, FactProposition, FactRecord, PublicationKind,
    ReactionFamily, ReviewMetadata, ReviewStatus, SafetyClassification, ValidatedCatalogue,
};
use chem_domain::{
    Dimension, ElementId, EvidenceSourceId, ExactScalar, Phase, Quantity, SourceDecimal,
    UnitExpression, UnitProduct,
};
use chem_kernel::{ElaborationStatus, TypedOperation, elaborate, execute_procedure};
use serde_json::json;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalogue() -> ValidatedCatalogue {
    ValidatedCatalogue::from_json(
        &fs::read(root().join("conformance/catalogue/silver-chloride-001.catalogue.json")).unwrap(),
    )
    .unwrap()
}

#[expect(
    clippy::too_many_lines,
    reason = "the synthetic catalogue keeps every Slice 5 premise visible in one test fixture builder"
)]
fn separation_catalogue() -> ValidatedCatalogue {
    let mut envelope: CatalogueEnvelope = serde_json::from_slice(
        &fs::read(root().join("conformance/catalogue/silver-chloride-001.catalogue.json")).unwrap(),
    )
    .unwrap();
    envelope.bundle.publication = PublicationKind::Working;
    let evidence_id: EvidenceSourceId = "synthetic.slice5".parse().unwrap();
    envelope.bundle.evidence.push(EvidenceSource {
        id: evidence_id.clone(),
        title: "Synthetic Slice 5 state fixture".to_owned(),
        publisher: "ChemSpec test suite".to_owned(),
        locator: "Generated test premise".to_owned(),
        reference: "urn:chemspec:test:slice5".to_owned(),
        publication_date: None,
        retrieved_on: "2026-07-14".to_owned(),
        usage: "Exercises liquid and separation transitions only.".to_owned(),
        review_notes: Some("Not scientific evidence.".to_owned()),
    });
    let evidence = BTreeSet::from([evidence_id]);
    let review = ReviewMetadata {
        status: ReviewStatus::Provisional,
        reviewers: Vec::new(),
    };
    for (id, element, value) in [
        ("atomic-mass.h.slice5", 1_u8, "1.008"),
        ("atomic-mass.o.slice5", 8, "15.999"),
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
            rule_version: "synthetic-slice5-1".to_owned(),
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
        id: "density.water.slice5".parse().unwrap(),
        proposition: FactProposition::HasDensity {
            substance: "water".parse().unwrap(),
            density: Box::new(density),
        },
        condition: ConditionDomain::default(),
        evidence,
        review,
        rule_version: "synthetic-slice5-1".to_owned(),
    });
    envelope.bundle.assumption_kinds.push(AssumptionKindRecord {
        id: "negligibleVolumeChange".parse().unwrap(),
        version: "synthetic-slice5-1".to_owned(),
        proposition: AssumptionPropositionKind::NegligibleVolumeChange,
        required_target: AssumptionTargetKind::Stage,
        stage_scope: AssumptionStageScope::SingleStage,
        condition: ConditionDomain::default(),
        permitted_goals: BTreeSet::from([AssumptionGoalKind::VolumeComposition]),
        explanation: "Treat component volumes as additive at the selected stage.".to_owned(),
        safety: SafetyClassification::PhysicalApproximation,
        evidence: BTreeSet::from(["synthetic.slice5".parse().unwrap()]),
        review: ReviewMetadata {
            status: ReviewStatus::Provisional,
            reviewers: Vec::new(),
        },
    });
    let mut exact_condition = envelope
        .bundle
        .species
        .iter()
        .find(|species| species.id.to_string() == "silver-chloride.s")
        .unwrap()
        .condition
        .clone();
    exact_condition.phases = None;
    envelope.bundle.coverage.push(CoverageDeclaration {
        id: "prepared.water-silver-chloride.slice5".parse().unwrap(),
        species: ["water.l", "silver-chloride.s"]
            .into_iter()
            .map(|id| id.parse().unwrap())
            .collect(),
        condition: exact_condition.clone(),
        families: BTreeSet::from([ReactionFamily::Precipitation]),
        exclusions: vec![CoverageExclusion {
            species: ["water.l", "silver-chloride.s"]
                .into_iter()
                .map(|id| id.parse().unwrap())
                .collect(),
            families: BTreeSet::from([ReactionFamily::Precipitation]),
            reason: "The prepared fixture contains an already isolated solid precipitate and liquid water; this domain declares no further net precipitation reaction."
                .to_owned(),
        }],
        evidence: BTreeSet::from(["synthetic.slice5".parse().unwrap()]),
        review: ReviewMetadata {
            status: ReviewStatus::Provisional,
            reviewers: Vec::new(),
        },
    });
    envelope.bundle.coverage.push(CoverageDeclaration {
        id: "aqueous-ions.slice5".parse().unwrap(),
        species: [
            "silver.aq.plus",
            "nitrate.aq.minus",
            "sodium.aq.plus",
            "chloride.aq.minus",
        ]
        .into_iter()
        .map(|id| id.parse().unwrap())
        .collect(),
        condition: exact_condition,
        families: BTreeSet::from([ReactionFamily::Precipitation]),
        exclusions: Vec::new(),
        evidence: BTreeSet::from(["synthetic.slice5".parse().unwrap()]),
        review: ReviewMetadata {
            status: ReviewStatus::Provisional,
            reviewers: Vec::new(),
        },
    });
    envelope.digest = envelope.computed_digest().unwrap();
    envelope.validate().unwrap()
}

fn separation_catalogue_without_coverage() -> ValidatedCatalogue {
    let catalogue = separation_catalogue();
    let mut envelope = CatalogueEnvelope {
        digest: catalogue.digest(),
        bundle: catalogue.document().clone(),
    };
    envelope.bundle.coverage.clear();
    envelope.digest = envelope.computed_digest().unwrap();
    envelope.validate().unwrap()
}

fn fixture(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap()
}

fn location_oracle(
    location: &chem_domain::InventoryLocation,
    typed: &chem_kernel::TypedExperiment,
) -> serde_json::Value {
    match location {
        chem_domain::InventoryLocation::Unplaced { material } => json!({
            "kind": "unplaced",
            "material": typed.materials.iter().find(|candidate| candidate.id == *material).unwrap().name,
        }),
        chem_domain::InventoryLocation::InVessel { vessel } => json!({
            "kind": "inVessel",
            "vessel": typed.vessels.iter().find(|candidate| candidate.id == *vessel).unwrap().name,
        }),
        chem_domain::InventoryLocation::SeparatedInto { vessel, .. } => json!({
            "kind": "separatedInto",
            "vessel": typed.vessels.iter().find(|candidate| candidate.id == *vessel).unwrap().name,
        }),
    }
}

fn ledger_oracle(
    entry: &chem_domain::LedgerEntry,
    typed: &chem_kernel::TypedExperiment,
) -> serde_json::Value {
    match entry {
        chem_domain::LedgerEntry::Initial { material, .. } => json!({
            "kind": "initial",
            "material": typed.materials.iter().find(|candidate| candidate.id == *material).unwrap().name,
        }),
        chem_domain::LedgerEntry::Move { from, to, .. } => json!({
            "kind": "move",
            "from": location_oracle(from, typed),
            "to": location_oracle(to, typed),
        }),
        chem_domain::LedgerEntry::Split {
            moved_fraction,
            from,
            retained_at,
            moved_to,
            ..
        } => json!({
            "kind": "split",
            "movedFraction": moved_fraction.to_string(),
            "from": location_oracle(from, typed),
            "retainedAt": location_oracle(retained_at, typed),
            "movedTo": location_oracle(moved_to, typed),
        }),
        chem_domain::LedgerEntry::Separate { from, products, .. } => json!({
            "kind": "separate",
            "from": location_oracle(from, typed),
            "products": products.iter().map(|product| location_oracle(&product.location, typed)).collect::<Vec<_>>(),
        }),
    }
}

fn timeline_oracle(
    timeline: &chem_domain::StageTimeline,
    typed: &chem_kernel::TypedExperiment,
) -> serde_json::Value {
    let mut previous_ledger_length = 0;
    let mut previous_vessels = std::collections::BTreeMap::<String, serde_json::Value>::new();
    let stages = timeline
        .stages
        .iter()
        .map(|stage| {
            let ledger_delta = stage.ledger[previous_ledger_length..]
                .iter()
                .map(|entry| ledger_oracle(entry, typed))
                .collect::<Vec<_>>();
            previous_ledger_length = stage.ledger.len();
            let current_vessels = typed
                .vessels
                .iter()
                .map(|declaration| {
                    let vessel = &stage.vessels[&declaration.id];
                    let snapshot = json!({
                        "name": declaration.name,
                        "closure": format!("{:?}", vessel.closure).to_lowercase(),
                        "temperatureCelsius": vessel.temperature.value_in(chem_domain::TemperatureScale::DegreesCelsius).to_string(),
                        "totalVolume": vessel.total_volume.as_ref().map(|volume| volume.canonical_value.to_string()),
                        "mixing": match &vessel.mixing {
                            chem_domain::MixingState::Unmixed => json!({"kind": "unmixed"}),
                            chem_domain::MixingState::HomogeneousContact { mobile_phases, rules } => json!({
                                "kind": "homogeneousContact",
                                "mobilePhases": mobile_phases.iter().map(|phase| format!("{phase:?}").to_lowercase()).collect::<Vec<_>>(),
                                "rules": rules.iter().map(|rule| format!("{rule:?}")).collect::<Vec<_>>(),
                            }),
                        },
                        "partitions": vessel.phase_partitions.iter().map(|partition| json!({
                            "phase": format!("{:?}", partition.phase).to_lowercase(),
                            "portionCount": partition.portions.len(),
                        })).collect::<Vec<_>>(),
                        "contents": vessel.contents.iter().map(|portion| json!({
                            "rootMaterial": typed.materials.iter().find(|material| material.id == portion.root_material).unwrap().name,
                            "knownVolume": portion.known_volume.as_ref().map(|volume| volume.canonical_value.to_string()),
                            "components": portion.components.iter().map(|component| json!({
                                "species": component.species.id,
                                "amount": component.amount.as_ref().map(|value| value.canonical_value.to_string()),
                                "mass": component.mass.as_ref().map(|value| value.canonical_value.to_string()),
                                "volume": component.volume.as_ref().map(|value| value.canonical_value.to_string()),
                                "concentration": component.concentration.as_ref().map(|value| value.canonical_value.to_string()),
                            })).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    });
                    (declaration.name.clone(), snapshot)
                })
                .collect::<Vec<_>>();
            let vessel_changes = current_vessels
                .iter()
                .filter(|(name, snapshot)| previous_vessels.get(name) != Some(snapshot))
                .map(|(_, snapshot)| snapshot.clone())
                .collect::<Vec<_>>();
            previous_vessels.extend(current_vessels);
            json!({
                "ordinal": stage.ordinal,
                "label": stage.source_label,
                "elapsedSeconds": stage.elapsed_seconds.to_string(),
                "unplacedMaterials": typed.materials.iter().filter(|material| stage.unplaced.contains_key(&material.id)).map(|material| material.name.clone()).collect::<Vec<_>>(),
                "vesselChanges": vessel_changes,
                "ledgerDelta": ledger_delta,
                "opportunities": stage.reaction_opportunities.iter().map(|opportunity| json!({
                    "vessel": typed.vessels.iter().find(|vessel| vessel.id == opportunity.vessel).unwrap().name,
                    "trigger": format!("{:?}", opportunity.trigger),
                    "candidates": opportunity.candidates.iter().map(|candidate| json!({
                        "species": candidate.species,
                        "amount": candidate.amount.as_ref().map(|amount| amount.canonical_value.to_string()),
                    })).collect::<Vec<_>>(),
                    "families": opportunity.families.iter().map(|family| format!("{family:?}")).collect::<Vec<_>>(),
                    "coverage": opportunity.coverage.iter().map(ToString::to_string).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "stages": stages,
        "finalAliasOrdinal": timeline.final_stage().unwrap().ordinal,
    })
}

#[test]
fn independently_authored_nonreactive_timeline_matches_canonical_state_and_ledger_oracle() {
    let source = fixture("conformance/procedures/state-timeline-001.chems");
    let catalogue = catalogue();
    let elaborated = elaborate(&source, &catalogue);
    assert!(elaborated.typed.is_some(), "{:#?}", elaborated.diagnostics);
    let typed = elaborated.typed.unwrap();
    let target_id = typed
        .vessels
        .iter()
        .find(|vessel| vessel.name == "target")
        .unwrap()
        .id;
    let first = execute_procedure(&typed, &catalogue);
    assert!(first.timeline.is_some(), "{:#?}", first.diagnostics);
    let second = execute_procedure(&typed, &catalogue);
    assert_eq!(first.timeline, second.timeline);
    let timeline = first.timeline.unwrap();
    assert_eq!(timeline.stages.len(), typed.procedure.len() + 1);
    for (stage, step) in timeline.stages.iter().skip(1).zip(&typed.procedure) {
        assert_eq!(stage.id, step.resulting_stage);
    }
    let final_stage = timeline.final_stage().unwrap();
    let actual = timeline_oracle(&timeline, &typed);
    let expected: serde_json::Value = serde_json::from_slice(
        &fs::read(root().join("conformance/procedures/state-timeline-001.expected.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        final_stage.vessels[&target_id]
            .temperature
            .value_in(chem_domain::TemperatureScale::DegreesCelsius),
        ExactScalar::from_integer(25)
    );
}

#[test]
fn ideal_filter_and_decant_partition_without_inventing_reactions() {
    let source = fixture("conformance/procedures/separation-001.chems");
    let catalogue = separation_catalogue();
    let elaborated = elaborate(&source, &catalogue);
    assert!(elaborated.typed.is_some(), "{:#?}", elaborated.diagnostics);
    let typed = elaborated.typed.unwrap();
    let vessel = |name: &str| {
        typed
            .vessels
            .iter()
            .find(|vessel| vessel.name == name)
            .unwrap()
            .id
    };
    let result = execute_procedure(&typed, &catalogue);
    assert!(result.timeline.is_some(), "{:#?}", result.diagnostics);
    let timeline = result.timeline.unwrap();
    let final_stage = timeline.final_stage().unwrap();
    assert_eq!(
        final_stage.vessels[&vessel("filtrate")]
            .phase_partitions
            .len(),
        1
    );
    assert!(
        final_stage.vessels[&vessel("filtrate")]
            .phase_partitions
            .iter()
            .any(|partition| partition.phase == Phase::Liquid)
    );
    assert!(
        final_stage.vessels[&vessel("residue")]
            .phase_partitions
            .iter()
            .any(|partition| partition.phase == Phase::Solid)
    );
    assert!(
        final_stage.vessels[&vessel("decanted")]
            .phase_partitions
            .iter()
            .any(|partition| partition.phase == Phase::Liquid)
    );
    assert!(
        final_stage.vessels[&vessel("decantSource")]
            .phase_partitions
            .iter()
            .any(|partition| partition.phase == Phase::Solid)
    );
    assert!(
        timeline
            .stages
            .iter()
            .flat_map(|stage| &stage.reaction_opportunities)
            .all(|opportunity| !opportunity.candidates.is_empty())
    );
}

fn two_solution_source(procedure: &str, source_capacity: &str) -> String {
    format!(
        "chems 1\nuse catalog ChemSpec.Aqueous@1\n\nexperiment Operations where\n  conditions\n    temperature := 25 degC\n    pressure := 1 atm\n    medium := aqueous\n\n  given\n    left := 40 mL of 0.100 mol/L AgNO3(aq)\n    right := 30 mL of 0.100 mol/L NaCl(aq)\n\n  vessels\n    source := open vessel {source_capacity}\n    target := open vessel 100 mL\n\n  procedure\n{procedure}\n\n  expect at final\n    amount AgNO3(aq) := ?\n\n  by\n    solve stoichiometry\n"
    )
}

fn execute_source(source: &str) -> chem_kernel::ProcedureResult {
    let catalogue = catalogue();
    let elaborated = elaborate(source, &catalogue);
    assert!(elaborated.typed.is_some(), "{:#?}", elaborated.diagnostics);
    execute_procedure(&elaborated.typed.unwrap(), &catalogue)
}

fn execute_source_with(
    source: &str,
    catalogue: &ValidatedCatalogue,
) -> chem_kernel::ProcedureResult {
    let elaborated = elaborate(source, catalogue);
    assert!(elaborated.typed.is_some(), "{:#?}", elaborated.diagnostics);
    execute_procedure(&elaborated.typed.unwrap(), catalogue)
}

fn with_volume_assumption(source: &str) -> String {
    source.replace(
        "\n  given\n",
        "\n  assuming\n    negligibleVolumeChange for result at result\n\n  given\n",
    )
}

#[test]
fn add_combine_whole_transfer_and_known_failures_follow_closed_preconditions() {
    let model_catalogue = separation_catalogue();
    for procedure in [
        "    place left in source\n    result: add right to source",
        "    result: combine left with right in source",
    ] {
        let source = with_volume_assumption(&two_solution_source(procedure, "100 mL"));
        let result = execute_source_with(&source, &model_catalogue);
        assert!(result.timeline.is_some(), "{:#?}", result.diagnostics);
        let final_stage = result.timeline.unwrap().final_stage().unwrap().clone();
        let volume = final_stage
            .vessels
            .values()
            .find_map(|vessel| vessel.total_volume.as_ref())
            .unwrap();
        assert_eq!(volume.derivation.assumptions.len(), 1);
    }

    let whole = execute_source(&two_solution_source(
        "    place left in source\n    transfer from source to target",
        "100 mL",
    ));
    assert!(whole.timeline.is_some(), "{:#?}", whole.diagnostics);

    let unsupported_add = execute_source(&two_solution_source(
        "    place left in source\n    add right to source",
        "100 mL",
    ));
    assert!(unsupported_add.timeline.is_none());
    assert!(unsupported_add.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Unsupported)
            && diagnostic.summary.contains("mixture-volume model")
    }));

    let capacity = execute_source(&two_solution_source("    place left in source", "10 mL"));
    assert!(capacity.timeline.is_none());
    assert!(capacity.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Invalid)
            && diagnostic.summary.contains("capacity")
    }));

    let duplicate = execute_source(&two_solution_source(
        "    place left in source\n    place left in target",
        "100 mL",
    ));
    assert!(duplicate.timeline.is_none());
    assert!(duplicate.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Invalid)
            && diagnostic.summary.contains("not unplaced")
    }));

    let direction = execute_source(&two_solution_source(
        "    place left in source\n    heat source to 10 degC",
        "100 mL",
    ));
    assert!(direction.timeline.is_none());
    assert!(direction.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Invalid)
            && diagnostic.summary.contains("strictly above")
    }));
}

#[test]
fn atomic_combine_is_operand_swap_equivalent_and_uses_actual_ionic_candidates() {
    let catalogue = separation_catalogue();
    let source = with_volume_assumption(&two_solution_source(
        "    result: combine left with right in source",
        "100 mL",
    ));
    let left_first = elaborate(&source, &catalogue).typed.unwrap();
    let mut right_first = left_first.clone();
    let TypedOperation::Combine { left, right, .. } = &mut right_first.procedure[0].operation
    else {
        panic!("fixture must elaborate to combine");
    };
    std::mem::swap(left, right);
    let left_timeline = execute_procedure(&left_first, &catalogue).timeline.unwrap();
    let right_timeline = execute_procedure(&right_first, &catalogue)
        .timeline
        .unwrap();
    assert_eq!(left_timeline, right_timeline);

    let opportunity = &left_timeline.final_stage().unwrap().reaction_opportunities[0];
    let candidate_ids = opportunity
        .candidates
        .iter()
        .map(|candidate| candidate.species.to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        candidate_ids,
        BTreeSet::from([
            "chloride.aq.minus".to_owned(),
            "nitrate.aq.minus".to_owned(),
            "silver.aq.plus".to_owned(),
            "sodium.aq.plus".to_owned(),
        ])
    );
    assert!(!opportunity.coverage.is_empty());
    assert!(!opportunity.premises.is_empty());
}

#[test]
fn thermal_changes_stop_at_catalogue_domains_and_accept_supported_water_states() {
    let unsupported = execute_source(&two_solution_source(
        "    place left in source\n    heat source to 40 degC",
        "100 mL",
    ));
    assert!(unsupported.timeline.is_none());
    assert!(unsupported.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Unsupported)
            && diagnostic.summary.contains("reviewed phase domain")
    }));

    let water = "chems 1\nuse catalog ChemSpec.Aqueous@1\n\nexperiment ThermalWater where\n  conditions\n    temperature := 25 degC\n    pressure := 1 atm\n    medium := aqueous\n\n  given\n    water := 50 mL of H2O(l)\n\n  vessels\n    source := open vessel 100 mL\n\n  procedure\n    place water in source\n    heat source to 40 degC\n    cool source to 20 degC\n\n  expect at final\n    amount H2O(l) := ?\n\n  by\n    solve stoichiometry\n";
    let catalogue = separation_catalogue();
    let supported = execute_source_with(water, &catalogue);
    assert!(supported.timeline.is_some(), "{:#?}", supported.diagnostics);
    assert_eq!(
        supported
            .timeline
            .unwrap()
            .final_stage()
            .unwrap()
            .vessels
            .values()
            .next()
            .unwrap()
            .temperature
            .value_in(chem_domain::TemperatureScale::DegreesCelsius),
        ExactScalar::from_integer(20)
    );
}

#[test]
fn prepared_colocation_requires_complete_catalogue_coverage_before_stage_zero() {
    let source = fixture("conformance/procedures/separation-001.chems");
    let catalogue = separation_catalogue_without_coverage();
    let typed = elaborate(&source, &catalogue).typed.unwrap();
    let result = execute_procedure(&typed, &catalogue);
    assert!(result.timeline.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Unsupported)
            && diagnostic.summary.contains("asserted initial co-location")
    }));
}

#[test]
fn split_and_separation_ledgers_record_replayable_source_and_destination_locations() {
    let state_catalogue = separation_catalogue();
    let timeline_source = fixture("conformance/procedures/state-timeline-001.chems");
    let timeline_typed = elaborate(&timeline_source, &catalogue()).typed.unwrap();
    let timeline = execute_procedure(&timeline_typed, &catalogue())
        .timeline
        .unwrap();
    assert!(timeline.final_stage().unwrap().ledger.iter().any(|entry| {
        matches!(
            entry,
            chem_domain::LedgerEntry::Split {
                from: chem_domain::InventoryLocation::InVessel { .. },
                retained_at: chem_domain::InventoryLocation::InVessel { .. },
                moved_to: chem_domain::InventoryLocation::InVessel { .. },
                ..
            }
        )
    }));

    let separation_source = fixture("conformance/procedures/separation-001.chems");
    let separation_typed = elaborate(&separation_source, &state_catalogue)
        .typed
        .unwrap();
    let separation = execute_procedure(&separation_typed, &state_catalogue)
        .timeline
        .unwrap();
    assert!(
        separation
            .final_stage()
            .unwrap()
            .ledger
            .iter()
            .any(|entry| {
                matches!(
                    entry,
                    chem_domain::LedgerEntry::Separate { products, .. }
                        if !products.is_empty()
                            && products.iter().all(|product| matches!(
                                product.location,
                                chem_domain::InventoryLocation::SeparatedInto { .. }
                            ))
                )
            })
    );
}

#[test]
fn whole_mobile_transfer_leaves_only_solid_residue_and_no_zero_liquid_portion() {
    let source = fixture("conformance/procedures/separation-001.chems").replace(
        "filter filterSource into filtrate and residue\n    place decantedMixture in decantSource\n    decant decantSource into decanted",
        "transfer 40 mL from filterSource to filtrate",
    );
    let catalogue = separation_catalogue();
    let typed = elaborate(&source, &catalogue).typed.unwrap();
    let source_id = typed
        .vessels
        .iter()
        .find(|vessel| vessel.name == "filterSource")
        .unwrap()
        .id;
    let result = execute_procedure(&typed, &catalogue);
    assert!(result.timeline.is_some(), "{:#?}", result.diagnostics);
    let final_stage = result.timeline.unwrap().final_stage().unwrap().clone();
    let remaining = &final_stage.vessels[&source_id].contents;
    assert!(!remaining.is_empty());
    assert!(
        remaining
            .iter()
            .flat_map(|portion| &portion.components)
            .all(|component| component.species.phase == Phase::Solid
                && component
                    .amount
                    .as_ref()
                    .is_none_or(|amount| !amount.canonical_value.is_zero()))
    );
}

#[test]
fn empty_opportunities_have_no_catalogue_matches_and_pure_solid_filtering_is_supported() {
    let empty = "chems 1\nuse catalog ChemSpec.Aqueous@1\n\nexperiment EmptyClosure where\n  conditions\n    temperature := 25 degC\n    pressure := 1 atm\n    medium := aqueous\n\n  given\n    solid := 1 mmol of AgCl(s)\n\n  vessels\n    source := open vessel 100 mL\n\n  procedure\n    seal source\n\n  expect at final\n    amount AgCl(s) := ?\n\n  by\n    solve stoichiometry\n";
    let catalogue = separation_catalogue();
    let empty_typed = elaborate(empty, &catalogue).typed.unwrap();
    let empty_timeline = execute_procedure(&empty_typed, &catalogue)
        .timeline
        .unwrap();
    let opportunity = &empty_timeline.final_stage().unwrap().reaction_opportunities[0];
    assert!(opportunity.candidates.is_empty());
    assert!(opportunity.coverage.is_empty());
    assert!(opportunity.families.is_empty());

    let solid_filter = "chems 1\nuse catalog ChemSpec.Aqueous@1\n\nexperiment SolidFilter where\n  conditions\n    temperature := 25 degC\n    pressure := 1 atm\n    medium := aqueous\n\n  given\n    solid := 1 mmol of AgCl(s)\n\n  vessels\n    source := open vessel 100 mL\n    filtrate := open vessel 100 mL\n    residue := open vessel 100 mL\n\n  procedure\n    place solid in source\n    filter source into filtrate and residue\n\n  expect at final\n    amount AgCl(s) := ?\n\n  by\n    solve stoichiometry\n";
    let filter_typed = elaborate(solid_filter, &catalogue).typed.unwrap();
    let filtrate = filter_typed
        .vessels
        .iter()
        .find(|vessel| vessel.name == "filtrate")
        .unwrap()
        .id;
    let residue = filter_typed
        .vessels
        .iter()
        .find(|vessel| vessel.name == "residue")
        .unwrap()
        .id;
    let filtered = execute_procedure(&filter_typed, &catalogue);
    assert!(filtered.timeline.is_some(), "{:#?}", filtered.diagnostics);
    let final_stage = filtered.timeline.unwrap().final_stage().unwrap().clone();
    assert!(final_stage.vessels[&filtrate].contents.is_empty());
    assert!(
        final_stage.vessels[&residue]
            .phase_partitions
            .iter()
            .any(|partition| partition.phase == Phase::Solid)
    );
}

#[test]
fn proportional_transfer_property_preserves_exact_inventory_and_stage_identity() {
    for millilitres in 1..40 {
        let source = fixture("conformance/procedures/state-timeline-001.chems")
            .replace("transfer 20 mL", &format!("transfer {millilitres} mL"));
        let catalogue = catalogue();
        let elaborated = elaborate(&source, &catalogue);
        let typed = elaborated.typed.unwrap();
        let result = execute_procedure(&typed, &catalogue);
        assert!(
            result.timeline.is_some(),
            "{millilitres}: {:#?}",
            result.diagnostics
        );
        let timeline = result.timeline.unwrap();
        let final_stage = timeline.final_stage().unwrap();
        let total = final_stage
            .vessels
            .values()
            .flat_map(|vessel| &vessel.contents)
            .filter_map(|portion| portion.components[0].amount.as_ref())
            .fold(ExactScalar::zero(), |total, amount| {
                &total + &amount.canonical_value
            });
        assert_eq!(total, ExactScalar::new(1.into(), 200.into()).unwrap());
        assert_eq!(timeline.stages[2].id, typed.procedure[1].resulting_stage);
    }
}

#[test]
fn missing_volume_and_uncontacted_sampling_are_unsupported_while_solids_remain() {
    let unknown = two_solution_source(
        "    place left in source\n    add right to source",
        "100 mL",
    )
    .replace(
        "right := 30 mL of 0.100 mol/L NaCl(aq)",
        "right := 3 mmol of NaCl(aq)",
    );
    let result = execute_source(&unknown);
    assert!(result.timeline.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Unsupported)
            && diagnostic.summary.contains("volume premise")
    }));

    let source = fixture("conformance/procedures/separation-001.chems")
        .replace(
            "filter filterSource into filtrate and residue\n    place decantedMixture in decantSource\n    decant decantSource into decanted",
            "transfer 1 mL from filterSource to filtrate",
        );
    let catalogue = separation_catalogue();
    let elaborated = elaborate(&source, &catalogue);
    let typed = elaborated.typed.unwrap();
    let result = execute_procedure(&typed, &catalogue);
    assert!(result.timeline.is_some(), "{:#?}", result.diagnostics);
    let final_stage = result.timeline.unwrap().final_stage().unwrap().clone();
    let filter_source = typed
        .vessels
        .iter()
        .find(|vessel| vessel.name == "filterSource")
        .map(|vessel| vessel.id)
        .unwrap();
    assert!(
        final_stage.vessels[&filter_source]
            .phase_partitions
            .iter()
            .any(|partition| partition.phase == Phase::Solid)
    );

    let uncontacted = two_solution_source(
        "    place left in source\n    mixed: add right to source\n    transfer 1 mL from source to target",
        "100 mL",
    )
    .replace(
        "\n  given\n",
        "\n  assuming\n    negligibleVolumeChange for mixed at mixed\n\n  given\n",
    );
    let result = execute_source_with(&uncontacted, &catalogue);
    assert!(result.timeline.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Unsupported)
            && diagnostic.summary.contains("homogeneous contact")
    }));
}

#[test]
fn partial_transfer_rejects_two_mobile_phase_partitions_even_after_stirring() {
    let source = "chems 1\nuse catalog ChemSpec.Aqueous@1\n\nexperiment TwoMobilePhases where\n  conditions\n    temperature := 25 degC\n    pressure := 1 atm\n    medium := aqueous\n\n  assuming\n    negligibleVolumeChange for mixed at mixed\n\n  given\n    liquid := 20 mL of H2O(l)\n    solution := 10 mL of 0.100 mol/L NaCl(aq)\n\n  vessels\n    source := open vessel 100 mL\n    target := open vessel 100 mL\n\n  procedure\n    place liquid in source\n    mixed: add solution to source\n    stir source\n    transfer 1 mL from source to target\n\n  expect at final\n    amount NaCl(aq) := ?\n\n  by\n    solve stoichiometry\n";
    let catalogue = separation_catalogue();
    let result = execute_source_with(source, &catalogue);
    assert!(result.timeline.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.status == Some(ElaborationStatus::Unsupported)
            && diagnostic
                .summary
                .contains("exactly one supported mobile phase partition")
    }));
}

#[test]
fn every_authored_operation_quantity_remains_dimension_checked_before_execution() {
    let source = two_solution_source(
        "    place left in source\n    transfer 2 s from source to target",
        "100 mL",
    );
    let result = elaborate(&source, &catalogue());
    assert!(result.typed.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.status == Some(ElaborationStatus::IllTyped) })
    );
    assert_eq!(
        Dimension::VOLUME,
        chem_domain::Dimension::new(0, 3, 0, 0, 0)
    );
}

#[test]
fn conformance_capacity_failure_has_the_exact_state_diagnostic_and_span() {
    let source = fixture("conformance/procedures/procedure-failures-001.chems");
    let result = execute_source(&source);
    assert!(result.timeline.is_none());
    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "CHEMS-S012");
    assert_eq!(diagnostic.status, Some(ElaborationStatus::Invalid));
    assert_eq!(diagnostic.primary_span, chems_lang::ByteSpan::new(273, 298));
}
