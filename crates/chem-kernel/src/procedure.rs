use std::collections::{BTreeMap, BTreeSet};

use chem_catalogue::{
    AssumptionGoalKind, AssumptionPropositionKind, ConditionPoint, FactProposition, ReactionFamily,
    ValidatedCatalogue,
};
use chem_domain::{
    AnalyticalComponent, AssumptionPremiseId, ClosureState, ContactRule, DerivedInput,
    DerivedQuantity, DerivedQuantityRule, DigestId, Dimension, ExactScalar, FactId,
    InventoryLocation, InventoryPortion, InventoryPortionId, InventoryPortionKind, LedgerEntry,
    Material, MaterialForm, MaterialId, MixingState, OperationId, OpportunityTrigger, Phase,
    PhasePartition, Quantity, QuantityDerivation, ReactionCandidate, ReactionOpportunity,
    ReactionOpportunityKind, ReactionRuleFamily, SeparatedProduct, SourceRange, Stage,
    StageEnvironment, StageKind, StageTimeline, VesselId, VesselState,
};
use chems_lang::ByteSpan;
use serde_json::json;

use crate::{
    AssumptionTarget, ElaborationDiagnostic, ElaborationStatus, StageReference, TypedExperiment,
    TypedOperation, TypedProcedureStep, VesselClosure,
};

/// Result of deterministic procedure execution.
#[derive(Debug, Clone)]
pub struct ProcedureResult {
    pub timeline: Option<StageTimeline>,
    pub diagnostics: Vec<ElaborationDiagnostic>,
}

#[derive(Debug)]
struct TransitionFailure {
    status: ElaborationStatus,
    summary: String,
}

impl TransitionFailure {
    fn invalid(summary: impl Into<String>) -> Self {
        Self {
            status: ElaborationStatus::Invalid,
            summary: summary.into(),
        }
    }

    fn unsupported(summary: impl Into<String>) -> Self {
        Self {
            status: ElaborationStatus::Unsupported,
            summary: summary.into(),
        }
    }
}

/// Constructs the immutable initial stage and applies every typed operation.
///
/// # Panics
///
/// Panics only if a caller forges internally inconsistent typed HIR instead of
/// using [`crate::elaborate`], for example by removing an operation vessel.
#[must_use]
pub fn execute_procedure(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
) -> ProcedureResult {
    let mut diagnostics = Vec::new();
    if experiment.catalogue.digest != catalogue.digest()
        || experiment.catalogue.name != catalogue.document().name
        || experiment.catalogue.version != catalogue.document().version
    {
        diagnostics.push(ElaborationDiagnostic::new(
            "CHEMS-S010",
            ElaborationStatus::Unsupported,
            "procedure execution requires the exact catalogue bound by typed HIR",
            experiment_origin(experiment),
        ));
        return ProcedureResult {
            timeline: None,
            diagnostics,
        };
    }
    let initial = match initial_stage(experiment, catalogue) {
        Ok(stage) => stage,
        Err(failure) => {
            diagnostics.push(transition_diagnostic(
                experiment,
                None,
                &failure,
                experiment_origin(experiment),
            ));
            return ProcedureResult {
                timeline: None,
                diagnostics,
            };
        }
    };
    let baseline = inventory_totals(&initial);
    let mut stages = vec![initial];
    for (index, step) in experiment.procedure.iter().enumerate() {
        let mut next = stages.last().expect("initial stage exists").clone();
        next.id = step.resulting_stage;
        next.ordinal = u32::try_from(index + 1).expect("procedure length fits u32");
        next.source_label.clone_from(&step.source_label);
        next.transition = Some(operation_id(&step.operation));
        next.reaction_opportunities.clear();
        if let Err(failure) = apply_operation(experiment, catalogue, step, &mut next) {
            diagnostics.push(transition_diagnostic(
                experiment,
                Some(operation_id(&step.operation)),
                &failure,
                operation_origin(experiment, operation_id(&step.operation)),
            ));
            return ProcedureResult {
                timeline: None,
                diagnostics,
            };
        }
        if verify_stage_invariants(&next).is_err() || inventory_totals(&next) != baseline {
            diagnostics.push(ElaborationDiagnostic::system_error(
                "CHEMS-S014",
                "internal inventory conservation invariant failed",
                operation_origin(experiment, operation_id(&step.operation)),
            ));
            return ProcedureResult {
                timeline: None,
                diagnostics,
            };
        }
        stages.push(next);
    }
    ProcedureResult {
        timeline: Some(StageTimeline { stages }),
        diagnostics,
    }
}

fn initial_stage(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
) -> Result<Stage, TransitionFailure> {
    let mut unplaced = BTreeMap::new();
    let mut ledger = Vec::new();
    for material in &experiment.materials {
        validate_initial_material(experiment, catalogue, material)?;
        let portion = initial_portion(experiment, material);
        if unplaced.insert(material.id, portion.clone()).is_some() {
            return Err(TransitionFailure::invalid(
                "duplicate initial material inventory identity",
            ));
        }
        ledger.push(LedgerEntry::Initial {
            portion: portion.id,
            material: material.id,
        });
    }
    let vessels = experiment
        .vessels
        .iter()
        .map(|vessel| {
            (
                vessel.id,
                VesselState {
                    id: vessel.id,
                    capacity: vessel.capacity.clone(),
                    closure: match vessel.closure {
                        VesselClosure::Open => ClosureState::Open,
                        VesselClosure::Closed => ClosureState::Closed,
                    },
                    temperature: experiment.environment.temperature.clone(),
                    pressure: experiment.environment.pressure.clone(),
                    contents: Vec::new(),
                    total_volume: None,
                    phase_partitions: Vec::new(),
                    mixing: MixingState::Unmixed,
                },
            )
        })
        .collect();
    let id = DigestId::<StageKind>::of_json(&json!({
        "experiment": experiment.id,
        "stage": "initial",
    }))
    .expect("initial stage identity is canonical");
    let portion_history = unplaced
        .values()
        .map(|portion| (portion.id, portion.clone()))
        .collect();
    Ok(Stage {
        id,
        ordinal: 0,
        source_label: Some("initial".to_owned()),
        elapsed_seconds: ExactScalar::zero(),
        environment: StageEnvironment {
            temperature: experiment.environment.temperature.clone(),
            pressure: experiment.environment.pressure.clone(),
            medium: experiment.environment.medium.clone(),
        },
        vessels,
        unplaced,
        portion_history,
        ledger,
        transition: None,
        reaction_opportunities: Vec::new(),
        source_origins: converted_origins(experiment),
    })
}

fn validate_initial_material(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    material: &Material,
) -> Result<(), TransitionFailure> {
    if material.analytical_inventory.is_empty() {
        return Err(TransitionFailure::invalid(format!(
            "material `{}` has no analytical inventory",
            material.name
        )));
    }
    if material.analytical_inventory.iter().any(|component| {
        component
            .amount
            .as_ref()
            .is_some_and(|amount| amount.canonical_value.is_negative())
    }) {
        return Err(TransitionFailure::invalid(format!(
            "material `{}` has negative initial inventory",
            material.name
        )));
    }
    if let MaterialForm::Prepared { components } = &material.form {
        let source_count = components
            .iter()
            .map(|component| component.source_component_indices.len())
            .sum::<usize>();
        let unique_source_count = components
            .iter()
            .flat_map(|component| &component.source_component_indices)
            .collect::<BTreeSet<_>>()
            .len();
        if components
            .iter()
            .any(|component| component.source_component_indices.is_empty())
            || source_count != unique_source_count
            || components
                .iter()
                .map(|component| &component.analytical)
                .ne(&material.analytical_inventory)
        {
            return Err(TransitionFailure::invalid(format!(
                "prepared material `{}` has inconsistent normalized components",
                material.name
            )));
        }
        if components.len() > 1 {
            validate_prepared_colocation(experiment, catalogue, material)?;
        }
    }
    Ok(())
}

