use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use chem_catalogue::{
    AssumptionPropositionKind, AssumptionStageScope, AssumptionTargetKind, ConditionPoint,
    FactProposition, SpeciesRecord, ValidatedCatalogue,
};
use chem_domain::{
    AnalyticalComponent, AssumptionKindId, AssumptionPremiseId, AssumptionPremiseKind, Charge,
    ChargeSign, ContentDigest, Count, DerivedInput, DerivedQuantity, DerivedQuantityRule, DigestId,
    Dimension, ExactScalar, ExperimentKind, FactId, FormulaPart, FormulaSegment, FormulaSyntax,
    IdKind, Material, MaterialForm, MaterialId, MaterialKind, OperationKind as OperationIdKind,
    Phase, PreparedComponent, Quantity, QuantityDerivation, ResolvedSpecies, StageId, StageKind,
    TemperaturePoint, TemperatureScale, VesselId, VesselKind,
};
use chems_lang::{
    ByteSpan, ChemicalSyntaxKind, DeclarationKind, Diagnostic, OperationKind, SourceAst,
    SourceExperiment, SourceNode, SourceNodeKind, parse_source,
};
use num_bigint::{BigInt, BigUint};
use serde_json::json;

use crate::{
    AssumptionApplicability, AssumptionTarget, AssumptionUsage, CatalogueBinding,
    DeferredExpectation, DeferredSections, ElaborationDiagnostic, ElaborationStatus, Environment,
    StageReference, TypedAssumption, TypedExperiment, TypedOperation, TypedProcedureStep,
    TypedVessel, VesselClosure,
    source::{
        descendants, direct_child, first_descendant, parse_quantity_parts, qualified_names,
        quantities, species_nodes, stage_references, value_names,
    },
};

/// The source and semantic results of one elaboration attempt.
#[derive(Debug, Clone)]
pub struct ElaborationResult {
    pub typed: Option<TypedExperiment>,
    pub source_diagnostics: Vec<Diagnostic>,
    pub diagnostics: Vec<ElaborationDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalKind {
    Material,
    Vessel,
    Stage,
}

#[derive(Debug, Clone)]
struct LocalDeclaration {
    kind: LocalKind,
    span: ByteSpan,
}

#[derive(Debug)]
struct Namespace {
    declarations: BTreeMap<String, LocalDeclaration>,
    material_ids: BTreeMap<String, MaterialId>,
    vessel_ids: BTreeMap<String, VesselId>,
    stage_ids: BTreeMap<String, StageId>,
}

struct MaterialContext<'a> {
    source_digest: ContentDigest,
    catalogue: &'a ValidatedCatalogue,
    environment: &'a Environment,
    experiment: &'a SourceExperiment,
    namespace: &'a Namespace,
}

/// Parses and elaborates one source file against exactly one validated
/// catalogue bundle.
#[must_use]
pub fn elaborate(source: &str, catalogue: &ValidatedCatalogue) -> ElaborationResult {
    let parsed = parse_source(source);
    if !parsed.diagnostics.is_empty() || !parsed.ast.complete {
        return ElaborationResult {
            typed: None,
            source_diagnostics: parsed.diagnostics,
            diagnostics: Vec::new(),
        };
    }
    let source_digest = ContentDigest::sha256(source.as_bytes());
    let mut diagnostics = Vec::new();
    let typed = elaborate_ast(source_digest, &parsed.ast, catalogue, &mut diagnostics);
    ElaborationResult {
        typed: (!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == chems_lang::Severity::Error))
        .then_some(typed)
        .flatten(),
        source_diagnostics: Vec::new(),
        diagnostics,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the root elaborator intentionally makes every Slice 4 phase and early-exit boundary visible"
)]
fn elaborate_ast(
    source_digest: ContentDigest,
    ast: &SourceAst,
    catalogue: &ValidatedCatalogue,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<TypedExperiment> {
    let language = ast.language.as_ref()?;
    let language_version = language.lexeme.parse::<u32>().ok()?;
    let selection = ast.catalogue.as_ref()?;
    if selection.name != catalogue.document().name
        || !catalogue_version_matches(
            selection.version.as_deref().unwrap_or_default(),
            &catalogue.document().version,
        )
    {
        diagnostics.push(
            ElaborationDiagnostic::new(
                "CHEMS-C018",
                ElaborationStatus::IllTyped,
                format!(
                    "source selects catalogue `{}@{}`, but the bound bundle is `{}@{}`",
                    selection.name,
                    selection.version.as_deref().unwrap_or(""),
                    catalogue.document().name,
                    catalogue.document().version
                ),
                selection.span,
            )
            .with_help("bind the exact reviewed catalogue selected by the source"),
        );
        return None;
    }
    let experiment = ast.experiment.as_ref()?;
    let id = stable_id::<ExperimentKind>(
        source_digest,
        "experiment",
        &experiment.name,
        experiment.span,
    );
    let namespace = collect_namespace(source_digest, experiment, diagnostics);
    if !diagnostics.is_empty() {
        return None;
    }
    let Some((environment, mut origins)) =
        elaborate_environment(experiment, catalogue, diagnostics)
    else {
        if diagnostics.is_empty() {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T006",
                ElaborationStatus::IllTyped,
                "initial environment could not be elaborated",
                experiment.span,
            ));
        }
        return None;
    };
    for (name, id) in &namespace.stage_ids {
        if let Some(declaration) = namespace.declarations.get(name) {
            origins.insert(format!("stage:{id}"), vec![declaration.span]);
        }
    }
    let materials = elaborate_materials(
        source_digest,
        experiment,
        catalogue,
        &environment,
        &namespace,
        diagnostics,
        &mut origins,
    );
    let vessels = elaborate_vessels(experiment, &namespace, diagnostics, &mut origins);
    let assumptions = elaborate_assumptions(
        source_digest,
        experiment,
        catalogue,
        &environment,
        &materials,
        &namespace,
        diagnostics,
        &mut origins,
    );
    let procedure = elaborate_procedure(
        source_digest,
        experiment,
        &namespace,
        diagnostics,
        &mut origins,
    );
    warn_unused_locals(experiment, &namespace, diagnostics);
    let deferred = DeferredSections {
        expectations: experiment
            .expectations
            .iter()
            .map(|expectation| DeferredExpectation {
                stage: expectation.stage.as_deref().and_then(|stage| {
                    resolve_stage_reference(stage, &namespace, diagnostics, expectation.span)
                }),
                source: expectation.clone(),
            })
            .collect(),
        tactics: experiment.tactics.clone(),
    };
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == chems_lang::Severity::Error)
    {
        return None;
    }
    origins.insert(format!("experiment:{id}"), vec![experiment.span]);
    Some(TypedExperiment {
        schema_version: 1,
        language_version,
        source_digest,
        catalogue: CatalogueBinding {
            name: catalogue.document().name.clone(),
            version: catalogue.document().version.clone(),
            digest: catalogue.digest(),
        },
        id,
        name: experiment.name.clone(),
        environment,
        assumptions,
        materials,
        vessels,
        procedure,
        deferred,
        source_origins: origins,
    })
}

fn catalogue_version_matches(selected: &str, available: &str) -> bool {
    let selected = selected.split('.').collect::<Vec<_>>();
    let available = available.split('.').collect::<Vec<_>>();
    !selected.is_empty()
        && selected.len() <= available.len()
        && selected
            .iter()
            .zip(available)
            .all(|(left, right)| left == &right)
}

fn warn_unused_locals(
    experiment: &SourceExperiment,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) {
    let mut referenced = BTreeSet::new();
    for entry in &experiment.procedure {
        if let Some(operation) = first_descendant(
            entry,
            |kind| matches!(kind, SourceNodeKind::Operation { operation } if *operation != OperationKind::Operation),
        ) {
            referenced.extend(
                value_names(operation)
                    .into_iter()
                    .filter_map(|node| node.lexeme.clone()),
            );
        }
    }
    for entry in &experiment.assumptions {
        referenced.extend(assumption_target_name(entry).map(str::to_owned));
        referenced.extend(
            stage_references(entry)
                .into_iter()
                .filter_map(|node| node.lexeme.clone()),
        );
    }
    for expectation in &experiment.expectations {
        referenced.extend(expectation.stage.clone());
        for claim in &expectation.claims {
            referenced.extend(
                value_names(claim)
                    .into_iter()
                    .filter_map(|node| node.lexeme.clone()),
            );
        }
    }
    for tactic in &experiment.tactics {
        referenced.extend(
            value_names(tactic)
                .into_iter()
                .filter_map(|node| node.lexeme.clone()),
        );
    }
    for (name, declaration) in &namespace.declarations {
        if !referenced.contains(name) {
            diagnostics.push(
                ElaborationDiagnostic::warning(
                    "CHEMS-T016",
                    format!(
                        "local {:?} `{name}` is declared but unused",
                        declaration.kind
                    ),
                    declaration.span,
                )
                .with_help("remove the declaration or reference it from the experiment"),
            );
        }
    }
}