fn validate_prepared_colocation(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    material: &Material,
) -> Result<(), TransitionFailure> {
    let (candidates, _) = actual_candidates(
        experiment,
        catalogue,
        material.analytical_inventory.clone(),
        &experiment.environment.temperature,
        &experiment.environment.pressure,
    )?;
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.species.clone())
        .collect::<BTreeSet<_>>();
    let has_exact_no_reaction_exclusion = catalogue.document().coverage.iter().any(|coverage| {
        candidate_ids.is_subset(&coverage.species)
            && candidates.iter().all(|candidate| {
                coverage.condition.contains(&ConditionPoint {
                    temperature_kelvin: experiment.environment.temperature.kelvin().clone(),
                    pressure_pascal: experiment.environment.pressure.canonical_value().clone(),
                    medium: experiment.environment.medium.clone(),
                    phase: Some(candidate.phase),
                })
            })
            && coverage.exclusions.iter().any(|exclusion| {
                exclusion.species == candidate_ids && exclusion.families == coverage.families
            })
    });
    if !has_exact_no_reaction_exclusion {
        return Err(TransitionFailure::unsupported(format!(
            "prepared material `{}` has no complete catalogue-backed no-reaction derivation for its asserted initial co-location",
            material.name
        )));
    }
    Ok(())
}

fn initial_portion(experiment: &TypedExperiment, material: &Material) -> InventoryPortion {
    let id = DigestId::<InventoryPortionKind>::of_json(&json!({
        "experiment": experiment.id,
        "material": material.id,
        "portion": "initial",
    }))
    .expect("initial portion identity is canonical");
    InventoryPortion {
        id,
        root_material: material.id,
        parent: None,
        components: material.analytical_inventory.clone(),
        known_volume: material_volume(material),
    }
}

fn material_volume(material: &Material) -> Option<DerivedQuantity> {
    match &material.form {
        MaterialForm::LiquidSampleByVolume { volume, .. }
        | MaterialForm::GasSampleByVolume { volume, .. } => Some(authored_volume(volume)),
        MaterialForm::Solution { total_volume, .. } => Some(authored_volume(total_volume)),
        MaterialForm::Prepared { .. } if material.analytical_inventory.len() == 1 => material
            .analytical_inventory
            .first()
            .and_then(|component| component.volume.clone()),
        MaterialForm::Prepared { .. }
        | MaterialForm::SampleByAmount { .. }
        | MaterialForm::SampleByMass { .. } => None,
    }
}

fn authored_volume(quantity: &Quantity) -> DerivedQuantity {
    DerivedQuantity::new(
        quantity.canonical_value().clone(),
        Dimension::VOLUME,
        QuantityDerivation {
            rule: DerivedQuantityRule::AuthoredQuantity,
            inputs: vec![DerivedInput {
                role: "authored".to_owned(),
                canonical_value: quantity.canonical_value().clone(),
                dimension: Dimension::VOLUME,
            }],
            premises: BTreeSet::new(),
            assumptions: BTreeSet::new(),
        },
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "the closed operation set is intentionally visible as one exhaustive transition table"
)]
fn apply_operation(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    step: &TypedProcedureStep,
    stage: &mut Stage,
) -> Result<(), TransitionFailure> {
    match &step.operation {
        TypedOperation::Place {
            id,
            material,
            vessel,
        } => place_or_add(
            experiment,
            catalogue,
            stage,
            *id,
            *material,
            *vessel,
            true,
            false,
            OpportunityTrigger::Placement,
        ),
        TypedOperation::Add {
            id,
            material,
            vessel,
        } => place_or_add(
            experiment,
            catalogue,
            stage,
            *id,
            *material,
            *vessel,
            false,
            true,
            OpportunityTrigger::CoLocation,
        ),
        TypedOperation::Combine {
            id,
            left,
            right,
            vessel,
        } => combine(experiment, catalogue, stage, *id, *left, *right, *vessel),
        TypedOperation::Transfer {
            id,
            quantity,
            source,
            destination,
        } => transfer(
            experiment,
            catalogue,
            stage,
            *id,
            quantity.as_ref(),
            *source,
            *destination,
        ),
        TypedOperation::Stir {
            id,
            vessel,
            duration,
        } => {
            let target = stage
                .vessels
                .get_mut(vessel)
                .ok_or_else(|| TransitionFailure::invalid("unknown vessel state"))?;
            if target.contents.is_empty() {
                return Err(TransitionFailure::invalid("cannot stir an empty vessel"));
            }
            let mobile_phases = target
                .phase_partitions
                .iter()
                .map(|partition| partition.phase)
                .filter(|phase| matches!(phase, Phase::Liquid | Phase::Aqueous))
                .collect::<BTreeSet<_>>();
            let liquid_species = target
                .contents
                .iter()
                .flat_map(|portion| &portion.components)
                .filter(|component| component.species.phase == Phase::Liquid)
                .map(|component| component.species.id.clone())
                .collect::<BTreeSet<_>>();
            if liquid_species.len() > 1 {
                return Err(TransitionFailure::unsupported(
                    "stirring multiple liquid species requires a catalogue-backed compatibility premise",
                ));
            }
            let mut rules = BTreeSet::new();
            if mobile_phases.contains(&Phase::Aqueous) {
                rules.insert(ContactRule::SameMediumAqueous);
            }
            if mobile_phases.contains(&Phase::Liquid) {
                rules.insert(ContactRule::SameSpeciesLiquid);
            }
            target.mixing = if mobile_phases.is_empty() {
                MixingState::Unmixed
            } else {
                MixingState::HomogeneousContact {
                    mobile_phases,
                    rules,
                }
            };
            if let Some(duration) = duration {
                stage.elapsed_seconds = &stage.elapsed_seconds + duration.canonical_value();
            }
            add_opportunity(
                experiment,
                catalogue,
                stage,
                *id,
                *vessel,
                OpportunityTrigger::HomogeneousContact,
            )?;
            Ok(())
        }
        TypedOperation::Heat { id, vessel, target } => {
            change_temperature(experiment, catalogue, stage, *id, *vessel, target, true)
        }
        TypedOperation::Cool { id, vessel, target } => {
            change_temperature(experiment, catalogue, stage, *id, *vessel, target, false)
        }
        TypedOperation::Wait { duration, .. } => {
            stage.elapsed_seconds = &stage.elapsed_seconds + duration.canonical_value();
            Ok(())
        }
        TypedOperation::Seal { id, vessel } => {
            let target = stage
                .vessels
                .get_mut(vessel)
                .ok_or_else(|| TransitionFailure::invalid("unknown vessel state"))?;
            if target.closure != ClosureState::Open {
                return Err(TransitionFailure::invalid(
                    "cannot seal an already closed vessel",
                ));
            }
            target.closure = ClosureState::Closed;
            add_opportunity(
                experiment,
                catalogue,
                stage,
                *id,
                *vessel,
                OpportunityTrigger::ClosureChange,
            )?;
            Ok(())
        }
        TypedOperation::Open { id, vessel } => {
            let target = stage
                .vessels
                .get_mut(vessel)
                .ok_or_else(|| TransitionFailure::invalid("unknown vessel state"))?;
            if target.closure != ClosureState::Closed {
                return Err(TransitionFailure::invalid(
                    "cannot open an already open vessel",
                ));
            }
            if target.pressure != experiment.environment.pressure
                || target.contents.iter().any(portion_has_gas)
            {
                return Err(TransitionFailure::unsupported(
                    "opening requires a supported pressure-equalization and retention model",
                ));
            }
            target.closure = ClosureState::Open;
            target.pressure = experiment.environment.pressure.clone();
            add_opportunity(
                experiment,
                catalogue,
                stage,
                *id,
                *vessel,
                OpportunityTrigger::ClosureChange,
            )?;
            Ok(())
        }
        TypedOperation::Filter {
            id,
            source,
            filtrate,
            residue,
        } => filter(
            experiment, catalogue, stage, *id, *source, *filtrate, *residue,
        ),
        TypedOperation::Decant {
            id,
            source,
            destination,
        } => decant(experiment, catalogue, stage, *id, *source, *destination),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the placement helper receives the explicit closed transition operands and execution context"
)]
fn place_or_add(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    material: MaterialId,
    vessel: VesselId,
    require_empty: bool,
    capacity_required: bool,
    trigger: OpportunityTrigger,
) -> Result<(), TransitionFailure> {
    let portion = stage
        .unplaced
        .get(&material)
        .ok_or_else(|| TransitionFailure::invalid("material is not unplaced"))?
        .clone();
    let target = stage
        .vessels
        .get(&vessel)
        .ok_or_else(|| TransitionFailure::invalid("unknown vessel state"))?;
    if require_empty && !target.contents.is_empty() {
        return Err(TransitionFailure::invalid("place requires an empty vessel"));
    }
    let mut prospective = target.contents.clone();
    prospective.push(portion.clone());
    let total_volume = derive_total_volume(experiment, catalogue, stage, vessel, &prospective)?;
    ensure_capacity(target, total_volume.as_ref(), capacity_required)?;
    stage.unplaced.remove(&material);
    let target = stage
        .vessels
        .get_mut(&vessel)
        .expect("checked vessel exists");
    target.contents.push(portion.clone());
    target.total_volume = total_volume;
    target.mixing = MixingState::Unmixed;
    rebuild_partitions(target);
    stage.ledger.push(LedgerEntry::Move {
        portion: portion.id,
        operation,
        from: InventoryLocation::Unplaced { material },
        to: InventoryLocation::InVessel { vessel },
    });
    add_opportunity(experiment, catalogue, stage, operation, vessel, trigger)?;
    Ok(())
}

fn combine(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    left: MaterialId,
    right: MaterialId,
    vessel: VesselId,
) -> Result<(), TransitionFailure> {
    if left == right {
        return Err(TransitionFailure::invalid(
            "combine requires two distinct materials",
        ));
    }
    let left_portion = stage
        .unplaced
        .get(&left)
        .ok_or_else(|| TransitionFailure::invalid("left material is not unplaced"))?
        .clone();
    let right_portion = stage
        .unplaced
        .get(&right)
        .ok_or_else(|| TransitionFailure::invalid("right material is not unplaced"))?
        .clone();
    let target = stage
        .vessels
        .get(&vessel)
        .ok_or_else(|| TransitionFailure::invalid("unknown vessel state"))?;
    if !target.contents.is_empty() {
        return Err(TransitionFailure::invalid(
            "combine requires an empty vessel",
        ));
    }
    let mut combined = vec![left_portion.clone(), right_portion.clone()];
    combined.sort_by_key(|portion| portion.root_material);
    let total_volume = derive_total_volume(experiment, catalogue, stage, vessel, &combined)?;
    ensure_capacity(target, total_volume.as_ref(), true)?;
    stage.unplaced.remove(&left);
    stage.unplaced.remove(&right);
    let target = stage
        .vessels
        .get_mut(&vessel)
        .expect("checked vessel exists");
    target.contents.clone_from(&combined);
    target.total_volume = total_volume;
    target.mixing = MixingState::Unmixed;
    rebuild_partitions(target);
    let mut moved = [(left, left_portion), (right, right_portion)];
    moved.sort_by_key(|(material, _)| *material);
    for (material, portion) in moved {
        stage.ledger.push(LedgerEntry::Move {
            portion: portion.id,
            operation,
            from: InventoryLocation::Unplaced { material },
            to: InventoryLocation::InVessel { vessel },
        });
    }
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        vessel,
        OpportunityTrigger::CoLocation,
    )?;
    Ok(())
}

fn transfer(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    quantity: Option<&chem_domain::Quantity>,
    source: VesselId,
    destination: VesselId,
) -> Result<(), TransitionFailure> {
    if source == destination {
        return Err(TransitionFailure::invalid(
            "transfer source and destination must differ",
        ));
    }
    if !stage
        .vessels
        .get(&destination)
        .ok_or_else(|| TransitionFailure::invalid("unknown destination vessel"))?
        .contents
        .is_empty()
    {
        return Err(TransitionFailure::invalid(
            "transfer destination must be empty",
        ));
    }
    let source_contents = stage
        .vessels
        .get(&source)
        .ok_or_else(|| TransitionFailure::invalid("unknown source vessel"))?
        .contents
        .clone();
    if source_contents.is_empty() {
        return Err(TransitionFailure::invalid("transfer source is empty"));
    }
    if let Some(quantity) = quantity {
        partial_transfer(
            experiment,
            catalogue,
            stage,
            operation,
            quantity,
            source,
            destination,
            source_contents,
        )
    } else {
        whole_transfer(
            experiment,
            catalogue,
            stage,
            operation,
            source,
            destination,
            source_contents,
        )
    }
}