fn stable_id<K: IdKind>(
    source_digest: ContentDigest,
    namespace: &str,
    name: &str,
    span: ByteSpan,
) -> DigestId<K> {
    DigestId::of_json(&json!({
        "sourceDigest": source_digest,
        "namespace": namespace,
        "name": name,
        "declarationStart": span.start,
    }))
    .expect("stable identifier input contains no floating-point values")
}

fn assumption_premise_id(
    source_digest: ContentDigest,
    index: usize,
    entry: &SourceNode,
    kind: &AssumptionKindId,
) -> AssumptionPremiseId {
    DigestId::<AssumptionPremiseKind>::of_json(&json!({
        "sourceDigest": source_digest,
        "kind": kind,
        "ordinal": index,
        "declarationStart": entry.span.start,
        "target": assumption_target_name(entry),
        "stage": stage_references(entry).first().and_then(|node| node.lexeme.as_deref()),
    }))
    .expect("assumption premise identity contains no floating-point values")
}

fn collect_namespace(
    source_digest: ContentDigest,
    experiment: &SourceExperiment,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Namespace {
    let mut namespace = Namespace {
        declarations: BTreeMap::new(),
        material_ids: BTreeMap::new(),
        vessel_ids: BTreeMap::new(),
        stage_ids: BTreeMap::new(),
    };
    for entry in &experiment.materials {
        if let Some(name) = value_names(entry).first() {
            let text = name.lexeme.as_deref().unwrap_or_default();
            let id = stable_id::<MaterialKind>(source_digest, "material", text, name.span);
            insert_local(
                &mut namespace,
                text,
                LocalKind::Material,
                name.span,
                diagnostics,
            );
            namespace.material_ids.insert(text.to_owned(), id);
        }
    }
    for entry in &experiment.vessels {
        if let Some(name) = value_names(entry).first() {
            let text = name.lexeme.as_deref().unwrap_or_default();
            let id = stable_id::<VesselKind>(source_digest, "vessel", text, name.span);
            insert_local(
                &mut namespace,
                text,
                LocalKind::Vessel,
                name.span,
                diagnostics,
            );
            namespace.vessel_ids.insert(text.to_owned(), id);
        }
    }
    for entry in &experiment.procedure {
        let Some(label) = first_descendant(entry, |kind| {
            matches!(
                kind,
                SourceNodeKind::Declaration {
                    form: DeclarationKind::StageLabel
                }
            )
        }) else {
            continue;
        };
        let names = value_names(label);
        let Some(name) = names.first() else {
            continue;
        };
        let text = name.lexeme.as_deref().unwrap_or_default();
        let id = stable_id::<StageKind>(source_digest, "stage", text, name.span);
        insert_local(
            &mut namespace,
            text,
            LocalKind::Stage,
            name.span,
            diagnostics,
        );
        namespace.stage_ids.insert(text.to_owned(), id);
    }
    namespace
}

fn insert_local(
    namespace: &mut Namespace,
    name: &str,
    kind: LocalKind,
    span: ByteSpan,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) {
    if matches!(name, "initial" | "final") {
        diagnostics.push(ElaborationDiagnostic::new(
            "CHEMS-T003",
            ElaborationStatus::IllTyped,
            format!("`{name}` is a built-in stage reference and cannot be declared"),
            span,
        ));
        return;
    }
    if let Some(previous) = namespace.declarations.get(name) {
        diagnostics.push(
            ElaborationDiagnostic::new(
                "CHEMS-T003",
                ElaborationStatus::IllTyped,
                format!("duplicate local declaration `{name}`"),
                span,
            )
            .related(previous.span),
        );
        return;
    }
    namespace
        .declarations
        .insert(name.to_owned(), LocalDeclaration { kind, span });
}

fn elaborate_environment(
    experiment: &SourceExperiment,
    catalogue: &ValidatedCatalogue,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<(Environment, BTreeMap<String, Vec<ByteSpan>>)> {
    let mut temperature = Vec::new();
    let mut pressure = Vec::new();
    let mut medium = Vec::new();
    for entry in &experiment.conditions {
        if let Some(node) = direct_child(entry, |kind| {
            matches!(
                kind,
                SourceNodeKind::Declaration {
                    form: DeclarationKind::Temperature
                }
            )
        }) {
            temperature.push(node);
        } else if let Some(node) = direct_child(entry, |kind| {
            matches!(
                kind,
                SourceNodeKind::Declaration {
                    form: DeclarationKind::Pressure
                }
            )
        }) {
            pressure.push(node);
        } else if let Some(node) = direct_child(entry, |kind| {
            matches!(
                kind,
                SourceNodeKind::Declaration {
                    form: DeclarationKind::Medium
                }
            )
        }) {
            medium.push(node);
        }
    }
    let temperature =
        exactly_one_condition("temperature", &temperature, experiment.span, diagnostics)?;
    let pressure = exactly_one_condition("pressure", &pressure, experiment.span, diagnostics)?;
    let medium = exactly_one_condition("medium", &medium, experiment.span, diagnostics)?;
    let temperature_quantity = quantities(temperature).first().copied()?;
    let temperature = elaborate_temperature(temperature_quantity, diagnostics)?;
    let pressure_node = quantities(pressure).first().copied()?;
    let pressure_value = elaborate_quantity(pressure_node, diagnostics)?;
    if pressure_value.dimension() != Dimension::PRESSURE {
        diagnostics.push(wrong_dimension(
            "pressure",
            Dimension::PRESSURE,
            &pressure_value,
            pressure_node.span,
        ));
        return None;
    }
    if !is_positive(pressure_value.canonical_value()) {
        diagnostics.push(positive_diagnostic("pressure", pressure_node.span));
        return None;
    }
    let qualified = qualified_names(medium);
    let (source_name, medium_name_span) =
        qualified.first().map_or(("aqueous", medium.span), |node| {
            (node.lexeme.as_deref().unwrap_or_default(), node.span)
        });
    let medium_record = resolve_medium(catalogue, source_name);
    let Some(medium_record) = medium_record else {
        diagnostics.push(ElaborationDiagnostic::new(
            "CHEMS-T004",
            ElaborationStatus::IllTyped,
            format!("unknown catalogue medium `{source_name}`"),
            medium_name_span,
        ));
        return None;
    };
    let mut origins = BTreeMap::new();
    origins.insert(
        "condition:temperature".to_owned(),
        vec![temperature_quantity.span],
    );
    origins.insert("condition:pressure".to_owned(), vec![pressure_node.span]);
    origins.insert("condition:medium".to_owned(), vec![medium_name_span]);
    Some((
        Environment {
            temperature,
            pressure: pressure_value,
            medium: medium_record.id.clone(),
            solvent: medium_record.solvent.clone(),
            medium_identity_premise: medium_record.provenance.id.clone(),
        },
        origins,
    ))
}

fn exactly_one_condition<'a>(
    name: &str,
    values: &[&'a SourceNode],
    experiment_span: ByteSpan,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<&'a SourceNode> {
    match values {
        [value] => Some(*value),
        [] => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T006",
                ElaborationStatus::IllTyped,
                format!("missing required `{name}` condition"),
                experiment_span,
            ));
            None
        }
        [first, rest @ ..] => {
            for duplicate in rest {
                diagnostics.push(
                    ElaborationDiagnostic::new(
                        "CHEMS-T006",
                        ElaborationStatus::IllTyped,
                        format!("duplicate `{name}` condition"),
                        duplicate.span,
                    )
                    .related(first.span),
                );
            }
            None
        }
    }
}

fn elaborate_quantity(
    node: &SourceNode,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<Quantity> {
    let (decimal, unit) = match parse_quantity_parts(node) {
        Ok(parts) => parts,
        Err(error) => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T005",
                ElaborationStatus::IllTyped,
                error,
                node.span,
            ));
            return None;
        }
    };
    match Quantity::new(decimal, unit) {
        Ok(quantity) => Some(quantity),
        Err(error) => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T005",
                ElaborationStatus::IllTyped,
                error.to_string(),
                node.span,
            ));
            None
        }
    }
}