fn whole_transfer(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    source: VesselId,
    destination: VesselId,
    contents: Vec<InventoryPortion>,
) -> Result<(), TransitionFailure> {
    let (volume, source_mixing) = {
        let source_state = stage.vessels.get(&source).expect("checked source");
        (
            source_state.total_volume.clone(),
            source_state.mixing.clone(),
        )
    };
    let target = stage
        .vessels
        .get(&destination)
        .expect("checked destination");
    ensure_capacity(target, volume.as_ref(), true)?;
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .contents
        .clear();
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .total_volume = None;
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .mixing = MixingState::Unmixed;
    rebuild_partitions(stage.vessels.get_mut(&source).expect("checked source"));
    let target = stage
        .vessels
        .get_mut(&destination)
        .expect("checked destination");
    target.contents.clone_from(&contents);
    target.total_volume = volume;
    target.mixing = source_mixing;
    rebuild_partitions(target);
    for portion in contents {
        stage.ledger.push(LedgerEntry::Move {
            portion: portion.id,
            operation,
            from: InventoryLocation::InVessel { vessel: source },
            to: InventoryLocation::InVessel {
                vessel: destination,
            },
        });
    }
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        destination,
        OpportunityTrigger::Transfer,
    )?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "partial transfer keeps every source and destination operand explicit"
)]
#[expect(
    clippy::too_many_lines,
    reason = "mobile-phase partition, contact proof, exact split, and replayable lineage are one atomic transition"
)]
fn partial_transfer(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    quantity: &chem_domain::Quantity,
    source: VesselId,
    destination: VesselId,
    contents: Vec<InventoryPortion>,
) -> Result<(), TransitionFailure> {
    let phases = contents
        .iter()
        .flat_map(|portion| &portion.components)
        .map(|component| component.species.phase)
        .collect::<BTreeSet<_>>();
    if phases
        .iter()
        .any(|phase| !matches!(phase, Phase::Liquid | Phase::Aqueous | Phase::Solid))
    {
        return Err(TransitionFailure::unsupported(
            "partial transfer requires one homogeneous mobile phase",
        ));
    }
    let mobile_phases = phases
        .iter()
        .copied()
        .filter(|phase| matches!(phase, Phase::Liquid | Phase::Aqueous))
        .collect::<BTreeSet<_>>();
    if mobile_phases.is_empty() {
        return Err(TransitionFailure::invalid(
            "partial transfer source has no mobile phase",
        ));
    }
    if mobile_phases.len() != 1 {
        return Err(TransitionFailure::unsupported(
            "partial transfer requires exactly one supported mobile phase partition",
        ));
    }
    let mobile_contributors = contents
        .iter()
        .flat_map(|portion| &portion.components)
        .filter(|component| matches!(component.species.phase, Phase::Liquid | Phase::Aqueous))
        .count();
    let source_mixing = stage.vessels[&source].mixing.clone();
    if mobile_contributors > 1
        && !matches!(
            &source_mixing,
            MixingState::HomogeneousContact { mobile_phases: contacted, .. }
                if mobile_phases.is_subset(contacted)
        )
    {
        return Err(TransitionFailure::unsupported(
            "partial transfer requires established homogeneous contact for every mobile phase",
        ));
    }
    let mut mobile = Vec::new();
    for (index, parent) in contents.iter().enumerate() {
        let components = parent
            .components
            .iter()
            .filter(|component| matches!(component.species.phase, Phase::Liquid | Phase::Aqueous))
            .cloned()
            .collect::<Vec<_>>();
        if !components.is_empty() {
            mobile.push(separated_child(
                parent,
                operation,
                "transfer-mobile",
                index,
                components,
            ));
        }
    }
    let available = if phases.contains(&Phase::Solid) {
        derive_total_volume(experiment, catalogue, stage, source, &mobile)?
    } else {
        stage.vessels[&source]
            .total_volume
            .clone()
            .or(derive_total_volume(
                experiment, catalogue, stage, source, &mobile,
            )?)
    }
    .ok_or_else(|| {
        TransitionFailure::unsupported("partial transfer requires known source volume")
    })?;
    let requested = quantity.canonical_value();
    if requested > &available.canonical_value {
        return Err(TransitionFailure::invalid(
            "partial transfer exceeds available volume",
        ));
    }
    if requested == &available.canonical_value && !phases.contains(&Phase::Solid) {
        return whole_transfer(
            experiment,
            catalogue,
            stage,
            operation,
            source,
            destination,
            contents,
        );
    }
    if requested == &available.canonical_value {
        return transfer_all_mobile_with_solids(
            experiment,
            catalogue,
            stage,
            operation,
            source,
            destination,
            &contents,
            available,
            source_mixing,
        );
    }
    ensure_capacity(
        stage
            .vessels
            .get(&destination)
            .expect("checked destination"),
        Some(&authored_volume(quantity)),
        true,
    )?;
    let fraction = requested
        .checked_div(&available.canonical_value)
        .expect("positive available volume");
    let retained_fraction = &ExactScalar::one() - &fraction;
    let mut retained = Vec::new();
    let mut moved = Vec::new();
    for (index, parent) in contents.iter().enumerate() {
        let mobile_components = parent
            .components
            .iter()
            .filter(|component| matches!(component.species.phase, Phase::Liquid | Phase::Aqueous))
            .cloned()
            .collect::<Vec<_>>();
        let solid_components = parent
            .components
            .iter()
            .filter(|component| component.species.phase == Phase::Solid)
            .cloned()
            .collect::<Vec<_>>();
        if mobile_components.is_empty() {
            retained.push(parent.clone());
            continue;
        }
        let mobile_parent = if solid_components.is_empty() {
            parent.clone()
        } else {
            separated_child(
                parent,
                operation,
                "transfer-mobile",
                index,
                mobile_components,
            )
        };
        let split_from = if solid_components.is_empty() {
            InventoryLocation::InVessel { vessel: source }
        } else {
            let solid_child =
                separated_child(parent, operation, "transfer-solid", index, solid_components);
            stage.ledger.push(LedgerEntry::Separate {
                parent: parent.id,
                operation,
                from: InventoryLocation::InVessel { vessel: source },
                products: vec![
                    SeparatedProduct {
                        portion: mobile_parent.id,
                        location: InventoryLocation::SeparatedInto {
                            vessel: source,
                            operation,
                        },
                    },
                    SeparatedProduct {
                        portion: solid_child.id,
                        location: InventoryLocation::SeparatedInto {
                            vessel: source,
                            operation,
                        },
                    },
                ],
            });
            remember_portion(stage, &mobile_parent);
            remember_portion(stage, &solid_child);
            retained.push(solid_child);
            InventoryLocation::SeparatedInto {
                vessel: source,
                operation,
            }
        };
        let retained_child = scaled_child(
            &mobile_parent,
            operation,
            "retained",
            index,
            &retained_fraction,
        );
        let moved_child = scaled_child(&mobile_parent, operation, "moved", index, &fraction);
        stage.ledger.push(LedgerEntry::Split {
            parent: mobile_parent.id,
            retained: retained_child.id,
            moved: moved_child.id,
            operation,
            moved_fraction: fraction.clone(),
            from: split_from,
            retained_at: InventoryLocation::InVessel { vessel: source },
            moved_to: InventoryLocation::InVessel {
                vessel: destination,
            },
        });
        remember_portion(stage, &retained_child);
        remember_portion(stage, &moved_child);
        retained.push(retained_child);
        moved.push(moved_child);
    }
    let source_state = stage.vessels.get_mut(&source).expect("checked source");
    source_state.contents = retained;
    source_state.total_volume = if phases.contains(&Phase::Solid) {
        None
    } else {
        Some(scale_derived(&available, &retained_fraction))
    };
    source_state.mixing = source_mixing.clone();
    rebuild_partitions(source_state);
    let destination_state = stage
        .vessels
        .get_mut(&destination)
        .expect("checked destination");
    destination_state.contents = moved;
    destination_state.total_volume = Some(scale_derived(&available, &fraction));
    destination_state.mixing = source_mixing;
    rebuild_partitions(destination_state);
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        source,
        OpportunityTrigger::Transfer,
    )?;
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        destination,
        OpportunityTrigger::Transfer,
    )?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the heterogeneous whole-mobile transfer records the complete atomic transition"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the atomic transition keeps partitioning, movement, and opportunity construction together"
)]
fn transfer_all_mobile_with_solids(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    source: VesselId,
    destination: VesselId,
    contents: &[InventoryPortion],
    mobile_volume: DerivedQuantity,
    source_mixing: MixingState,
) -> Result<(), TransitionFailure> {
    ensure_capacity(
        stage
            .vessels
            .get(&destination)
            .expect("checked destination"),
        Some(&mobile_volume),
        true,
    )?;
    let mut retained = Vec::new();
    let mut moved = Vec::new();
    for (index, parent) in contents.iter().enumerate() {
        let mobile_components = parent
            .components
            .iter()
            .filter(|component| matches!(component.species.phase, Phase::Liquid | Phase::Aqueous))
            .cloned()
            .collect::<Vec<_>>();
        let solid_components = parent
            .components
            .iter()
            .filter(|component| component.species.phase == Phase::Solid)
            .cloned()
            .collect::<Vec<_>>();
        match (mobile_components.is_empty(), solid_components.is_empty()) {
            (false, false) => {
                let mobile_child = separated_child(
                    parent,
                    operation,
                    "transfer-mobile",
                    index,
                    mobile_components,
                );
                let solid_child =
                    separated_child(parent, operation, "transfer-solid", index, solid_components);
                stage.ledger.push(LedgerEntry::Separate {
                    parent: parent.id,
                    operation,
                    from: InventoryLocation::InVessel { vessel: source },
                    products: vec![
                        SeparatedProduct {
                            portion: mobile_child.id,
                            location: InventoryLocation::SeparatedInto {
                                vessel: destination,
                                operation,
                            },
                        },
                        SeparatedProduct {
                            portion: solid_child.id,
                            location: InventoryLocation::SeparatedInto {
                                vessel: source,
                                operation,
                            },
                        },
                    ],
                });
                remember_portion(stage, &mobile_child);
                remember_portion(stage, &solid_child);
                moved.push(mobile_child);
                retained.push(solid_child);
            }
            (false, true) => {
                stage.ledger.push(LedgerEntry::Move {
                    portion: parent.id,
                    operation,
                    from: InventoryLocation::InVessel { vessel: source },
                    to: InventoryLocation::InVessel {
                        vessel: destination,
                    },
                });
                moved.push(parent.clone());
            }
            (true, false) => retained.push(parent.clone()),
            (true, true) => {
                return Err(TransitionFailure::invalid(
                    "inventory portion has no transferable components",
                ));
            }
        }
    }
    let source_state = stage.vessels.get_mut(&source).expect("checked source");
    source_state.contents = retained;
    source_state.total_volume = None;
    source_state.mixing = MixingState::Unmixed;
    rebuild_partitions(source_state);
    let destination_state = stage
        .vessels
        .get_mut(&destination)
        .expect("checked destination");
    destination_state.contents = moved;
    destination_state.total_volume = Some(mobile_volume);
    destination_state.mixing = source_mixing;
    rebuild_partitions(destination_state);
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        source,
        OpportunityTrigger::Transfer,
    )?;
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        destination,
        OpportunityTrigger::Transfer,
    )?;
    Ok(())
}

fn change_temperature(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    vessel: VesselId,
    target: &chem_domain::TemperaturePoint,
    heating: bool,
) -> Result<(), TransitionFailure> {
    let vessel_state = stage
        .vessels
        .get_mut(&vessel)
        .ok_or_else(|| TransitionFailure::invalid("unknown vessel state"))?;
    let ordering = target.kelvin().cmp(vessel_state.temperature.kelvin());
    if (heating && ordering != std::cmp::Ordering::Greater)
        || (!heating && ordering != std::cmp::Ordering::Less)
    {
        return Err(TransitionFailure::invalid(if heating {
            "heat target must be strictly above the current temperature"
        } else {
            "cool target must be strictly below the current temperature"
        }));
    }
    if vessel_state.closure == ClosureState::Closed
        && vessel_state.contents.iter().any(portion_has_gas)
    {
        return Err(TransitionFailure::unsupported(
            "closed-vessel gas temperature change requires a pressure model",
        ));
    }
    let target_point = ConditionPoint {
        temperature_kelvin: target.kelvin().clone(),
        pressure_pascal: vessel_state.pressure.canonical_value().clone(),
        medium: experiment.environment.medium.clone(),
        phase: None,
    };
    for component in vessel_state
        .contents
        .iter()
        .flat_map(|portion| &portion.components)
    {
        let species = catalogue
            .species(&component.species.id)
            .ok_or_else(|| TransitionFailure::unsupported("unknown catalogue species in vessel"))?;
        let mut species_point = target_point.clone();
        species_point.phase = Some(species.phase);
        if !species.condition.contains(&species_point) {
            return Err(TransitionFailure::unsupported(format!(
                "temperature target is outside the reviewed phase domain for `{}`",
                component.species.id
            )));
        }
    }
    vessel_state.temperature = target.clone();
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        vessel,
        OpportunityTrigger::ThermalChange,
    )?;
    Ok(())
}

fn filter(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    source: VesselId,
    filtrate: VesselId,
    residue: VesselId,
) -> Result<(), TransitionFailure> {
    if BTreeSet::from([source, filtrate, residue]).len() != 3 {
        return Err(TransitionFailure::invalid(
            "filter requires three distinct vessels",
        ));
    }
    ensure_empty_destinations(stage, &[filtrate, residue])?;
    let contents = stage
        .vessels
        .get(&source)
        .ok_or_else(|| TransitionFailure::invalid("unknown source vessel"))?
        .contents
        .clone();
    if contents.is_empty() {
        return Err(TransitionFailure::invalid("filter source is empty"));
    }
    let (mobile, solid) = separate_contents(&contents, operation)?;
    let mobile_volume = if mobile.is_empty() {
        None
    } else {
        Some(
            derive_total_volume(experiment, catalogue, stage, source, &mobile)?.ok_or_else(
                || TransitionFailure::unsupported("filtrate capacity requires known mobile volume"),
            )?,
        )
    };
    if let Some(volume) = &mobile_volume {
        ensure_capacity(
            stage.vessels.get(&filtrate).expect("checked filtrate"),
            Some(volume),
            true,
        )?;
    }
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .contents
        .clear();
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .total_volume = None;
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .mixing = MixingState::Unmixed;
    rebuild_partitions(stage.vessels.get_mut(&source).expect("checked source"));
    stage
        .vessels
        .get_mut(&filtrate)
        .expect("checked filtrate")
        .contents
        .clone_from(&mobile);
    stage
        .vessels
        .get_mut(&filtrate)
        .expect("checked filtrate")
        .total_volume = mobile_volume;
    rebuild_partitions(stage.vessels.get_mut(&filtrate).expect("checked filtrate"));
    stage
        .vessels
        .get_mut(&residue)
        .expect("checked residue")
        .contents
        .clone_from(&solid);
    stage
        .vessels
        .get_mut(&residue)
        .expect("checked residue")
        .total_volume = None;
    rebuild_partitions(stage.vessels.get_mut(&residue).expect("checked residue"));
    record_separations(
        stage, operation, source, &contents, filtrate, &mobile, residue, &solid,
    );
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        filtrate,
        OpportunityTrigger::Separation,
    )?;
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        residue,
        OpportunityTrigger::Separation,
    )?;
    Ok(())
}

fn decant(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    source: VesselId,
    destination: VesselId,
) -> Result<(), TransitionFailure> {
    if source == destination {
        return Err(TransitionFailure::invalid(
            "decant source and destination must differ",
        ));
    }
    ensure_empty_destinations(stage, &[destination])?;
    let contents = stage
        .vessels
        .get(&source)
        .ok_or_else(|| TransitionFailure::invalid("unknown source vessel"))?
        .contents
        .clone();
    let (mobile, solid) = separate_contents(&contents, operation)?;
    if mobile.is_empty() || solid.is_empty() {
        return Err(TransitionFailure::invalid(
            "decant requires a supported solid/liquid partition",
        ));
    }
    let mobile_volume = derive_total_volume(experiment, catalogue, stage, source, &mobile)?
        .ok_or_else(|| {
            TransitionFailure::unsupported("decant capacity requires known mobile volume")
        })?;
    ensure_capacity(
        stage
            .vessels
            .get(&destination)
            .expect("checked destination"),
        Some(&mobile_volume),
        true,
    )?;
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .contents
        .clone_from(&solid);
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .total_volume = None;
    stage
        .vessels
        .get_mut(&source)
        .expect("checked source")
        .mixing = MixingState::Unmixed;
    rebuild_partitions(stage.vessels.get_mut(&source).expect("checked source"));
    stage
        .vessels
        .get_mut(&destination)
        .expect("checked destination")
        .contents
        .clone_from(&mobile);
    stage
        .vessels
        .get_mut(&destination)
        .expect("checked destination")
        .total_volume = Some(mobile_volume);
    rebuild_partitions(
        stage
            .vessels
            .get_mut(&destination)
            .expect("checked destination"),
    );
    record_separations(
        stage,
        operation,
        source,
        &contents,
        destination,
        &mobile,
        source,
        &solid,
    );
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        source,
        OpportunityTrigger::Separation,
    )?;
    add_opportunity(
        experiment,
        catalogue,
        stage,
        operation,
        destination,
        OpportunityTrigger::Separation,
    )?;
    Ok(())
}

fn separate_contents(
    contents: &[InventoryPortion],
    operation: OperationId,
) -> Result<(Vec<InventoryPortion>, Vec<InventoryPortion>), TransitionFailure> {
    let mut mobile = Vec::new();
    let mut solid = Vec::new();
    for (index, parent) in contents.iter().enumerate() {
        if parent
            .components
            .iter()
            .any(|component| component.species.phase == Phase::Gas)
        {
            return Err(TransitionFailure::unsupported(
                "gas-phase material has no filtration or decant partition model",
            ));
        }
        let mobile_components = parent
            .components
            .iter()
            .filter(|component| matches!(component.species.phase, Phase::Liquid | Phase::Aqueous))
            .cloned()
            .collect::<Vec<_>>();
        let solid_components = parent
            .components
            .iter()
            .filter(|component| component.species.phase == Phase::Solid)
            .cloned()
            .collect::<Vec<_>>();
        if !mobile_components.is_empty() {
            mobile.push(separated_child(
                parent,
                operation,
                "mobile",
                index,
                mobile_components,
            ));
        }
        if !solid_components.is_empty() {
            solid.push(separated_child(
                parent,
                operation,
                "solid",
                index,
                solid_components,
            ));
        }
    }
    Ok((mobile, solid))
}

fn separated_child(
    parent: &InventoryPortion,
    operation: OperationId,
    role: &str,
    index: usize,
    components: Vec<AnalyticalComponent>,
) -> InventoryPortion {
    let known_volume = if components == parent.components {
        parent.known_volume.clone()
    } else if components.len() == 1 {
        components[0].volume.clone()
    } else {
        None
    };
    InventoryPortion {
        id: child_portion_id(parent.id, operation, role, index),
        root_material: parent.root_material,
        parent: Some(parent.id),
        known_volume,
        components,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "separation ledger entries record both normative destination vessels explicitly"
)]
fn record_separations(
    stage: &mut Stage,
    operation: OperationId,
    source: VesselId,
    parents: &[InventoryPortion],
    mobile_vessel: VesselId,
    mobile: &[InventoryPortion],
    solid_vessel: VesselId,
    solid: &[InventoryPortion],
) {
    for child in mobile.iter().chain(solid) {
        remember_portion(stage, child);
    }
    for parent in parents {
        let products = mobile
            .iter()
            .filter(|child| child.parent == Some(parent.id))
            .map(|child| SeparatedProduct {
                portion: child.id,
                location: InventoryLocation::SeparatedInto {
                    vessel: mobile_vessel,
                    operation,
                },
            })
            .chain(
                solid
                    .iter()
                    .filter(|child| child.parent == Some(parent.id))
                    .map(|child| SeparatedProduct {
                        portion: child.id,
                        location: InventoryLocation::SeparatedInto {
                            vessel: solid_vessel,
                            operation,
                        },
                    }),
            )
            .collect();
        stage.ledger.push(LedgerEntry::Separate {
            parent: parent.id,
            operation,
            from: InventoryLocation::InVessel { vessel: source },
            products,
        });
    }
}

fn remember_portion(stage: &mut Stage, portion: &InventoryPortion) {
    stage
        .portion_history
        .entry(portion.id)
        .or_insert_with(|| portion.clone());
}

fn ensure_empty_destinations(stage: &Stage, vessels: &[VesselId]) -> Result<(), TransitionFailure> {
    for vessel in vessels {
        let vessel_state = stage
            .vessels
            .get(vessel)
            .ok_or_else(|| TransitionFailure::invalid("unknown destination vessel"))?;
        if !vessel_state.contents.is_empty() {
            return Err(TransitionFailure::invalid(
                "separation destination must be empty",
            ));
        }
    }
    Ok(())
}