fn elaborate_temperature(
    node: &SourceNode,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<TemperaturePoint> {
    let (decimal, unit) = match parse_quantity_parts(node) {
        Ok(parts) => parts,
        Err(error) => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T005",
                ElaborationStatus::IllTyped,
                error,
                node.span,
            ));
            return None;
        }
    };
    let unit_source = unit.dividend().factors();
    let scale = match (unit_source, unit.divisors()) {
        ([factor], []) if factor.exponent() == 1 => match factor.symbol() {
            chem_domain::UnitSymbol::Kelvin => Some(TemperatureScale::Kelvin),
            chem_domain::UnitSymbol::DegreesCelsius => Some(TemperatureScale::DegreesCelsius),
            _ => None,
        },
        _ => None,
    };
    let Some(scale) = scale else {
        diagnostics.push(ElaborationDiagnostic::new(
            "CHEMS-T005",
            ElaborationStatus::IllTyped,
            "temperature requires standalone `K` or `degC`",
            node.span,
        ));
        return None;
    };
    match TemperaturePoint::new(decimal, scale) {
        Ok(point) => Some(point),
        Err(error) => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T005",
                ElaborationStatus::IllTyped,
                error.to_string(),
                node.span,
            ));
            None
        }
    }
}

fn resolve_medium<'a>(
    catalogue: &'a ValidatedCatalogue,
    source: &str,
) -> Option<&'a chem_catalogue::MediumRecord> {
    catalogue.medium_by_alias(source).or_else(|| {
        source
            .rsplit('.')
            .next()
            .and_then(|name| catalogue.medium_by_alias(name))
    })
}

fn environment_point(environment: &Environment, phase: Option<Phase>) -> ConditionPoint {
    ConditionPoint {
        temperature_kelvin: environment.temperature.kelvin().clone(),
        pressure_pascal: environment.pressure.canonical_value().clone(),
        medium: environment.medium.clone(),
        phase,
    }
}

fn elaborate_materials(
    source_digest: ContentDigest,
    experiment: &SourceExperiment,
    catalogue: &ValidatedCatalogue,
    environment: &Environment,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    origins: &mut BTreeMap<String, Vec<ByteSpan>>,
) -> Vec<Material> {
    let context = MaterialContext {
        source_digest,
        catalogue,
        environment,
        experiment,
        namespace,
    };
    experiment
        .materials
        .iter()
        .filter_map(|entry| {
            let name_node = value_names(entry).first().copied()?;
            let name = name_node.lexeme.as_deref().unwrap_or_default();
            let id = *namespace.material_ids.get(name)?;
            let material = if first_descendant(entry, |kind| {
                matches!(
                    kind,
                    SourceNodeKind::Declaration {
                        form: DeclarationKind::PreparedMaterial
                    }
                )
            })
            .is_some()
            {
                elaborate_prepared(entry, id, name, &context, diagnostics)
            } else {
                elaborate_simple_material(entry, id, name, &context, diagnostics)
            };
            if let Some(material) = &material {
                record_material_origins(material, entry, origins);
            }
            material
        })
        .collect()
}

fn record_material_origins(
    material: &Material,
    entry: &SourceNode,
    origins: &mut BTreeMap<String, Vec<ByteSpan>>,
) {
    let prefix = format!("material:{}", material.id);
    origins.insert(prefix.clone(), vec![entry.span]);
    origins.insert(format!("{prefix}:constructor"), vec![entry.span]);
    let quantity_spans = quantities(entry)
        .into_iter()
        .map(|node| node.span)
        .collect::<Vec<_>>();
    for (index, span) in quantity_spans.iter().enumerate() {
        origins.insert(format!("{prefix}:quantity:{index}"), vec![*span]);
    }
    for (index, species) in species_nodes(entry).iter().enumerate() {
        origins.insert(format!("{prefix}:species:{index}"), vec![species.span]);
    }
    for (index, node) in descendants(entry, |kind| {
        matches!(
            kind,
            SourceNodeKind::Chemical {
                form: ChemicalSyntaxKind::Formula
                    | ChemicalSyntaxKind::FormulaSegment
                    | ChemicalSyntaxKind::FormulaPart
                    | ChemicalSyntaxKind::Element
                    | ChemicalSyntaxKind::Charge
                    | ChemicalSyntaxKind::Phase
            }
        )
    })
    .iter()
    .enumerate()
    {
        origins.insert(format!("{prefix}:chemical:{index}"), vec![node.span]);
    }
    for premise in &material.required_premises {
        origins.insert(format!("{prefix}:premise:{premise}"), vec![entry.span]);
    }
    for premise in &material.required_assumptions {
        origins.insert(format!("{prefix}:assumption:{premise}"), vec![entry.span]);
    }
    if let MaterialForm::Prepared { components } = &material.form {
        let source_components = descendants(entry, |kind| {
            matches!(
                kind,
                SourceNodeKind::Declaration {
                    form: DeclarationKind::Component
                }
            )
        });
        for (index, component) in components.iter().enumerate() {
            let component_nodes = component
                .source_component_indices
                .iter()
                .filter_map(|source_index| source_components.get(*source_index as usize))
                .collect::<Vec<_>>();
            origins.insert(
                format!("{prefix}:preparedComponent:{index}"),
                component_nodes.iter().map(|node| node.span).collect(),
            );
            let spans = component_nodes
                .iter()
                .flat_map(|node| quantities(node).into_iter().map(|quantity| quantity.span))
                .collect::<Vec<_>>();
            record_derived_component_origins(
                &prefix,
                index,
                &component.analytical,
                &spans,
                origins,
            );
        }
    } else {
        for (index, component) in material.analytical_inventory.iter().enumerate() {
            record_derived_component_origins(&prefix, index, component, &quantity_spans, origins);
        }
    }
}