fn ensure_capacity(
    vessel: &VesselState,
    total: Option<&DerivedQuantity>,
    required: bool,
) -> Result<(), TransitionFailure> {
    let Some(total) = total else {
        return if required {
            Err(TransitionFailure::unsupported(
                "capacity judgment requires a supported volume premise",
            ))
        } else {
            Ok(())
        };
    };
    if total.canonical_value > *vessel.capacity.canonical_value() {
        return Err(TransitionFailure::invalid("vessel capacity exceeded"));
    }
    Ok(())
}

fn derive_total_volume(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &Stage,
    vessel: VesselId,
    portions: &[InventoryPortion],
) -> Result<Option<DerivedQuantity>, TransitionFailure> {
    if portions.is_empty() {
        return Ok(None);
    }
    if portions.len() == 1 {
        return Ok(portions[0].known_volume.clone());
    }
    let volumes = portions
        .iter()
        .map(|portion| portion.known_volume.as_ref())
        .collect::<Option<Vec<_>>>();
    let Some(volumes) = volumes else {
        return Ok(None);
    };
    let assumption = applicable_volume_assumption(experiment, catalogue, stage, vessel, portions)
        .ok_or_else(|| {
            TransitionFailure::unsupported(
                "combined volume requires a catalogue-backed mixture-volume model or explicit permitted assumption",
            )
        })?;
    let mut premises = BTreeSet::new();
    let mut assumptions = BTreeSet::from([assumption]);
    let mut inputs = Vec::new();
    let mut total = ExactScalar::zero();
    for volume in volumes {
        total = &total + &volume.canonical_value;
        premises.extend(volume.derivation.premises.iter().cloned());
        assumptions.extend(volume.derivation.assumptions.iter().copied());
        inputs.push(DerivedInput {
            role: "componentVolume".to_owned(),
            canonical_value: volume.canonical_value.clone(),
            dimension: Dimension::VOLUME,
        });
    }
    Ok(Some(DerivedQuantity::new(
        total,
        Dimension::VOLUME,
        QuantityDerivation {
            rule: DerivedQuantityRule::MixtureVolume,
            inputs,
            premises,
            assumptions,
        },
    )))
}

fn applicable_volume_assumption(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &Stage,
    vessel: VesselId,
    portions: &[InventoryPortion],
) -> Option<AssumptionPremiseId> {
    experiment.assumptions.iter().find_map(|assumption| {
        let record = catalogue.assumption_kind(&assumption.kind)?;
        if record.proposition != AssumptionPropositionKind::NegligibleVolumeChange
            || !record
                .permitted_goals
                .contains(&AssumptionGoalKind::VolumeComposition)
            || !assumption_target_matches(&assumption.target, stage, vessel, portions)
            || !assumption_stage_matches(assumption.stage.as_ref(), stage, experiment)
        {
            return None;
        }
        let phases = portions
            .iter()
            .flat_map(|portion| &portion.components)
            .map(|component| component.species.phase)
            .collect::<BTreeSet<_>>();
        let phase = (phases.len() == 1).then(|| *phases.first().expect("one phase"));
        let point = ConditionPoint {
            temperature_kelvin: stage.vessels[&vessel].temperature.kelvin().clone(),
            pressure_pascal: stage.vessels[&vessel].pressure.canonical_value().clone(),
            medium: experiment.environment.medium.clone(),
            phase,
        };
        record.condition.contains(&point).then_some(assumption.id)
    })
}

fn assumption_target_matches(
    target: &AssumptionTarget,
    stage: &Stage,
    vessel: VesselId,
    portions: &[InventoryPortion],
) -> bool {
    match target {
        AssumptionTarget::Environment => true,
        AssumptionTarget::Material { id } => {
            portions.iter().all(|portion| portion.root_material == *id)
        }
        AssumptionTarget::Species { material, .. } => portions
            .iter()
            .all(|portion| portion.root_material == *material),
        AssumptionTarget::Vessel { id } => *id == vessel,
        AssumptionTarget::Stage { id } => *id == stage.id,
    }
}

fn assumption_stage_matches(
    reference: Option<&StageReference>,
    stage: &Stage,
    experiment: &TypedExperiment,
) -> bool {
    match reference {
        None => true,
        Some(StageReference::Initial) => stage.ordinal == 0,
        Some(StageReference::Final) => {
            usize::try_from(stage.ordinal).ok() == Some(experiment.procedure.len())
        }
        Some(StageReference::Label { id }) => *id == stage.id,
    }
}

fn scaled_child(
    parent: &InventoryPortion,
    operation: OperationId,
    role: &str,
    index: usize,
    fraction: &ExactScalar,
) -> InventoryPortion {
    InventoryPortion {
        id: child_portion_id(parent.id, operation, role, index),
        root_material: parent.root_material,
        parent: Some(parent.id),
        components: parent
            .components
            .iter()
            .map(|component| scale_component(component, fraction))
            .collect(),
        known_volume: parent
            .known_volume
            .as_ref()
            .map(|volume| scale_derived(volume, fraction)),
    }
}

fn scale_component(component: &AnalyticalComponent, fraction: &ExactScalar) -> AnalyticalComponent {
    AnalyticalComponent {
        species: component.species.clone(),
        amount: component
            .amount
            .as_ref()
            .map(|value| scale_derived(value, fraction)),
        mass: component
            .mass
            .as_ref()
            .map(|value| scale_derived(value, fraction)),
        volume: component
            .volume
            .as_ref()
            .map(|value| scale_derived(value, fraction)),
        concentration: component.concentration.clone(),
    }
}

fn scale_derived(value: &DerivedQuantity, fraction: &ExactScalar) -> DerivedQuantity {
    DerivedQuantity::new(
        &value.canonical_value * fraction,
        value.dimension,
        QuantityDerivation {
            rule: DerivedQuantityRule::ProportionalSplit,
            inputs: vec![
                DerivedInput {
                    role: "source".to_owned(),
                    canonical_value: value.canonical_value.clone(),
                    dimension: value.dimension,
                },
                DerivedInput {
                    role: "fraction".to_owned(),
                    canonical_value: fraction.clone(),
                    dimension: Dimension::DIMENSIONLESS,
                },
            ],
            premises: value.derivation.premises.clone(),
            assumptions: value.derivation.assumptions.clone(),
        },
    )
}

fn child_portion_id(
    parent: InventoryPortionId,
    operation: OperationId,
    role: &str,
    index: usize,
) -> InventoryPortionId {
    DigestId::<InventoryPortionKind>::of_json(&json!({
        "parent": parent,
        "operation": operation,
        "role": role,
        "index": index,
    }))
    .expect("child portion identity is canonical")
}

fn rebuild_partitions(vessel: &mut VesselState) {
    let mut partitions = BTreeMap::new();
    for portion in &vessel.contents {
        for phase in portion
            .components
            .iter()
            .map(|component| component.species.phase)
            .collect::<BTreeSet<_>>()
        {
            partitions
                .entry(phase)
                .or_insert_with(Vec::new)
                .push(portion.id);
        }
    }
    for portions in partitions.values_mut() {
        portions.sort();
        portions.dedup();
    }
    vessel.phase_partitions = partitions
        .into_iter()
        .map(|(phase, portions)| PhasePartition { phase, portions })
        .collect();
}

fn portion_has_gas(portion: &InventoryPortion) -> bool {
    portion
        .components
        .iter()
        .any(|component| component.species.phase == Phase::Gas)
}