fn record_derived_component_origins(
    prefix: &str,
    component_index: usize,
    component: &AnalyticalComponent,
    spans: &[ByteSpan],
    origins: &mut BTreeMap<String, Vec<ByteSpan>>,
) {
    for (field, value) in ["amount", "mass", "volume", "concentration"]
        .into_iter()
        .zip([
            component.amount.as_ref(),
            component.mass.as_ref(),
            component.volume.as_ref(),
            component.concentration.as_ref(),
        ])
    {
        if value.is_some() {
            origins.insert(
                format!("{prefix}:component:{component_index}:derived:{field}"),
                spans.to_vec(),
            );
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the closed material-constructor decision table is clearest when kept in one exhaustive match"
)]
fn elaborate_simple_material(
    entry: &SourceNode,
    id: MaterialId,
    name: &str,
    context: &MaterialContext<'_>,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<Material> {
    let quantity_nodes = quantities(entry);
    let species_node = species_nodes(entry).first().copied()?;
    let species = elaborate_species(
        species_node,
        context.catalogue,
        context.environment,
        diagnostics,
    )?;
    let values = quantity_nodes
        .iter()
        .map(|node| elaborate_quantity(node, diagnostics))
        .collect::<Option<Vec<_>>>()?;
    if values
        .iter()
        .any(|quantity| !is_positive(quantity.canonical_value()))
    {
        diagnostics.push(positive_diagnostic("initial material quantity", entry.span));
        return None;
    }
    let mut premises = BTreeSet::from([species.identity_premise.clone()]);
    let mut required_assumptions = BTreeSet::new();
    let (form, inventory) = match values.as_slice() {
        [amount] if amount.dimension() == Dimension::AMOUNT => {
            let analytical = analytical_with_amount(&species, amount.canonical_value().clone());
            (
                MaterialForm::SampleByAmount {
                    species: species.clone(),
                    amount: amount.clone(),
                },
                vec![analytical],
            )
        }
        [mass] if mass.dimension() == Dimension::MASS => {
            let (molar_mass, mass_premises) =
                molar_mass(&species, context.catalogue, entry.span, diagnostics)?;
            premises.extend(mass_premises.iter().cloned());
            let amount_value = mass
                .canonical_value()
                .checked_div(&molar_mass.canonical_value)
                .ok()?;
            let amount = derived_quantity(
                amount_value,
                Dimension::AMOUNT,
                DerivedQuantityRule::AmountFromMass,
                vec![
                    derived_input("mass", mass),
                    derived_input_from("molarMass", &molar_mass),
                ],
                mass_premises.clone(),
                BTreeSet::new(),
            );
            let analytical = AnalyticalComponent {
                species: species.clone(),
                amount: Some(amount.clone()),
                mass: Some(authored_quantity(mass)),
                volume: None,
                concentration: None,
            };
            (
                MaterialForm::SampleByMass {
                    species: species.clone(),
                    mass: mass.clone(),
                    molar_mass,
                    amount,
                },
                vec![analytical],
            )
        }
        [volume] if volume.dimension() == Dimension::VOLUME && species.phase == Phase::Liquid => {
            let (density, density_premise) = density_for(
                &species,
                context.catalogue,
                context.environment,
                entry.span,
                diagnostics,
            )?;
            let (molar_mass, mass_premises) =
                molar_mass(&species, context.catalogue, entry.span, diagnostics)?;
            premises.insert(density_premise.clone());
            premises.extend(mass_premises.iter().cloned());
            let mass_value = volume.canonical_value() * density.canonical_value();
            let amount_value = mass_value.checked_div(&molar_mass.canonical_value).ok()?;
            let mass = derived_quantity(
                mass_value,
                Dimension::MASS,
                DerivedQuantityRule::MassFromVolumeAndDensity,
                vec![
                    derived_input("volume", volume),
                    derived_input("density", &density),
                ],
                BTreeSet::from([density_premise.clone()]),
                BTreeSet::new(),
            );
            let amount = derived_quantity(
                amount_value,
                Dimension::AMOUNT,
                DerivedQuantityRule::AmountFromLiquidVolume,
                vec![
                    derived_input_from("mass", &mass),
                    derived_input_from("molarMass", &molar_mass),
                ],
                mass.derivation
                    .premises
                    .union(&molar_mass.derivation.premises)
                    .cloned()
                    .collect(),
                mass.derivation
                    .assumptions
                    .union(&molar_mass.derivation.assumptions)
                    .copied()
                    .collect(),
            );
            let analytical = AnalyticalComponent {
                species: species.clone(),
                amount: Some(amount.clone()),
                mass: Some(mass.clone()),
                volume: Some(authored_quantity(volume)),
                concentration: None,
            };
            (
                MaterialForm::LiquidSampleByVolume {
                    species: species.clone(),
                    volume: volume.clone(),
                    density,
                    mass,
                    molar_mass,
                    amount,
                },
                vec![analytical],
            )
        }
        [volume] if volume.dimension() == Dimension::VOLUME && species.phase == Phase::Gas => {
            let (premise, assumption) =
                gas_model_for(&species, name, context, entry.span, diagnostics)?;
            premises.extend(premise.iter().cloned());
            required_assumptions.extend(assumption.iter().copied());
            let amount_value =
                ideal_gas_amount(context.environment, volume.canonical_value()).ok()?;
            let amount = derived_quantity(
                amount_value,
                Dimension::AMOUNT,
                DerivedQuantityRule::IdealGasAmount,
                vec![
                    derived_input("pressure", &context.environment.pressure),
                    derived_input("volume", volume),
                    DerivedInput {
                        role: "temperature".to_owned(),
                        canonical_value: context.environment.temperature.kelvin().clone(),
                        dimension: Dimension::TEMPERATURE,
                    },
                ],
                premise.into_iter().collect(),
                assumption.into_iter().collect(),
            );
            let analytical = AnalyticalComponent {
                species: species.clone(),
                amount: Some(amount.clone()),
                mass: None,
                volume: Some(authored_quantity(volume)),
                concentration: None,
            };
            (
                MaterialForm::GasSampleByVolume {
                    species: species.clone(),
                    volume: volume.clone(),
                    amount,
                },
                vec![analytical],
            )
        }
        [volume, concentration]
            if volume.dimension() == Dimension::VOLUME
                && concentration.dimension() == Dimension::CONCENTRATION
                && species.phase == Phase::Aqueous =>
        {
            premises.insert(context.environment.medium_identity_premise.clone());
            let amount_value = volume.canonical_value() * concentration.canonical_value();
            let amount = derived_quantity(
                amount_value,
                Dimension::AMOUNT,
                DerivedQuantityRule::AnalyticalAmount,
                vec![
                    derived_input("volume", volume),
                    derived_input("concentration", concentration),
                ],
                BTreeSet::from([context.environment.medium_identity_premise.clone()]),
                BTreeSet::new(),
            );
            let analytical = AnalyticalComponent {
                species: species.clone(),
                amount: Some(amount.clone()),
                mass: None,
                volume: Some(authored_quantity(volume)),
                concentration: Some(authored_quantity(concentration)),
            };
            (
                MaterialForm::Solution {
                    analytical_species: species.clone(),
                    total_volume: volume.clone(),
                    analytical_concentration: concentration.clone(),
                    analytical_amount: amount,
                    medium: context.environment.medium.clone(),
                    solvent: context.environment.solvent.clone(),
                },
                vec![analytical],
            )
        }
        _ => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T010",
                ElaborationStatus::IllTyped,
                "material quantities and phase do not select a legal constructor",
                entry.span,
            ));
            return None;
        }
    };
    Some(Material {
        id,
        name: name.to_owned(),
        form,
        analytical_inventory: inventory,
        required_premises: premises,
        required_assumptions,
    })
}

fn elaborate_prepared(
    entry: &SourceNode,
    id: MaterialId,
    name: &str,
    context: &MaterialContext<'_>,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<Material> {
    let prepared = first_descendant(entry, |kind| {
        matches!(
            kind,
            SourceNodeKind::Declaration {
                form: DeclarationKind::PreparedMaterial
            }
        )
    })?;
    let components = prepared
        .children
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                SourceNodeKind::Declaration {
                    form: DeclarationKind::Component
                }
            )
        })
        .collect::<Vec<_>>();
    let mut normalized = BTreeMap::<chem_domain::SpeciesId, PreparedComponent>::new();
    let mut premises = BTreeSet::new();
    let mut required_assumptions = BTreeSet::new();
    for (index, component) in components.iter().enumerate() {
        let synthetic_id = stable_id::<MaterialKind>(
            ContentDigest::sha256(name.as_bytes()),
            "prepared-component",
            name,
            component.span,
        );
        let material =
            elaborate_simple_material(component, synthetic_id, name, context, diagnostics)?;
        premises.extend(material.required_premises);
        required_assumptions.extend(material.required_assumptions);
        let Some(analytical) = material.analytical_inventory.into_iter().next() else {
            continue;
        };
        if analytical.concentration.is_some() {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T010",
                ElaborationStatus::IllTyped,
                "prepared components cannot use concentration",
                component.span,
            ));
            return None;
        }
        normalized
            .entry(analytical.species.id.clone())
            .and_modify(|existing| {
                merge_analytical(&mut existing.analytical, &analytical);
                existing
                    .source_component_indices
                    .push(u32::try_from(index).unwrap_or(u32::MAX));
            })
            .or_insert(PreparedComponent {
                analytical,
                source_component_indices: vec![u32::try_from(index).unwrap_or(u32::MAX)],
            });
    }
    if components.len() == 1 {
        diagnostics.push(
            ElaborationDiagnostic::warning(
                "CHEMS-T015",
                "one-component prepared material can be simplified",
                prepared.span,
            )
            .with_help("write the component as a simple material declaration"),
        );
    }
    let components = normalized.into_values().collect::<Vec<_>>();
    let inventory = components
        .iter()
        .map(|component| component.analytical.clone())
        .collect();
    Some(Material {
        id,
        name: name.to_owned(),
        form: MaterialForm::Prepared { components },
        analytical_inventory: inventory,
        required_premises: premises,
        required_assumptions,
    })
}

fn merge_analytical(target: &mut AnalyticalComponent, source: &AnalyticalComponent) {
    merge_derived(&mut target.amount, source.amount.as_ref());
    merge_derived(&mut target.mass, source.mass.as_ref());
    merge_derived(&mut target.volume, source.volume.as_ref());
    merge_derived(&mut target.concentration, source.concentration.as_ref());
}

fn merge_derived(target: &mut Option<DerivedQuantity>, source: Option<&DerivedQuantity>) {
    match (target.as_mut(), source) {
        (Some(target), Some(source)) if target.dimension == source.dimension => {
            let left = target.clone();
            target.canonical_value = &target.canonical_value + &source.canonical_value;
            *target.derivation = QuantityDerivation {
                rule: DerivedQuantityRule::PreparedComponentSum,
                inputs: vec![
                    derived_input_from("left", &left),
                    derived_input_from("right", source),
                ],
                premises: left
                    .derivation
                    .premises
                    .union(&source.derivation.premises)
                    .cloned()
                    .collect(),
                assumptions: left
                    .derivation
                    .assumptions
                    .union(&source.derivation.assumptions)
                    .copied()
                    .collect(),
            };
        }
        (None, Some(source)) => *target = Some(source.clone()),
        _ => {}
    }
}

fn analytical_with_amount(species: &ResolvedSpecies, amount: ExactScalar) -> AnalyticalComponent {
    AnalyticalComponent {
        species: species.clone(),
        amount: Some(derived_quantity(
            amount,
            Dimension::AMOUNT,
            DerivedQuantityRule::AuthoredQuantity,
            Vec::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        )),
        mass: None,
        volume: None,
        concentration: None,
    }
}

fn elaborate_species(
    node: &SourceNode,
    catalogue: &ValidatedCatalogue,
    environment: &Environment,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<ResolvedSpecies> {
    let source = node.lexeme.as_deref()?;
    let (formula_source, charge, phase) = match split_species(source) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T007",
                ElaborationStatus::IllTyped,
                error,
                node.span,
            ));
            return None;
        }
    };
    let formula = match FormulaParser::parse(formula_source).and_then(|syntax| {
        syntax
            .normalize(catalogue)
            .map_err(|error| error.to_string())
    }) {
        Ok(formula) => formula,
        Err(error) => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T007",
                ElaborationStatus::IllTyped,
                error,
                node.span,
            ));
            return None;
        }
    };
    let point = environment_point(environment, Some(phase));
    let Some(record) = catalogue.resolve_species(&formula, &charge, phase, &point) else {
        if let Some(fact) = catalogue.contradicting_phase_fact(&formula, &charge, phase, &point) {
            diagnostics.push(
                ElaborationDiagnostic::new(
                    "CHEMS-T009",
                    ElaborationStatus::Invalid,
                    format!(
                        "explicit phase in `{source}` contradicts reviewed premise `{}`",
                        fact.id
                    ),
                    node.span,
                )
                .with_help(
                    "correct the explicit phase or select a catalogue valid for these conditions",
                ),
            );
            return None;
        }
        diagnostics.push(ElaborationDiagnostic::new(
            "CHEMS-T008",
            ElaborationStatus::Unsupported,
            format!(
                "catalogue has no supported species for `{source}` under the experiment conditions"
            ),
            node.span,
        ));
        return None;
    };
    Some(resolved_species(record, formula))
}

fn resolved_species(
    record: &SpeciesRecord,
    formula: chem_domain::NormalizedFormula,
) -> ResolvedSpecies {
    ResolvedSpecies {
        id: record.id.clone(),
        substance: record.substance.clone(),
        formula,
        charge: record.charge.clone(),
        phase: record.phase,
        identity_premise: record.provenance.id.clone(),
    }
}

fn split_species(source: &str) -> Result<(&str, Charge, Phase), String> {
    let (body, phase) = if let Some(body) = source.strip_suffix("(aq)") {
        (body, Phase::Aqueous)
    } else if let Some(body) = source.strip_suffix("(s)") {
        (body, Phase::Solid)
    } else if let Some(body) = source.strip_suffix("(l)") {
        (body, Phase::Liquid)
    } else if let Some(body) = source.strip_suffix("(g)") {
        (body, Phase::Gas)
    } else {
        return Err("species requires a supported phase suffix".to_owned());
    };
    let Some((formula, charge_source)) = body.split_once('^') else {
        return Ok((body, Charge::neutral(), phase));
    };
    let (magnitude, sign) = charge_source.split_at(charge_source.len().saturating_sub(1));
    let sign = match sign {
        "+" => ChargeSign::Positive,
        "-" => ChargeSign::Negative,
        _ => return Err("invalid species charge sign".to_owned()),
    };
    let magnitude = if magnitude.is_empty() {
        BigUint::from(1_u8)
    } else {
        BigUint::from_str(magnitude).map_err(|error| error.to_string())?
    };
    let charge = Charge::from_magnitude(magnitude, sign).map_err(|error| error.to_string())?;
    Ok((formula, charge, phase))
}

struct FormulaParser<'a> {
    source: &'a [u8],
    position: usize,
}

impl<'a> FormulaParser<'a> {
    fn parse(source: &'a str) -> Result<FormulaSyntax, String> {
        let mut segments = Vec::new();
        for segment in source.split('.') {
            let coefficient_end = segment.bytes().take_while(u8::is_ascii_digit).count();
            let (coefficient, formula) = if coefficient_end == 0 {
                (Count::one(), segment)
            } else {
                (
                    parse_count(&segment[..coefficient_end])?,
                    &segment[coefficient_end..],
                )
            };
            let mut parser = Self {
                source: formula.as_bytes(),
                position: 0,
            };
            let parts = parser.parts(None)?;
            if parser.position != parser.source.len() {
                return Err("formula contains trailing syntax".to_owned());
            }
            segments.push(FormulaSegment { coefficient, parts });
        }
        Ok(FormulaSyntax { segments })
    }

    fn parts(&mut self, terminator: Option<u8>) -> Result<Vec<FormulaPart>, String> {
        let mut parts = Vec::new();
        while self.position < self.source.len() && Some(self.source[self.position]) != terminator {
            if self.source[self.position] == b'(' {
                self.position += 1;
                let nested = self.parts(Some(b')'))?;
                if self.source.get(self.position) != Some(&b')') {
                    return Err("unclosed formula group".to_owned());
                }
                self.position += 1;
                parts.push(FormulaPart::Group {
                    parts: nested,
                    count: self.count()?,
                });
            } else {
                let start = self.position;
                if !self.source[self.position].is_ascii_uppercase() {
                    return Err("formula element must begin with an uppercase letter".to_owned());
                }
                self.position += 1;
                if self
                    .source
                    .get(self.position)
                    .is_some_and(u8::is_ascii_lowercase)
                {
                    self.position += 1;
                }
                let symbol = std::str::from_utf8(&self.source[start..self.position])
                    .map_err(|error| error.to_string())?
                    .parse()
                    .map_err(|error: chem_domain::FormulaError| error.to_string())?;
                parts.push(FormulaPart::Element {
                    symbol,
                    count: self.count()?,
                });
            }
        }
        if parts.is_empty() {
            return Err("formula group cannot be empty".to_owned());
        }
        Ok(parts)
    }

    fn count(&mut self) -> Result<Count, String> {
        let start = self.position;
        while self
            .source
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        if start == self.position {
            Ok(Count::one())
        } else {
            parse_count(
                std::str::from_utf8(&self.source[start..self.position])
                    .map_err(|error| error.to_string())?,
            )
        }
    }
}