fn actual_candidates(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    components: impl IntoIterator<Item = AnalyticalComponent>,
    temperature: &chem_domain::TemperaturePoint,
    pressure: &Quantity,
) -> Result<(Vec<ReactionCandidate>, BTreeSet<FactId>), TransitionFailure> {
    let mut candidates = Vec::new();
    let mut all_premises = BTreeSet::new();
    for component in components {
        let point = ConditionPoint {
            temperature_kelvin: temperature.kelvin().clone(),
            pressure_pascal: pressure.canonical_value().clone(),
            medium: experiment.environment.medium.clone(),
            phase: Some(component.species.phase),
        };
        let species = catalogue.species(&component.species.id).ok_or_else(|| {
            TransitionFailure::unsupported("reaction candidate is absent from the catalogue")
        })?;
        if !species.condition.contains(&point) {
            return Err(TransitionFailure::unsupported(format!(
                "species `{}` is outside its reviewed condition domain",
                component.species.id
            )));
        }
        let dissociations = catalogue
            .applicable_facts_for_species(&component.species.id, &point)
            .into_iter()
            .filter_map(|fact| match &fact.proposition {
                FactProposition::Dissociates {
                    analytical_species,
                    products,
                } if analytical_species == &component.species.id => Some((fact, products)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if dissociations.len() > 1 {
            return Err(TransitionFailure::unsupported(format!(
                "species `{}` has non-unique dissociation premises",
                component.species.id
            )));
        }
        if let Some((fact, products)) = dissociations.first() {
            all_premises.insert(fact.id.clone());
            for product in *products {
                let product_record = catalogue.species(&product.species).ok_or_else(|| {
                    TransitionFailure::unsupported("dissociation product is absent from catalogue")
                })?;
                let mut premises = BTreeSet::from([
                    component.species.identity_premise.clone(),
                    product_record.provenance.id.clone(),
                    fact.id.clone(),
                ]);
                if let Some(amount) = &component.amount {
                    premises.extend(amount.derivation.premises.iter().cloned());
                }
                all_premises.extend(premises.iter().cloned());
                let coefficient = ExactScalar::from_integer(num_bigint::BigInt::from(
                    product.coefficient.value().clone(),
                ));
                candidates.push(ReactionCandidate {
                    species: product.species.clone(),
                    phase: product_record.phase,
                    amount: component
                        .amount
                        .as_ref()
                        .map(|amount| dissociation_amount(amount, &coefficient, &fact.id)),
                    premises,
                });
            }
        } else {
            let mut premises = BTreeSet::from([component.species.identity_premise.clone()]);
            if let Some(amount) = &component.amount {
                premises.extend(amount.derivation.premises.iter().cloned());
            }
            all_premises.extend(premises.iter().cloned());
            candidates.push(ReactionCandidate {
                species: component.species.id,
                phase: component.species.phase,
                amount: component.amount,
                premises,
            });
        }
    }
    candidates.sort_by(|left, right| left.species.cmp(&right.species));
    Ok((candidates, all_premises))
}

fn dissociation_amount(
    analytical: &DerivedQuantity,
    coefficient: &ExactScalar,
    premise: &FactId,
) -> DerivedQuantity {
    let mut premises = analytical.derivation.premises.clone();
    premises.insert(premise.clone());
    DerivedQuantity::new(
        &analytical.canonical_value * coefficient,
        Dimension::AMOUNT,
        QuantityDerivation {
            rule: DerivedQuantityRule::DissociationStoichiometry,
            inputs: vec![
                DerivedInput {
                    role: "analyticalAmount".to_owned(),
                    canonical_value: analytical.canonical_value.clone(),
                    dimension: Dimension::AMOUNT,
                },
                DerivedInput {
                    role: "stoichiometricCoefficient".to_owned(),
                    canonical_value: coefficient.clone(),
                    dimension: Dimension::DIMENSIONLESS,
                },
            ],
            premises,
            assumptions: analytical.derivation.assumptions.clone(),
        },
    )
}

fn matching_coverage(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    candidates: &[ReactionCandidate],
    temperature: &chem_domain::TemperaturePoint,
    pressure: &Quantity,
) -> (Vec<chem_domain::CoverageId>, BTreeSet<ReactionRuleFamily>) {
    if candidates.is_empty() {
        return (Vec::new(), BTreeSet::new());
    }
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.species.clone())
        .collect::<BTreeSet<_>>();
    let mut matched = Vec::new();
    let mut families = BTreeSet::new();
    for coverage in &catalogue.document().coverage {
        let condition_applies = candidates.iter().all(|candidate| {
            coverage.condition.contains(&ConditionPoint {
                temperature_kelvin: temperature.kelvin().clone(),
                pressure_pascal: pressure.canonical_value().clone(),
                medium: experiment.environment.medium.clone(),
                phase: Some(candidate.phase),
            })
        });
        if condition_applies && candidate_ids.is_subset(&coverage.species) {
            matched.push(coverage.id.clone());
            let mut declaration_families = coverage
                .families
                .iter()
                .copied()
                .map(reaction_family)
                .collect::<BTreeSet<_>>();
            for exclusion in &coverage.exclusions {
                if exclusion.species.is_subset(&candidate_ids) {
                    for family in &exclusion.families {
                        declaration_families.remove(&reaction_family(*family));
                    }
                }
            }
            families.extend(declaration_families);
        }
    }
    matched.sort();
    (matched, families)
}

const fn reaction_family(family: ReactionFamily) -> ReactionRuleFamily {
    match family {
        ReactionFamily::Precipitation => ReactionRuleFamily::Precipitation,
        ReactionFamily::StrongAcidBase => ReactionRuleFamily::StrongAcidBase,
        ReactionFamily::CuratedGasFormation => ReactionRuleFamily::CuratedGasFormation,
    }
}

fn add_opportunity(
    experiment: &TypedExperiment,
    catalogue: &ValidatedCatalogue,
    stage: &mut Stage,
    operation: OperationId,
    vessel: VesselId,
    trigger: OpportunityTrigger,
) -> Result<(), TransitionFailure> {
    let vessel_state = stage
        .vessels
        .get(&vessel)
        .expect("opportunity vessel exists");
    let components = vessel_state
        .contents
        .iter()
        .flat_map(|portion| portion.components.clone())
        .collect::<Vec<_>>();
    let (candidates, premises) = actual_candidates(
        experiment,
        catalogue,
        components,
        &vessel_state.temperature,
        &vessel_state.pressure,
    )?;
    let (coverage, mut families) = matching_coverage(
        experiment,
        catalogue,
        &candidates,
        &vessel_state.temperature,
        &vessel_state.pressure,
    );
    if trigger == OpportunityTrigger::Separation {
        families.insert(ReactionRuleFamily::PhasePartition);
    }
    let id = DigestId::<ReactionOpportunityKind>::of_json(&json!({
        "stage": stage.id,
        "operation": operation,
        "vessel": vessel,
        "ordinal": stage.reaction_opportunities.len(),
    }))
    .expect("reaction opportunity identity is canonical");
    stage.reaction_opportunities.push(ReactionOpportunity {
        id,
        operation,
        stage: stage.id,
        vessel,
        trigger,
        candidates,
        temperature: vessel_state.temperature.clone(),
        pressure: vessel_state.pressure.clone(),
        families: families.into_iter().collect(),
        coverage,
        premises,
    });
    Ok(())
}

fn verify_stage_invariants(stage: &Stage) -> Result<(), ()> {
    let mut active_ids = BTreeSet::new();
    for portion in stage
        .unplaced
        .values()
        .chain(stage.vessels.values().flat_map(|vessel| &vessel.contents))
    {
        if !active_ids.insert(portion.id)
            || portion.parent == Some(portion.id)
            || portion
                .known_volume
                .as_ref()
                .is_some_and(|volume| volume.canonical_value.is_negative())
            || portion.components.iter().any(|component| {
                [
                    component.amount.as_ref(),
                    component.mass.as_ref(),
                    component.volume.as_ref(),
                    component.concentration.as_ref(),
                ]
                .into_iter()
                .flatten()
                .any(|quantity| quantity.canonical_value.is_negative())
            })
        {
            return Err(());
        }
    }
    for vessel in stage.vessels.values() {
        if let Some(volume) = &vessel.total_volume
            && (volume.dimension != Dimension::VOLUME
                || volume.canonical_value.is_negative()
                || volume.canonical_value > *vessel.capacity.canonical_value())
        {
            return Err(());
        }
        if vessel.contents.is_empty() && vessel.total_volume.is_some() {
            return Err(());
        }
        let mut rebuilt = vessel.clone();
        rebuild_partitions(&mut rebuilt);
        if rebuilt.phase_partitions != vessel.phase_partitions {
            return Err(());
        }
    }
    if !ledger_replays(stage) {
        return Err(());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum PhysicalLocation {
    Unplaced(MaterialId),
    Vessel(VesselId),
}

fn physical_location(location: &InventoryLocation) -> PhysicalLocation {
    match location {
        InventoryLocation::Unplaced { material } => PhysicalLocation::Unplaced(*material),
        InventoryLocation::InVessel { vessel }
        | InventoryLocation::SeparatedInto { vessel, .. } => PhysicalLocation::Vessel(*vessel),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "all closed ledger entry variants are replayed and quantitatively checked in one exhaustive verifier"
)]
fn ledger_replays(stage: &Stage) -> bool {
    let mut locations = BTreeMap::<InventoryPortionId, InventoryLocation>::new();
    for entry in &stage.ledger {
        match entry {
            LedgerEntry::Initial { portion, material } => {
                if stage
                    .portion_history
                    .get(portion)
                    .is_none_or(|snapshot| snapshot.root_material != *material)
                {
                    return false;
                }
                if locations
                    .insert(
                        *portion,
                        InventoryLocation::Unplaced {
                            material: *material,
                        },
                    )
                    .is_some()
                {
                    return false;
                }
            }
            LedgerEntry::Move {
                portion,
                operation,
                from,
                to,
            } => {
                if !stage.portion_history.contains_key(portion)
                    || !separation_operation_matches(from, *operation)
                    || !separation_operation_matches(to, *operation)
                {
                    return false;
                }
                if locations
                    .get(portion)
                    .is_none_or(|current| physical_location(current) != physical_location(from))
                {
                    return false;
                }
                locations.insert(*portion, to.clone());
            }
            LedgerEntry::Split {
                parent,
                retained,
                moved,
                from,
                retained_at,
                moved_to,
                operation,
                moved_fraction,
                ..
            } => {
                let Some(parent_snapshot) = stage.portion_history.get(parent) else {
                    return false;
                };
                let Some(retained_snapshot) = stage.portion_history.get(retained) else {
                    return false;
                };
                let Some(moved_snapshot) = stage.portion_history.get(moved) else {
                    return false;
                };
                if moved_fraction <= &ExactScalar::zero()
                    || moved_fraction >= &ExactScalar::one()
                    || !portion_matches_scale(parent_snapshot, moved_snapshot, moved_fraction)
                    || !portion_matches_scale(
                        parent_snapshot,
                        retained_snapshot,
                        &(&ExactScalar::one() - moved_fraction),
                    )
                    || !separation_operation_matches(from, *operation)
                    || !separation_operation_matches(retained_at, *operation)
                    || !separation_operation_matches(moved_to, *operation)
                {
                    return false;
                }
                let Some(current) = locations.remove(parent) else {
                    return false;
                };
                if physical_location(&current) != physical_location(from)
                    || retained == moved
                    || locations.insert(*retained, retained_at.clone()).is_some()
                    || locations.insert(*moved, moved_to.clone()).is_some()
                {
                    return false;
                }
            }
            LedgerEntry::Separate {
                parent,
                from,
                products,
                operation,
                ..
            } => {
                let Some(parent_snapshot) = stage.portion_history.get(parent) else {
                    return false;
                };
                let product_snapshots = products
                    .iter()
                    .map(|product| stage.portion_history.get(&product.portion))
                    .collect::<Option<Vec<_>>>();
                let Some(product_snapshots) = product_snapshots else {
                    return false;
                };
                if product_snapshots.iter().any(|product| {
                    product.parent != Some(*parent)
                        || product.root_material != parent_snapshot.root_material
                }) || portion_quantity_totals(std::iter::once(parent_snapshot))
                    != portion_quantity_totals(product_snapshots.iter().copied())
                    || products.iter().any(|product| {
                        !matches!(
                            product.location,
                            InventoryLocation::SeparatedInto {
                                operation: location_operation,
                                ..
                            } if location_operation == *operation
                        )
                    })
                {
                    return false;
                }
                let Some(current) = locations.remove(parent) else {
                    return false;
                };
                if physical_location(&current) != physical_location(from)
                    || products.is_empty()
                    || products.iter().any(|product| {
                        locations
                            .insert(product.portion, product.location.clone())
                            .is_some()
                    })
                {
                    return false;
                }
            }
        }
    }
    let active = stage
        .unplaced
        .iter()
        .map(|(material, portion)| (portion.id, PhysicalLocation::Unplaced(*material)))
        .chain(stage.vessels.iter().flat_map(|(vessel, state)| {
            state
                .contents
                .iter()
                .map(|portion| (portion.id, PhysicalLocation::Vessel(*vessel)))
        }))
        .collect::<BTreeMap<_, _>>();
    if stage
        .unplaced
        .values()
        .chain(stage.vessels.values().flat_map(|vessel| &vessel.contents))
        .any(|portion| stage.portion_history.get(&portion.id) != Some(portion))
    {
        return false;
    }
    let replayed = locations
        .into_iter()
        .map(|(portion, location)| (portion, physical_location(&location)))
        .collect::<BTreeMap<_, _>>();
    active == replayed
}

fn separation_operation_matches(location: &InventoryLocation, operation: OperationId) -> bool {
    !matches!(
        location,
        InventoryLocation::SeparatedInto {
            operation: location_operation,
            ..
        } if *location_operation != operation
    )
}

fn portion_matches_scale(
    parent: &InventoryPortion,
    child: &InventoryPortion,
    fraction: &ExactScalar,
) -> bool {
    child.root_material == parent.root_material
        && child.parent == Some(parent.id)
        && child.components
            == parent
                .components
                .iter()
                .map(|component| scale_component(component, fraction))
                .collect::<Vec<_>>()
        && child.known_volume
            == parent
                .known_volume
                .as_ref()
                .map(|volume| scale_derived(volume, fraction))
}

fn portion_quantity_totals<'a>(
    portions: impl IntoIterator<Item = &'a InventoryPortion>,
) -> BTreeMap<(MaterialId, String, &'static str), ExactScalar> {
    portions
        .into_iter()
        .fold(BTreeMap::new(), |mut totals, portion| {
            for component in &portion.components {
                for (kind, value) in [
                    ("amount", component.amount.as_ref()),
                    ("mass", component.mass.as_ref()),
                    ("volume", component.volume.as_ref()),
                ] {
                    let Some(value) = value else {
                        continue;
                    };
                    let key = (
                        portion.root_material,
                        component.species.id.to_string(),
                        kind,
                    );
                    let total = totals.entry(key).or_insert_with(ExactScalar::zero);
                    *total = &*total + &value.canonical_value;
                }
            }
            totals
        })
}

fn inventory_totals(stage: &Stage) -> BTreeMap<(MaterialId, String, &'static str), ExactScalar> {
    portion_quantity_totals(
        stage
            .unplaced
            .values()
            .chain(stage.vessels.values().flat_map(|vessel| &vessel.contents)),
    )
}

const fn operation_id(operation: &TypedOperation) -> OperationId {
    match operation {
        TypedOperation::Place { id, .. }
        | TypedOperation::Add { id, .. }
        | TypedOperation::Combine { id, .. }
        | TypedOperation::Transfer { id, .. }
        | TypedOperation::Stir { id, .. }
        | TypedOperation::Heat { id, .. }
        | TypedOperation::Cool { id, .. }
        | TypedOperation::Wait { id, .. }
        | TypedOperation::Seal { id, .. }
        | TypedOperation::Open { id, .. }
        | TypedOperation::Filter { id, .. }
        | TypedOperation::Decant { id, .. } => *id,
    }
}

fn converted_origins(experiment: &TypedExperiment) -> BTreeMap<String, Vec<SourceRange>> {
    experiment
        .source_origins
        .iter()
        .map(|(key, spans)| {
            (
                key.clone(),
                spans
                    .iter()
                    .map(|span| SourceRange {
                        start: span.start,
                        end: span.end,
                    })
                    .collect(),
            )
        })
        .collect()
}

fn operation_origin(experiment: &TypedExperiment, operation: OperationId) -> ByteSpan {
    experiment
        .source_origins
        .get(&format!("operation:{operation}"))
        .and_then(|spans| spans.first())
        .copied()
        .unwrap_or_else(|| experiment_origin(experiment))
}

fn experiment_origin(experiment: &TypedExperiment) -> ByteSpan {
    experiment
        .source_origins
        .get(&format!("experiment:{}", experiment.id))
        .and_then(|spans| spans.first())
        .copied()
        .unwrap_or(ByteSpan::empty(0))
}

fn transition_diagnostic(
    _experiment: &TypedExperiment,
    operation: Option<OperationId>,
    failure: &TransitionFailure,
    span: ByteSpan,
) -> ElaborationDiagnostic {
    ElaborationDiagnostic::new(
        match failure.status {
            ElaborationStatus::Unsupported => "CHEMS-S013",
            ElaborationStatus::Invalid => "CHEMS-S012",
            ElaborationStatus::IllTyped => "CHEMS-S011",
        },
        failure.status,
        operation.map_or(failure.summary.clone(), |id| {
            format!("operation `{id}`: {}", failure.summary)
        }),
        span,
    )
}