fn parse_count(source: &str) -> Result<Count, String> {
    Count::new(BigUint::from_str(source).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn molar_mass(
    species: &ResolvedSpecies,
    catalogue: &ValidatedCatalogue,
    span: ByteSpan,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<(DerivedQuantity, BTreeSet<FactId>)> {
    let mut total_grams = ExactScalar::zero();
    let mut premises = BTreeSet::new();
    let mut inputs = Vec::new();
    for (element, count) in species.formula.composition() {
        let fact = catalogue.document().facts.iter().find(|fact| {
            matches!(fact.proposition, FactProposition::HasAtomicMass { element: candidate, .. } if candidate == *element)
        });
        let Some(fact) = fact else {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T012",
                ElaborationStatus::Unsupported,
                format!(
                    "molar mass requires an atomic-mass premise for element {}",
                    element.atomic_number()
                ),
                span,
            ));
            return None;
        };
        let FactProposition::HasAtomicMass {
            relative_atomic_mass,
            ..
        } = &fact.proposition
        else {
            unreachable!()
        };
        let multiplier = ExactScalar::from_integer(BigInt::from(count.clone()));
        total_grams = &total_grams + &(&relative_atomic_mass.exact_value() * &multiplier);
        premises.insert(fact.id.clone());
        inputs.push(DerivedInput {
            role: format!("atomicMass:{}x{count}", element.atomic_number()),
            canonical_value: relative_atomic_mass.exact_value(),
            dimension: Dimension::DIMENSIONLESS,
        });
    }
    let kilograms = total_grams
        .checked_div(&ExactScalar::from_integer(1000))
        .ok()?;
    Some((
        derived_quantity(
            kilograms,
            Dimension::MOLAR_MASS,
            DerivedQuantityRule::MolarMass,
            inputs,
            premises.clone(),
            BTreeSet::new(),
        ),
        premises,
    ))
}

fn density_for(
    species: &ResolvedSpecies,
    catalogue: &ValidatedCatalogue,
    environment: &Environment,
    span: ByteSpan,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<(Quantity, FactId)> {
    let point = environment_point(environment, Some(species.phase));
    let fact = catalogue.document().facts.iter().find(|fact| {
        matches!(&fact.proposition, FactProposition::HasDensity { substance, .. } if substance == &species.substance)
            && fact.condition.contains(&point)
    });
    let Some(fact) = fact else {
        diagnostics.push(ElaborationDiagnostic::new(
            "CHEMS-T012",
            ElaborationStatus::Unsupported,
            format!(
                "liquid volume requires an applicable density premise for `{}`",
                species.substance
            ),
            span,
        ));
        return None;
    };
    let FactProposition::HasDensity { density, .. } = &fact.proposition else {
        unreachable!()
    };
    Some(((**density).clone(), fact.id.clone()))
}

fn gas_model_for(
    species: &ResolvedSpecies,
    material_name: &str,
    context: &MaterialContext<'_>,
    span: ByteSpan,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
) -> Option<(Option<FactId>, Option<AssumptionPremiseId>)> {
    let point = environment_point(context.environment, Some(Phase::Gas));
    let fact = context.catalogue.document().facts.iter().find(|fact| {
        matches!(&fact.proposition, FactProposition::SupportsGasModel { species: candidate } if candidate == &species.id)
            && fact.condition.contains(&point)
    });
    if let Some(fact) = fact {
        return Some((Some(fact.id.clone()), None));
    }
    if let Some(assumption) = admitted_ideal_gas_assumption(
        context.source_digest,
        material_name,
        context.catalogue,
        context.environment,
        context.experiment,
        context.namespace,
    ) {
        return Some((None, Some(assumption)));
    }
    diagnostics.push(
        ElaborationDiagnostic::new(
            "CHEMS-T012",
            ElaborationStatus::Unsupported,
            format!(
                "gas volume requires an applicable gas-model premise for `{}`",
                species.id
            ),
            span,
        )
        .with_help(
            "add a reviewed gas-model fact or explicitly admit a permitted ideal-gas assumption",
        ),
    );
    None
}

fn admitted_ideal_gas_assumption(
    source_digest: ContentDigest,
    material_name: &str,
    catalogue: &ValidatedCatalogue,
    environment: &Environment,
    experiment: &SourceExperiment,
    namespace: &Namespace,
) -> Option<AssumptionPremiseId> {
    experiment
        .assumptions
        .iter()
        .enumerate()
        .find_map(|(index, entry)| {
            let name = qualified_names(entry)
                .first()
                .and_then(|node| node.lexeme.as_deref())?;
            let record = assumption_record(catalogue, name)?;
            if record.proposition != AssumptionPropositionKind::IdealGasBehaviour
                || !record
                    .condition
                    .contains(&environment_point(environment, Some(Phase::Gas)))
            {
                return None;
            }
            let target_name = assumption_target_name(entry);
            let target_matches = match record.required_target {
                AssumptionTargetKind::Environment => target_name.is_none(),
                AssumptionTargetKind::Species => {
                    target_name == Some(material_name)
                        && namespace.material_ids.contains_key(material_name)
                }
                _ => false,
            };
            let stage_matches = match stage_references(entry).first() {
                None => record.stage_scope == AssumptionStageScope::Initial,
                Some(stage) => {
                    stage.lexeme.as_deref() == Some("initial")
                        && record.stage_scope == AssumptionStageScope::Initial
                }
            };
            (target_matches && stage_matches)
                .then(|| assumption_premise_id(source_digest, index, entry, &record.id))
        })
}

fn ideal_gas_amount(
    environment: &Environment,
    volume: &ExactScalar,
) -> Result<ExactScalar, chem_domain::ScalarError> {
    let numerator = environment.pressure.canonical_value() * volume;
    let gas_constant = ExactScalar::new(
        BigInt::from(207_861_565_453_831_i64),
        BigInt::from(25_000_000_000_000_i64),
    )?;
    let denominator = &gas_constant * environment.temperature.kelvin();
    numerator.checked_div(&denominator)
}

fn elaborate_vessels(
    experiment: &SourceExperiment,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    origins: &mut BTreeMap<String, Vec<ByteSpan>>,
) -> Vec<TypedVessel> {
    experiment
        .vessels
        .iter()
        .filter_map(|entry| {
            let name_node = value_names(entry).first().copied()?;
            let name = name_node.lexeme.as_deref().unwrap_or_default();
            let id = *namespace.vessel_ids.get(name)?;
            let openness = first_descendant(entry, |kind| {
                matches!(
                    kind,
                    SourceNodeKind::Declaration {
                        form: DeclarationKind::Openness
                    }
                )
            })?;
            let closure = match openness.lexeme.as_deref() {
                Some("open") => VesselClosure::Open,
                Some("closed") => VesselClosure::Closed,
                _ => return None,
            };
            let quantity_node = quantities(entry).first().copied()?;
            let capacity = elaborate_quantity(quantity_node, diagnostics)?;
            if capacity.dimension() != Dimension::VOLUME {
                diagnostics.push(wrong_dimension(
                    "vessel capacity",
                    Dimension::VOLUME,
                    &capacity,
                    quantity_node.span,
                ));
                return None;
            }
            if !is_positive(capacity.canonical_value()) {
                diagnostics.push(positive_diagnostic("vessel capacity", quantity_node.span));
                return None;
            }
            origins.insert(format!("vessel:{id}"), vec![entry.span]);
            Some(TypedVessel {
                id,
                name: name.to_owned(),
                closure,
                capacity,
            })
        })
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "assumption resolution keeps the closed schema, target, stage, applicability, use, and origin checks together"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "assumption elaboration requires the complete typed experiment context and diagnostic sinks"
)]
fn elaborate_assumptions(
    source_digest: ContentDigest,
    experiment: &SourceExperiment,
    catalogue: &ValidatedCatalogue,
    environment: &Environment,
    materials: &[Material],
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    origins: &mut BTreeMap<String, Vec<ByteSpan>>,
) -> Vec<TypedAssumption> {
    let used_premises = materials
        .iter()
        .flat_map(|material| material.required_assumptions.iter().copied())
        .collect::<BTreeSet<_>>();
    experiment
        .assumptions
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let name_node = qualified_names(entry).first().copied()?;
            let source_name = name_node.lexeme.as_deref().unwrap_or_default();
            let record = assumption_record(catalogue, source_name);
            let Some(record) = record else {
                diagnostics.push(ElaborationDiagnostic::new(
                    "CHEMS-T013",
                    ElaborationStatus::IllTyped,
                    format!("unknown catalogue assumption kind `{source_name}`"),
                    name_node.span,
                ));
                return None;
            };
            let premise_id = assumption_premise_id(source_digest, index, entry, &record.id);
            let target_name = assumption_target_name(entry);
            let target = resolve_assumption_target(
                record.required_target,
                target_name,
                materials,
                namespace,
                diagnostics,
                entry.span,
            )?;
            let stage = stage_references(entry).first().and_then(|node| {
                resolve_stage_reference(
                    node.lexeme.as_deref().unwrap_or_default(),
                    namespace,
                    diagnostics,
                    node.span,
                )
            });
            if !stage_scope_accepts(record.stage_scope, stage.as_ref()) {
                diagnostics.push(ElaborationDiagnostic::new(
                    "CHEMS-T013",
                    ElaborationStatus::IllTyped,
                    format!(
                        "assumption `{}` does not permit that stage scope",
                        record.id
                    ),
                    entry.span,
                ));
                return None;
            }
            let mut phase_independent_domain = record.condition.clone();
            phase_independent_domain.phases = None;
            if !phase_independent_domain.contains(&environment_point(environment, None)) {
                diagnostics.push(ElaborationDiagnostic::new(
                    "CHEMS-T013",
                    ElaborationStatus::Invalid,
                    format!(
                        "assumption `{}` is outside its reviewed condition domain",
                        record.id
                    ),
                    entry.span,
                ));
                return None;
            }
            let phase = if record.condition.phases.is_none() {
                None
            } else if record.proposition == AssumptionPropositionKind::IdealGasBehaviour
                && matches!(&target, AssumptionTarget::Environment)
            {
                Some(Phase::Gas)
            } else if let Ok(phase) = resolved_target_phase(&target, materials) {
                phase
            } else {
                diagnostics.push(ElaborationDiagnostic::new(
                    "CHEMS-T013",
                    ElaborationStatus::Unsupported,
                    format!(
                        "assumption `{}` requires one unambiguous target phase",
                        record.id
                    ),
                    entry.span,
                ));
                return None;
            };
            let applicability = match (record.condition.phases.is_some(), phase) {
                (false, _) => AssumptionApplicability::Applicable,
                (true, Some(phase))
                    if record
                        .condition
                        .contains(&environment_point(environment, Some(phase))) =>
                {
                    AssumptionApplicability::Applicable
                }
                (true, Some(_)) => {
                    diagnostics.push(ElaborationDiagnostic::new(
                        "CHEMS-T013",
                        ElaborationStatus::Invalid,
                        format!(
                            "assumption `{}` is outside its reviewed phase domain",
                            record.id
                        ),
                        entry.span,
                    ));
                    return None;
                }
                (true, None) => AssumptionApplicability::DeferredToProcedure,
            };
            let used = used_premises.contains(&premise_id);
            let usage = if used {
                AssumptionUsage::UsedInMaterialElaboration
            } else if record.proposition == AssumptionPropositionKind::IdealGasBehaviour {
                AssumptionUsage::Unused
            } else {
                AssumptionUsage::DeferredToProcedure
            };
            if usage == AssumptionUsage::Unused {
                diagnostics.push(
                    ElaborationDiagnostic::warning(
                        "CHEMS-T015",
                        format!("assumption `{}` is declared but unused", record.id),
                        entry.span,
                    )
                    .with_help("remove it or use it to discharge an applicable premise"),
                );
            }
            origins.insert(
                format!("assumption:{index}:{}", record.id),
                vec![entry.span],
            );
            Some(TypedAssumption {
                id: premise_id,
                kind: record.id.clone(),
                required_target: record.required_target,
                stage_scope: record.stage_scope,
                safety: record.safety,
                target,
                stage,
                applicability,
                usage,
            })
        })
        .collect()
}

fn assumption_target_name(entry: &SourceNode) -> Option<&str> {
    let kind_span = qualified_names(entry).first()?.span;
    value_names(entry)
        .into_iter()
        .find(|node| node.span.start >= kind_span.end)
        .and_then(|node| node.lexeme.as_deref())
}

fn resolved_target_phase(
    target: &AssumptionTarget,
    materials: &[Material],
) -> Result<Option<Phase>, ()> {
    let material = match target {
        AssumptionTarget::Material { id } => materials.iter().find(|material| material.id == *id),
        AssumptionTarget::Species { material, species } => {
            let material = materials.iter().find(|candidate| candidate.id == *material);
            return Ok(material.and_then(|material| {
                material
                    .analytical_inventory
                    .iter()
                    .find(|component| component.species.id == *species)
                    .map(|component| component.species.phase)
            }));
        }
        AssumptionTarget::Environment
        | AssumptionTarget::Vessel { .. }
        | AssumptionTarget::Stage { .. } => return Ok(None),
    };
    let phases = material
        .into_iter()
        .flat_map(|material| &material.analytical_inventory)
        .map(|component| component.species.phase)
        .collect::<BTreeSet<_>>();
    match phases.len() {
        0 => Ok(None),
        1 => Ok(phases.first().copied()),
        _ => Err(()),
    }
}

fn assumption_record<'a>(
    catalogue: &'a ValidatedCatalogue,
    source: &str,
) -> Option<&'a chem_catalogue::AssumptionKindRecord> {
    [source, source.rsplit('.').next().unwrap_or(source)]
        .into_iter()
        .find_map(|candidate| {
            chem_domain::AssumptionKindId::from_str(candidate)
                .ok()
                .and_then(|id| catalogue.assumption_kind(&id))
        })
}

fn resolve_assumption_target(
    required: AssumptionTargetKind,
    source: Option<&str>,
    materials: &[Material],
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    span: ByteSpan,
) -> Option<AssumptionTarget> {
    match (required, source) {
        (AssumptionTargetKind::Environment, None) => Some(AssumptionTarget::Environment),
        (AssumptionTargetKind::Material, Some(name)) => namespace
            .material_ids
            .get(name)
            .copied()
            .map(|id| AssumptionTarget::Material { id })
            .or_else(|| {
                assumption_target_error(name, required, namespace, diagnostics, span);
                None
            }),
        (AssumptionTargetKind::Species, Some(name)) => {
            let material_id = namespace.material_ids.get(name).copied().or_else(|| {
                assumption_target_error(name, required, namespace, diagnostics, span);
                None
            })?;
            let material = materials
                .iter()
                .find(|material| material.id == material_id)?;
            let species = material
                .analytical_inventory
                .iter()
                .map(|component| &component.species.id)
                .collect::<BTreeSet<_>>();
            let mut species = species.into_iter();
            let Some(resolved_species) = species.next() else {
                diagnostics.push(ElaborationDiagnostic::new(
                    "CHEMS-T013",
                    ElaborationStatus::IllTyped,
                    format!(
                        "species-targeted assumption requires `{name}` to resolve to exactly one species"
                    ),
                    span,
                ));
                return None;
            };
            if species.next().is_some() {
                diagnostics.push(ElaborationDiagnostic::new(
                    "CHEMS-T013",
                    ElaborationStatus::IllTyped,
                    format!(
                        "species-targeted assumption requires `{name}` to resolve to exactly one species"
                    ),
                    span,
                ));
                return None;
            }
            Some(AssumptionTarget::Species {
                material: material_id,
                species: resolved_species.clone(),
            })
        }
        (AssumptionTargetKind::Vessel, Some(name)) => namespace
            .vessel_ids
            .get(name)
            .copied()
            .map(|id| AssumptionTarget::Vessel { id })
            .or_else(|| {
                assumption_target_error(name, required, namespace, diagnostics, span);
                None
            }),
        (AssumptionTargetKind::Stage, Some(name)) => namespace
            .stage_ids
            .get(name)
            .copied()
            .map(|id| AssumptionTarget::Stage { id })
            .or_else(|| {
                assumption_target_error(name, required, namespace, diagnostics, span);
                None
            }),
        _ => {
            diagnostics.push(ElaborationDiagnostic::new(
                "CHEMS-T013",
                ElaborationStatus::IllTyped,
                "assumption target is missing or forbidden for that assumption kind",
                span,
            ));
            None
        }
    }
}

fn assumption_target_error(
    name: &str,
    expected: AssumptionTargetKind,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    span: ByteSpan,
) {
    let mut diagnostic = ElaborationDiagnostic::new(
        "CHEMS-T013",
        ElaborationStatus::IllTyped,
        namespace.declarations.get(name).map_or_else(
            || format!("unknown assumption target `{name}`"),
            |actual| {
                format!(
                    "assumption target `{name}` is a {:?}, but the assumption requires {expected:?}",
                    actual.kind
                )
            },
        ),
        span,
    );
    if let Some(actual) = namespace.declarations.get(name) {
        diagnostic = diagnostic.related(actual.span);
    }
    diagnostics.push(diagnostic);
}

fn stage_scope_accepts(scope: AssumptionStageScope, stage: Option<&StageReference>) -> bool {
    match scope {
        AssumptionStageScope::Initial => {
            stage.is_none_or(|stage| matches!(stage, StageReference::Initial))
        }
        AssumptionStageScope::SingleStage | AssumptionStageScope::RemainingProcedure => {
            stage.is_some()
        }
    }
}

fn resolve_stage_reference(
    source: &str,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    span: ByteSpan,
) -> Option<StageReference> {
    match source {
        "initial" => Some(StageReference::Initial),
        "final" => Some(StageReference::Final),
        name => namespace
            .stage_ids
            .get(name)
            .copied()
            .map(|id| StageReference::Label { id })
            .or_else(|| {
                unknown_local(name, LocalKind::Stage, namespace, diagnostics, span);
                None
            }),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the closed procedure-operand typing table is intentionally exhaustive and execution-free"
)]
fn elaborate_procedure(
    source_digest: ContentDigest,
    experiment: &SourceExperiment,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    origins: &mut BTreeMap<String, Vec<ByteSpan>>,
) -> Vec<TypedProcedureStep> {
    experiment.procedure.iter().enumerate().filter_map(|(index, entry)| {
        let operation = first_descendant(entry, |kind| {
            matches!(kind, SourceNodeKind::Operation { operation } if *operation != OperationKind::Operation)
        })?;
        let SourceNodeKind::Operation {
            operation: operation_kind,
        } = operation.kind
        else {
            return None;
        };
        let id = stable_id::<OperationIdKind>(
            source_digest,
            "operation",
            &index.to_string(),
            entry.span,
        );
        let names = value_names(operation);
        let name = |position: usize| names.get(position).and_then(|node| node.lexeme.as_deref()).zip(names.get(position).map(|node| node.span));
        let typed = match operation_kind {
            OperationKind::Place | OperationKind::Add => {
                let (material, material_span) = name(0)?;
                let (vessel, vessel_span) = name(1)?;
                let material = resolve_material(material, namespace, diagnostics, material_span)?;
                let vessel = resolve_vessel(vessel, namespace, diagnostics, vessel_span)?;
                if operation_kind == OperationKind::Place {
                    TypedOperation::Place { id, material, vessel }
                } else {
                    TypedOperation::Add { id, material, vessel }
                }
            }
            OperationKind::Combine => {
                let (left, left_span) = name(0)?;
                let (right, right_span) = name(1)?;
                let (vessel, vessel_span) = name(2)?;
                TypedOperation::Combine {
                    id,
                    left: resolve_material(left, namespace, diagnostics, left_span)?,
                    right: resolve_material(right, namespace, diagnostics, right_span)?,
                    vessel: resolve_vessel(vessel, namespace, diagnostics, vessel_span)?,
                }
            }
            OperationKind::Transfer => {
                let quantity = quantities(operation).first().and_then(|node| elaborate_quantity(node, diagnostics));
                if let Some(quantity) = &quantity
                    && (quantity.dimension() != Dimension::VOLUME
                        || !is_positive(quantity.canonical_value()))
                {
                    diagnostics.push(ElaborationDiagnostic::new(
                        "CHEMS-T005",
                        ElaborationStatus::IllTyped,
                        "partial transfer requires a positive volume",
                        operation.span,
                    ));
                    return None;
                }
                let (source, source_span) = name(0)?;
                let (destination, destination_span) = name(1)?;
                TypedOperation::Transfer {
                    id,
                    quantity,
                    source: resolve_vessel(source, namespace, diagnostics, source_span)?,
                    destination: resolve_vessel(destination, namespace, diagnostics, destination_span)?,
                }
            }
            OperationKind::Stir => {
                let (vessel, span) = name(0)?;
                let duration = quantities(operation).first().and_then(|node| elaborate_quantity(node, diagnostics));
                if duration.as_ref().is_some_and(|duration| duration.dimension() != Dimension::TIME || duration.canonical_value().is_negative()) {
                    diagnostics.push(wrong_operation_quantity("stir duration", operation.span));
                    return None;
                }
                TypedOperation::Stir { id, vessel: resolve_vessel(vessel, namespace, diagnostics, span)?, duration }
            }
            OperationKind::Heat | OperationKind::Cool => {
                let (vessel, span) = name(0)?;
                let target_node = quantities(operation).first().copied()?;
                let target = elaborate_temperature(target_node, diagnostics)?;
                let vessel = resolve_vessel(vessel, namespace, diagnostics, span)?;
                if operation_kind == OperationKind::Heat {
                    TypedOperation::Heat { id, vessel, target }
                } else {
                    TypedOperation::Cool { id, vessel, target }
                }
            }
            OperationKind::Wait => {
                let node = quantities(operation).first().copied()?;
                let duration = elaborate_quantity(node, diagnostics)?;
                if duration.dimension() != Dimension::TIME || duration.canonical_value().is_negative() {
                    diagnostics.push(wrong_operation_quantity("wait duration", node.span));
                    return None;
                }
                TypedOperation::Wait { id, duration }
            }
            OperationKind::Seal | OperationKind::Open => {
                let (vessel, span) = name(0)?;
                let vessel = resolve_vessel(vessel, namespace, diagnostics, span)?;
                if operation_kind == OperationKind::Seal {
                    TypedOperation::Seal { id, vessel }
                } else {
                    TypedOperation::Open { id, vessel }
                }
            }
            OperationKind::Filter => {
                let (source, source_span) = name(0)?;
                let (filtrate, filtrate_span) = name(1)?;
                let (residue, residue_span) = name(2)?;
                TypedOperation::Filter {
                    id,
                    source: resolve_vessel(source, namespace, diagnostics, source_span)?,
                    filtrate: resolve_vessel(filtrate, namespace, diagnostics, filtrate_span)?,
                    residue: resolve_vessel(residue, namespace, diagnostics, residue_span)?,
                }
            }
            OperationKind::Decant => {
                let (source, source_span) = name(0)?;
                let (destination, destination_span) = name(1)?;
                TypedOperation::Decant {
                    id,
                    source: resolve_vessel(source, namespace, diagnostics, source_span)?,
                    destination: resolve_vessel(destination, namespace, diagnostics, destination_span)?,
                }
            }
            OperationKind::Operation => return None,
        };
        origins.insert(format!("operation:{id}"), vec![entry.span]);
        for (operand_index, operand) in names.iter().enumerate() {
            origins.insert(
                format!("operation:{id}:operand:{operand_index}"),
                vec![operand.span],
            );
        }
        for (quantity_index, quantity) in quantities(operation).iter().enumerate() {
            origins.insert(
                format!("operation:{id}:quantity:{quantity_index}"),
                vec![quantity.span],
            );
        }
        let source_label = first_descendant(entry, |kind| {
            matches!(kind, SourceNodeKind::Declaration { form: DeclarationKind::StageLabel })
        })
        .and_then(|label| value_names(label).first().copied())
        .and_then(|name| name.lexeme.clone());
        let resulting_stage = source_label
            .as_ref()
            .and_then(|label| namespace.stage_ids.get(label).copied())
            .unwrap_or_else(|| stable_id::<StageKind>(source_digest, "stage", &index.to_string(), entry.span));
        origins.insert(format!("operation:{id}:resultingStage"), vec![entry.span]);
        Some(TypedProcedureStep {
            resulting_stage,
            source_label,
            operation: typed,
        })
    }).collect()
}

fn resolve_material(
    name: &str,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    span: ByteSpan,
) -> Option<MaterialId> {
    namespace.material_ids.get(name).copied().or_else(|| {
        unknown_local(name, LocalKind::Material, namespace, diagnostics, span);
        None
    })
}

fn resolve_vessel(
    name: &str,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    span: ByteSpan,
) -> Option<VesselId> {
    namespace.vessel_ids.get(name).copied().or_else(|| {
        unknown_local(name, LocalKind::Vessel, namespace, diagnostics, span);
        None
    })
}

fn unknown_local(
    name: &str,
    expected: LocalKind,
    namespace: &Namespace,
    diagnostics: &mut Vec<ElaborationDiagnostic>,
    span: ByteSpan,
) {
    let summary = namespace.declarations.get(name).map_or_else(
        || format!("unknown local name `{name}`"),
        |actual| format!("`{name}` is a {:?}, expected a {expected:?}", actual.kind),
    );
    let mut diagnostic =
        ElaborationDiagnostic::new("CHEMS-T004", ElaborationStatus::IllTyped, summary, span);
    if let Some(actual) = namespace.declarations.get(name) {
        diagnostic = diagnostic.related(actual.span);
    }
    diagnostics.push(diagnostic);
}

fn wrong_dimension(
    context: &str,
    expected: Dimension,
    actual: &Quantity,
    span: ByteSpan,
) -> ElaborationDiagnostic {
    ElaborationDiagnostic::new(
        "CHEMS-T005",
        ElaborationStatus::IllTyped,
        format!(
            "{context} requires {expected:?}, found {:?}",
            actual.dimension()
        ),
        span,
    )
}

fn positive_diagnostic(context: &str, span: ByteSpan) -> ElaborationDiagnostic {
    ElaborationDiagnostic::new(
        "CHEMS-T011",
        ElaborationStatus::IllTyped,
        format!("{context} must be greater than zero"),
        span,
    )
}

fn wrong_operation_quantity(context: &str, span: ByteSpan) -> ElaborationDiagnostic {
    ElaborationDiagnostic::new(
        "CHEMS-T005",
        ElaborationStatus::IllTyped,
        format!("{context} has the wrong dimension or a negative value"),
        span,
    )
}

fn is_positive(value: &ExactScalar) -> bool {
    !value.is_zero() && !value.is_negative()
}

fn derived_input(role: &str, quantity: &Quantity) -> DerivedInput {
    DerivedInput {
        role: role.to_owned(),
        canonical_value: quantity.canonical_value().clone(),
        dimension: quantity.dimension(),
    }
}

fn derived_input_from(role: &str, quantity: &DerivedQuantity) -> DerivedInput {
    DerivedInput {
        role: role.to_owned(),
        canonical_value: quantity.canonical_value.clone(),
        dimension: quantity.dimension,
    }
}

fn authored_quantity(quantity: &Quantity) -> DerivedQuantity {
    derived_quantity(
        quantity.canonical_value().clone(),
        quantity.dimension(),
        DerivedQuantityRule::AuthoredQuantity,
        vec![derived_input("authored", quantity)],
        BTreeSet::new(),
        BTreeSet::new(),
    )
}

fn derived_quantity(
    canonical_value: ExactScalar,
    dimension: Dimension,
    rule: DerivedQuantityRule,
    inputs: Vec<DerivedInput>,
    premises: BTreeSet<FactId>,
    assumptions: BTreeSet<AssumptionPremiseId>,
) -> DerivedQuantity {
    DerivedQuantity::new(
        canonical_value,
        dimension,
        QuantityDerivation {
            rule,
            inputs,
            premises,
            assumptions,
        },
    )
}
