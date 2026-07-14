use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use chem_domain::{
    PremiseId, ReactionRuleId, RepresentationKind, StructureDefinition, StructureId,
};

use super::{
    CatalogueError, CatalogueErrorCode, GeneralizedArgumentRecord, GeneralizedCasePredicateRecord,
    GeneralizedParameterRecord, GeneralizedReactionCaseRecord, GeneralizedReactionRuleRecord,
    GeneralizedStructureSelectorRecord, GraphPatternId, GraphPatternRecord, MappingPairRecord,
    ObservationPredicate, OperationTemplateRecord, PatternElementRecord, RepresentationRecord,
    RequestRelation, RuleSideRecord, StructuralTraitAssertionRecord, StructuralTraitId,
    StructureTemplateApplicationRecord, StructureTemplateId, StructureTemplateRecord,
    ValidatedCatalogueBundle, duplicate_id, require_premise, validate_label,
};

const MAX_GENERALIZED_PARAMETER_BINDINGS: usize = 4_096;
const MAX_GENERALIZED_PARAMETERS: usize = 64;
const MAX_GENERALIZED_ROLE_COEFFICIENT: u32 = 8;
const MAX_GENERALIZED_TOTAL_INSTANCES: u32 = 32;

#[derive(Debug, Clone)]
pub struct ValidatedGeneralizedRule {
    record_index: usize,
    parameter_domains: BTreeMap<String, BTreeSet<String>>,
}

impl ValidatedGeneralizedRule {
    #[must_use]
    pub const fn parameter_domains(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.parameter_domains
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralizedCaseSelection<'a> {
    Supported(&'a GeneralizedReactionCaseRecord),
    Unsupported(&'a GeneralizedReactionCaseRecord),
}

impl ValidatedCatalogueBundle {
    #[must_use]
    pub fn generalized_rule(&self, id: &ReactionRuleId) -> Option<&GeneralizedReactionRuleRecord> {
        self.generalized_rules
            .get(id)
            .map(|validated| &self.document.generalized_rules[validated.record_index])
    }

    #[must_use]
    pub fn generalized_parameter_domains(
        &self,
        id: &ReactionRuleId,
    ) -> Option<&BTreeMap<String, BTreeSet<String>>> {
        self.generalized_rules
            .get(id)
            .map(|rule| &rule.parameter_domains)
    }

    /// Selects the unique statically validated case for an exact parameter binding.
    ///
    /// An uncovered in-domain binding returns `Ok(None)` and remains unsupported.
    /// This method does not match graphs, resolve source, or instantiate rewrites.
    ///
    /// # Errors
    ///
    /// Returns an invalid-generalized-rule error for a missing, extra, or
    /// out-of-domain parameter binding.
    pub fn select_generalized_case<'a>(
        &'a self,
        id: &ReactionRuleId,
        binding: &BTreeMap<String, String>,
    ) -> Result<Option<GeneralizedCaseSelection<'a>>, CatalogueError> {
        let validated = self.generalized_rules.get(id).ok_or_else(|| {
            CatalogueError::new(
                CatalogueErrorCode::UnknownReference,
                format!("generalized rule `{id}` does not resolve"),
            )
        })?;
        if binding.keys().collect::<BTreeSet<_>>()
            != validated.parameter_domains.keys().collect::<BTreeSet<_>>()
            || binding
                .iter()
                .any(|(parameter, value)| !validated.parameter_domains[parameter].contains(value))
        {
            return generalized_error(format!(
                "generalized rule `{id}` received an invalid parameter binding"
            ));
        }
        let record = &self.document.generalized_rules[validated.record_index];
        let selected = record
            .cases
            .iter()
            .find(|case| predicate_matches(case.when(), binding));
        Ok(selected.map(|case| match case {
            GeneralizedReactionCaseRecord::Supported { .. } => {
                GeneralizedCaseSelection::Supported(case)
            }
            GeneralizedReactionCaseRecord::Unsupported { .. } => {
                GeneralizedCaseSelection::Unsupported(case)
            }
        }))
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_generalized_rules(
    records: &[GeneralizedReactionRuleRecord],
    categories: &BTreeMap<super::ElementCategoryId, BTreeSet<chem_domain::ElementSymbol>>,
    membership_provenance: &BTreeMap<
        (chem_domain::ElementSymbol, super::ElementCategoryId),
        super::ElementMembershipProvenance,
    >,
    structures: &BTreeMap<StructureId, StructureDefinition>,
    structure_premises: &BTreeMap<StructureId, BTreeSet<PremiseId>>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    trait_records: &[super::StructuralTraitDefinitionRecord],
    trait_index: &BTreeMap<StructuralTraitId, usize>,
    templates: &[StructureTemplateRecord],
    template_index: &BTreeMap<StructureTemplateId, usize>,
    applications: &[StructureTemplateApplicationRecord],
    pattern_records: &[GraphPatternRecord],
    pattern_index: &BTreeMap<GraphPatternId, usize>,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<ReactionRuleId, ValidatedGeneralizedRule>, CatalogueError> {
    let mut result = BTreeMap::new();
    for (record_index, record) in records.iter().enumerate() {
        if result.contains_key(&record.id) {
            return duplicate_id(&record.id);
        }
        validate_generalized_rule_shape(record, premises)?;
        let domains = parameter_domains(record, categories, structure_traits, trait_index)?;
        validate_parameter_provenance(
            record,
            categories,
            membership_provenance,
            structure_premises,
            structure_traits,
            trait_records,
            trait_index,
        )?;
        validate_roles_and_selectors(
            record,
            &domains,
            structures,
            structure_premises,
            structure_traits,
            templates,
            template_index,
            applications,
        )?;
        validate_cases(
            record,
            &domains,
            structures,
            structure_premises,
            structure_traits,
            trait_records,
            templates,
            template_index,
            applications,
            pattern_records,
            pattern_index,
            premises,
        )?;
        result.insert(
            record.id.clone(),
            ValidatedGeneralizedRule {
                record_index,
                parameter_domains: domains,
            },
        );
    }
    Ok(result)
}

fn validate_generalized_rule_shape(
    record: &GeneralizedReactionRuleRecord,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<(), CatalogueError> {
    if record.parameters.is_empty()
        || record.roles.is_empty()
        || record.cases.is_empty()
        || record.premise_ids.is_empty()
        || record.model_assumptions.premise_ids.is_empty()
        || record.applicability.required_context.trim().is_empty()
        || record.applicability.request_relation != RequestRelation::Contact
    {
        return generalized_error(format!(
            "generalized rule `{}` has an empty required section",
            record.id
        ));
    }
    for premise in &record.premise_ids {
        require_premise(premise, premises)?;
    }
    require_bound_premise(record, &record.applicability.premise_id, premises)?;
    for premise in &record.model_assumptions.premise_ids {
        require_bound_premise(record, premise, premises)?;
    }
    Ok(())
}

fn parameter_domains(
    record: &GeneralizedReactionRuleRecord,
    categories: &BTreeMap<super::ElementCategoryId, BTreeSet<chem_domain::ElementSymbol>>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    trait_index: &BTreeMap<StructuralTraitId, usize>,
) -> Result<BTreeMap<String, BTreeSet<String>>, CatalogueError> {
    if record.parameters.len() > MAX_GENERALIZED_PARAMETERS {
        return generalized_error(format!(
            "generalized rule `{}` exceeds the parameter limit of {MAX_GENERALIZED_PARAMETERS}",
            record.id
        ));
    }
    let mut domains = BTreeMap::new();
    for (name, parameter) in &record.parameters {
        validate_label(name, CatalogueErrorCode::InvalidGeneralizedRule)?;
        let values = match parameter {
            GeneralizedParameterRecord::Element { category } => categories
                .get(category)
                .ok_or_else(|| {
                    CatalogueError::new(
                        CatalogueErrorCode::UnknownReference,
                        format!("generalized parameter category `{category}` does not resolve"),
                    )
                })?
                .iter()
                .map(ToString::to_string)
                .collect(),
            GeneralizedParameterRecord::Structure { trait_id } => {
                if !trait_index.contains_key(trait_id) {
                    return Err(CatalogueError::new(
                        CatalogueErrorCode::UnknownReference,
                        format!("generalized parameter trait `{trait_id}` does not resolve"),
                    ));
                }
                structure_traits
                    .iter()
                    .filter(|(_, traits)| traits.contains_key(trait_id))
                    .map(|(structure, _)| structure.to_string())
                    .collect()
            }
            GeneralizedParameterRecord::Enum { values } => {
                if values.is_empty()
                    || values
                        .iter()
                        .any(|value| value.trim() != value || value.is_empty())
                {
                    return generalized_error(format!(
                        "generalized rule `{}` has an invalid enum domain",
                        record.id
                    ));
                }
                values.clone()
            }
        };
        if values.is_empty() {
            return generalized_error(format!(
                "generalized rule `{}` parameter `{name}` has an empty finite domain",
                record.id
            ));
        }
        domains.insert(name.clone(), values);
    }
    let binding_count = domains.values().try_fold(1_usize, |count, domain| {
        count
            .checked_mul(domain.len())
            .filter(|product| *product <= MAX_GENERALIZED_PARAMETER_BINDINGS)
    });
    if binding_count.is_none() {
        return generalized_error(format!(
            "generalized rule `{}` exceeds the finite binding limit of {MAX_GENERALIZED_PARAMETER_BINDINGS}",
            record.id
        ));
    }
    Ok(domains)
}

#[allow(clippy::too_many_arguments)]
fn validate_parameter_provenance(
    record: &GeneralizedReactionRuleRecord,
    categories: &BTreeMap<super::ElementCategoryId, BTreeSet<chem_domain::ElementSymbol>>,
    membership_provenance: &BTreeMap<
        (chem_domain::ElementSymbol, super::ElementCategoryId),
        super::ElementMembershipProvenance,
    >,
    structure_premises: &BTreeMap<StructureId, BTreeSet<PremiseId>>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    trait_records: &[super::StructuralTraitDefinitionRecord],
    trait_index: &BTreeMap<StructuralTraitId, usize>,
) -> Result<(), CatalogueError> {
    for parameter in record.parameters.values() {
        match parameter {
            GeneralizedParameterRecord::Element { category } => {
                for element in &categories[category] {
                    let provenance = &membership_provenance[&(element.clone(), category.clone())];
                    require_bound_premises(record, &provenance.element_premise_ids)?;
                    require_bound_premises(record, &provenance.category_premise_ids)?;
                }
            }
            GeneralizedParameterRecord::Structure { trait_id } => {
                require_bound_premises(record, &trait_records[trait_index[trait_id]].premise_ids)?;
                for (structure, traits) in structure_traits {
                    if let Some(assertion) = traits.get(trait_id) {
                        require_bound_premises(record, &structure_premises[structure])?;
                        require_bound_premises(record, &assertion.premise_ids)?;
                    }
                }
            }
            GeneralizedParameterRecord::Enum { .. } => {}
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_roles_and_selectors(
    record: &GeneralizedReactionRuleRecord,
    domains: &BTreeMap<String, BTreeSet<String>>,
    structures: &BTreeMap<StructureId, StructureDefinition>,
    structure_premises: &BTreeMap<StructureId, BTreeSet<PremiseId>>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    templates: &[StructureTemplateRecord],
    template_index: &BTreeMap<StructureTemplateId, usize>,
    applications: &[StructureTemplateApplicationRecord],
) -> Result<(), CatalogueError> {
    let reactant_roles = roles_on_side(record, RuleSideRecord::Reactant);
    let product_roles = roles_on_side(record, RuleSideRecord::Product);
    if reactant_roles.is_empty()
        || product_roles.is_empty()
        || record.reactants.keys().collect::<BTreeSet<_>>()
            != reactant_roles.iter().collect::<BTreeSet<_>>()
    {
        return generalized_error(format!(
            "generalized rule `{}` role and reactant schemas disagree",
            record.id
        ));
    }
    for (role, schema) in &record.roles {
        validate_label(role, CatalogueErrorCode::InvalidGeneralizedRule)?;
        if schema.coefficient == 0 || schema.coefficient > MAX_GENERALIZED_ROLE_COEFFICIENT {
            return generalized_error(format!(
                "generalized rule `{}` role `{role}` has an unsupported coefficient",
                record.id
            ));
        }
    }
    if record
        .roles
        .values()
        .map(|schema| schema.coefficient)
        .try_fold(0_u32, u32::checked_add)
        .is_none_or(|total| total > MAX_GENERALIZED_TOTAL_INSTANCES)
    {
        return generalized_error(format!(
            "generalized rule `{}` exceeds the total instance limit",
            record.id
        ));
    }
    for selector in record.reactants.values() {
        validate_selector_shape(
            selector,
            false,
            &record.parameters,
            structures,
            structure_traits,
            templates,
            template_index,
            applications,
        )?;
    }
    for binding in enumerate_bindings(domains) {
        for (role, selector) in &record.reactants {
            let resolved = resolve_selector(
                selector,
                &binding,
                structures,
                structure_traits,
                applications,
            )?;
            if resolved.is_empty()
                || resolved.iter().any(|id| {
                    structures[id].representation()
                        != representation_kind(record.roles[role].representation)
                })
            {
                return generalized_error(format!(
                    "generalized rule `{}` reactant role `{role}` cannot resolve its finite domain",
                    record.id
                ));
            }
            for id in &resolved {
                require_bound_premises(record, &structure_premises[id])?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn validate_cases(
    record: &GeneralizedReactionRuleRecord,
    domains: &BTreeMap<String, BTreeSet<String>>,
    structures: &BTreeMap<StructureId, StructureDefinition>,
    structure_premises: &BTreeMap<StructureId, BTreeSet<PremiseId>>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    trait_records: &[super::StructuralTraitDefinitionRecord],
    templates: &[StructureTemplateRecord],
    template_index: &BTreeMap<StructureTemplateId, usize>,
    applications: &[StructureTemplateApplicationRecord],
    pattern_records: &[GraphPatternRecord],
    pattern_index: &BTreeMap<GraphPatternId, usize>,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<(), CatalogueError> {
    let mut ids = BTreeSet::new();
    let mut reachable = BTreeMap::<String, usize>::new();
    for case in &record.cases {
        validate_label(case.id(), CatalogueErrorCode::InvalidGeneralizedCase)?;
        if !ids.insert(case.id().clone()) || case.premise_ids().is_empty() {
            return generalized_case_error(record, case.id(), "duplicate ID or empty premises");
        }
        for premise in case.premise_ids() {
            require_bound_premise(record, premise, premises)?;
        }
        validate_predicate(case.when(), domains)?;
        reachable.insert(case.id().clone(), 0);
    }

    let bindings = enumerate_bindings(domains);
    for binding in &bindings {
        let selected = record
            .cases
            .iter()
            .filter(|case| predicate_matches(case.when(), binding))
            .collect::<Vec<_>>();
        if selected.len() > 1 {
            return generalized_case_error(record, selected[0].id(), "overlaps another case");
        }
        if let Some(case) = selected.first() {
            reachable
                .entry(case.id().clone())
                .and_modify(|count| *count += 1);
        }
    }
    if let Some((id, _)) = reachable.iter().find(|(_, count)| **count == 0) {
        return generalized_case_error(record, id, "is unreachable over the finite domain");
    }

    let reactant_roles = roles_on_side(record, RuleSideRecord::Reactant);
    let product_roles = roles_on_side(record, RuleSideRecord::Product);
    for case in &record.cases {
        match case {
            GeneralizedReactionCaseRecord::Unsupported {
                id,
                required_feature,
                explanation,
                ..
            } => {
                if validate_label(required_feature, CatalogueErrorCode::InvalidGeneralizedCase)
                    .is_err()
                    || explanation.trim().is_empty()
                {
                    return generalized_case_error(record, id, "has an empty domain-gap reason");
                }
            }
            GeneralizedReactionCaseRecord::Supported {
                id,
                products,
                patterns,
                correspondence,
                rewrite,
                observation_compatibility,
                ..
            } => {
                if products.keys().collect::<BTreeSet<_>>()
                    != product_roles.iter().collect::<BTreeSet<_>>()
                    || patterns.keys().collect::<BTreeSet<_>>()
                        != reactant_roles.iter().collect::<BTreeSet<_>>()
                    || correspondence.is_empty()
                    || rewrite.is_empty()
                {
                    return generalized_case_error(record, id, "has incomplete supported payload");
                }
                for selector in products.values() {
                    validate_selector_shape(
                        selector,
                        true,
                        &record.parameters,
                        structures,
                        structure_traits,
                        templates,
                        template_index,
                        applications,
                    )?;
                }
                for pattern in patterns.values() {
                    if !pattern_index.contains_key(pattern) {
                        return Err(CatalogueError::new(
                            CatalogueErrorCode::UnknownReference,
                            format!("generalized case pattern `{pattern}` does not resolve"),
                        ));
                    }
                    require_bound_premises(
                        record,
                        &pattern_records[pattern_index[pattern]].premise_ids,
                    )?;
                    validate_pattern_parameters(
                        record,
                        id,
                        &pattern_records[pattern_index[pattern]],
                    )?;
                }
                for item in correspondence {
                    if item.premise_ids.is_empty() {
                        return generalized_case_error(
                            record,
                            id,
                            "correspondence has no premises",
                        );
                    }
                    for premise in &item.premise_ids {
                        require_bound_premise(record, premise, premises)?;
                    }
                }
                for operation in rewrite {
                    if operation.premise_ids().is_empty() {
                        return generalized_case_error(record, id, "rewrite has no premises");
                    }
                    for premise in operation.premise_ids() {
                        require_bound_premise(record, premise, premises)?;
                    }
                }
                for observation in observation_compatibility {
                    let Some(role) = record.roles.get(&observation.subject_role) else {
                        return generalized_case_error(
                            record,
                            id,
                            "observation role does not resolve",
                        );
                    };
                    require_bound_premise(record, &observation.premise_id, premises)?;
                    let shape_matches = match observation.predicate {
                        ObservationPredicate::Evolves => {
                            role.side == RuleSideRecord::Product
                                && role.representation == RepresentationRecord::Molecular
                                && observation.value.is_none()
                        }
                        ObservationPredicate::Disappears => {
                            role.side == RuleSideRecord::Reactant && observation.value.is_none()
                        }
                        ObservationPredicate::Forms => {
                            role.side == RuleSideRecord::Product && observation.value.is_none()
                        }
                        ObservationPredicate::Colour => {
                            role.side == RuleSideRecord::Product
                                && observation
                                    .value
                                    .as_deref()
                                    .is_some_and(|value| !value.trim().is_empty())
                        }
                    };
                    if !shape_matches || observation.evidence_subject.trim().is_empty() {
                        return generalized_case_error(
                            record,
                            id,
                            "invalid observation compatibility fact",
                        );
                    }
                }
                let unique_observations = observation_compatibility
                    .iter()
                    .map(|observation| {
                        (
                            observation.subject_role.as_str(),
                            observation.predicate,
                            observation.evidence_subject.as_str(),
                            observation.value.as_deref(),
                        )
                    })
                    .collect::<BTreeSet<_>>();
                if unique_observations.len() != observation_compatibility.len() {
                    return generalized_case_error(
                        record,
                        id,
                        "duplicate observation compatibility fact",
                    );
                }
                for binding in bindings
                    .iter()
                    .filter(|binding| predicate_matches(case.when(), binding))
                {
                    validate_supported_binding(
                        record,
                        id,
                        products,
                        patterns,
                        correspondence,
                        rewrite,
                        binding,
                        structures,
                        structure_premises,
                        structure_traits,
                        trait_records,
                        applications,
                        pattern_records,
                        pattern_index,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn validate_pattern_parameters(
    record: &GeneralizedReactionRuleRecord,
    case_id: &str,
    pattern: &GraphPatternRecord,
) -> Result<(), CatalogueError> {
    for variable in pattern.variables.values() {
        if let Some(PatternElementRecord::Parameter(reference)) = &variable.atom.element
            && !matches!(
                record.parameters.get(&reference.parameter),
                Some(GeneralizedParameterRecord::Element { .. })
            )
        {
            return generalized_case_error(
                record,
                case_id,
                format!(
                    "pattern `{}` element parameter `{}` is absent or has the wrong kind",
                    pattern.id, reference.parameter
                ),
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_selector_shape(
    selector: &GeneralizedStructureSelectorRecord,
    product: bool,
    parameters: &BTreeMap<String, GeneralizedParameterRecord>,
    structures: &BTreeMap<StructureId, StructureDefinition>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    templates: &[StructureTemplateRecord],
    template_index: &BTreeMap<StructureTemplateId, usize>,
    applications: &[StructureTemplateApplicationRecord],
) -> Result<(), CatalogueError> {
    match selector {
        GeneralizedStructureSelectorRecord::Exact { structure } => {
            if !structures.contains_key(structure) {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!("generalized selector structure `{structure}` does not resolve"),
                ));
            }
        }
        GeneralizedStructureSelectorRecord::Template {
            template,
            arguments,
        } => {
            let Some(index) = template_index.get(template) else {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!("generalized selector template `{template}` does not resolve"),
                ));
            };
            let template_record = &templates[*index];
            if arguments.keys().collect::<BTreeSet<_>>()
                != template_record.parameters().keys().collect::<BTreeSet<_>>()
            {
                return generalized_error(format!(
                    "template selector `{template}` has the wrong argument set"
                ));
            }
            for argument in arguments.values() {
                if let GeneralizedArgumentRecord::Parameter(reference) = argument
                    && !parameters.contains_key(&reference.parameter)
                {
                    return generalized_error(format!(
                        "template selector `{template}` references unknown parameter `{}`",
                        reference.parameter
                    ));
                }
            }
            for (name, template_parameter) in template_record.parameters() {
                if let GeneralizedArgumentRecord::Parameter(reference) = &arguments[name] {
                    let compatible = matches!(
                        (template_parameter, &parameters[&reference.parameter]),
                        (
                            super::StructureTemplateParameterRecord::Element { .. },
                            GeneralizedParameterRecord::Element { .. }
                        ) | (
                            super::StructureTemplateParameterRecord::Structure { .. },
                            GeneralizedParameterRecord::Structure { .. }
                        ) | (
                            super::StructureTemplateParameterRecord::Enum { .. },
                            GeneralizedParameterRecord::Enum { .. }
                        )
                    );
                    if !compatible {
                        return generalized_error(format!(
                            "template selector `{template}` argument `{name}` has an incompatible parameter kind"
                        ));
                    }
                }
            }
            if !applications
                .iter()
                .any(|application| application.template == *template)
            {
                return generalized_error(format!(
                    "template selector `{template}` has no stable applications"
                ));
            }
        }
        GeneralizedStructureSelectorRecord::Trait { trait_id } => {
            if product
                || !structure_traits
                    .values()
                    .any(|traits| traits.contains_key(trait_id))
            {
                return generalized_error(format!(
                    "trait selector `{trait_id}` is not valid in this position"
                ));
            }
        }
        GeneralizedStructureSelectorRecord::StructureParameter { parameter } => {
            if product
                || !matches!(
                    parameters.get(parameter),
                    Some(GeneralizedParameterRecord::Structure { .. })
                )
            {
                return generalized_error(format!(
                    "structure parameter selector `{parameter}` is not valid in this position"
                ));
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn validate_supported_binding(
    record: &GeneralizedReactionRuleRecord,
    case_id: &str,
    products: &BTreeMap<String, GeneralizedStructureSelectorRecord>,
    patterns: &BTreeMap<String, GraphPatternId>,
    correspondence: &[MappingPairRecord],
    rewrite: &[OperationTemplateRecord],
    binding: &BTreeMap<String, String>,
    structures: &BTreeMap<StructureId, StructureDefinition>,
    structure_premises: &BTreeMap<StructureId, BTreeSet<PremiseId>>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    trait_records: &[super::StructuralTraitDefinitionRecord],
    applications: &[StructureTemplateApplicationRecord],
    pattern_records: &[GraphPatternRecord],
    pattern_index: &BTreeMap<GraphPatternId, usize>,
) -> Result<(), CatalogueError> {
    let mut resolved_reactants = BTreeMap::new();
    for (role, selector) in &record.reactants {
        let resolved = resolve_selector(
            selector,
            binding,
            structures,
            structure_traits,
            applications,
        )?;
        if resolved.len() != 1 {
            return generalized_case_error(record, case_id, "reactant identity is ambiguous");
        }
        let id = resolved.into_iter().next().unwrap();
        require_bound_premises(record, &structure_premises[&id])?;
        resolved_reactants.insert(role.clone(), id);
    }
    let mut resolved_products = BTreeMap::new();
    for (role, selector) in products {
        let resolved = resolve_selector(
            selector,
            binding,
            structures,
            structure_traits,
            applications,
        )?;
        if resolved.len() != 1 {
            return generalized_case_error(record, case_id, "product identity is ambiguous");
        }
        let id = resolved.into_iter().next().unwrap();
        require_bound_premises(record, &structure_premises[&id])?;
        if structures[&id].representation()
            != representation_kind(record.roles[role].representation)
        {
            return generalized_case_error(record, case_id, "product representation disagrees");
        }
        resolved_products.insert(role.clone(), id);
    }

    let mut source_atoms = BTreeSet::new();
    let mut source_bindings = BTreeMap::<String, super::StructuralTraitSiteKindRecord>::new();
    for (role, pattern_id) in patterns {
        let pattern = &pattern_records[pattern_index[pattern_id]];
        let structure = &structures[&resolved_reactants[role]];
        for required_trait in &pattern.traits {
            let assertion = structure_traits
                .get(&resolved_reactants[role])
                .and_then(|traits| traits.get(&required_trait.trait_id))
                .ok_or_else(|| {
                    CatalogueError::new(
                        CatalogueErrorCode::InvalidGeneralizedCase,
                        format!(
                            "generalized rule `{}` case `{case_id}` requires an absent checked trait",
                            record.id
                        ),
                    )
                })?;
            require_bound_premises(record, &assertion.premise_ids)?;
            let definition = trait_records
                .iter()
                .find(|definition| definition.id == required_trait.trait_id)
                .expect("validated graph pattern trait resolves");
            require_bound_premises(record, &definition.premise_ids)?;
        }
        if pattern.variables.len() != structure.graph().atoms().len() {
            return generalized_case_error(
                record,
                case_id,
                "reactant pattern does not expose a total atom shape",
            );
        }
        let coefficient = record.roles[role].coefficient;
        for instance in 1..=coefficient {
            for variable in pattern.variables.keys() {
                let reference = format!("{role}[{instance}].{variable}");
                source_atoms.insert(reference.clone());
                source_bindings.insert(reference, super::StructuralTraitSiteKindRecord::Atom);
            }
            for relationship in &pattern.relationships {
                source_bindings.insert(
                    format!("{role}[{instance}].{}", relationship.binding_name()),
                    relationship.binding_kind(),
                );
            }
        }
    }

    let mut product_atoms = BTreeSet::new();
    let mut product_instances = BTreeSet::new();
    for (role, structure_id) in &resolved_products {
        for instance in 1..=record.roles[role].coefficient {
            product_instances.insert(format!("{role}[{instance}]"));
            for atom in structures[structure_id].graph().atoms().keys() {
                product_atoms.insert(format!("{role}[{instance}].{atom}"));
            }
        }
    }
    let mapped_sources = correspondence
        .iter()
        .map(|item| item.reactant.clone())
        .collect::<BTreeSet<_>>();
    let mapped_products = correspondence
        .iter()
        .map(|item| item.product.clone())
        .collect::<BTreeSet<_>>();
    if mapped_sources.len() != correspondence.len()
        || mapped_products.len() != correspondence.len()
        || mapped_sources != source_atoms
        || mapped_products != product_atoms
    {
        return generalized_case_error(record, case_id, "atom correspondence is not total");
    }
    let mut expected_product_assignments = BTreeMap::<String, BTreeSet<String>>::new();
    for item in correspondence {
        let product = item
            .product
            .split_once('.')
            .map(|(instance, _)| instance)
            .ok_or_else(|| {
                CatalogueError::new(
                    CatalogueErrorCode::InvalidGeneralizedCase,
                    "generalized product atom reference has no instance",
                )
            })?;
        expected_product_assignments
            .entry(product.to_owned())
            .or_default()
            .insert(item.reactant.clone());
    }
    validate_rewrite_references(
        record,
        case_id,
        rewrite,
        &source_atoms,
        &source_bindings,
        &product_instances,
        &expected_product_assignments,
    )
}

#[allow(clippy::too_many_lines)]
fn validate_rewrite_references(
    record: &GeneralizedReactionRuleRecord,
    case_id: &str,
    rewrite: &[OperationTemplateRecord],
    source_atoms: &BTreeSet<String>,
    source_bindings: &BTreeMap<String, super::StructuralTraitSiteKindRecord>,
    product_instances: &BTreeSet<String>,
    expected_product_assignments: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), CatalogueError> {
    let atom = |reference: &str| source_atoms.contains(reference);
    let kind = |reference: &str, expected| source_bindings.get(reference) == Some(&expected);
    let mut assigned_atoms = BTreeSet::new();
    let mut assigned_products = BTreeSet::new();
    let mut assigned_atom_count = 0;
    let mut assigned_product_count = 0;
    for operation in rewrite {
        let valid = match operation {
            OperationTemplateRecord::CleaveCovalent {
                edge, allocation, ..
            } => {
                edge.0 != edge.1
                    && atom(&edge.0)
                    && atom(&edge.1)
                    && cleavage_allocation_valid(allocation, &edge.0, &edge.1)
            }
            OperationTemplateRecord::FormCovalent { edge, .. } => {
                edge.0 != edge.1 && atom(&edge.0) && atom(&edge.1)
            }
            OperationTemplateRecord::CleaveDative {
                donor,
                acceptor,
                allocation,
                ..
            } => {
                donor != acceptor
                    && atom(donor)
                    && atom(acceptor)
                    && cleavage_allocation_valid(allocation, donor, acceptor)
            }
            OperationTemplateRecord::FormDative {
                donor, acceptor, ..
            }
            | OperationTemplateRecord::TransferElectron {
                donor, acceptor, ..
            } => donor != acceptor && atom(donor) && atom(acceptor),
            OperationTemplateRecord::ChangeCovalent {
                edge, allocation, ..
            } => {
                edge.0 != edge.1
                    && atom(&edge.0)
                    && atom(&edge.1)
                    && cleavage_allocation_valid(allocation, &edge.0, &edge.1)
            }
            OperationTemplateRecord::AssociateIonic {
                label,
                components,
                component_charges,
                ..
            } => {
                let component_atoms = components.iter().flatten().collect::<BTreeSet<_>>();
                validate_label(label, CatalogueErrorCode::InvalidGeneralizedCase).is_ok()
                    && components.len() >= 2
                    && components.len() == component_charges.len()
                    && component_charges
                        .iter()
                        .map(|charge| i64::from(*charge))
                        .sum::<i64>()
                        == 0
                    && component_atoms.len() == components.iter().map(Vec::len).sum::<usize>()
                    && components.iter().all(|component| {
                        !component.is_empty() && component.iter().all(|site| atom(site))
                    })
            }
            OperationTemplateRecord::DissociateIonic { association, .. } => kind(
                association,
                super::StructuralTraitSiteKindRecord::IonicAssociation,
            ),
            OperationTemplateRecord::ReleaseMetallic { site, domain, .. }
            | OperationTemplateRecord::JoinMetallic { site, domain, .. } => {
                atom(site) && kind(domain, super::StructuralTraitSiteKindRecord::MetallicDomain)
            }
            OperationTemplateRecord::AssignProduct { atoms, product, .. } => {
                assigned_atoms.extend(atoms.iter().cloned());
                assigned_products.insert(product.clone());
                assigned_atom_count += atoms.len();
                assigned_product_count += 1;
                !atoms.is_empty()
                    && atoms.iter().collect::<BTreeSet<_>>().len() == atoms.len()
                    && atoms.iter().all(|site| atom(site))
                    && product_instances.contains(product)
                    && expected_product_assignments.get(product)
                        == Some(&atoms.iter().cloned().collect())
            }
        };
        if !valid {
            return generalized_case_error(
                record,
                case_id,
                format!("rewrite endpoint does not resolve for {operation:?}"),
            );
        }
    }
    if assigned_atoms != *source_atoms
        || assigned_products != *product_instances
        || assigned_atom_count != source_atoms.len()
        || assigned_product_count != product_instances.len()
    {
        return generalized_case_error(record, case_id, "product assignment is not total");
    }
    Ok(())
}

fn cleavage_allocation_valid(
    allocation: &super::CleavageAllocationRecord,
    left: &str,
    right: &str,
) -> bool {
    match allocation {
        super::CleavageAllocationRecord::Homolytic(value) => value == "homolytic",
        super::CleavageAllocationRecord::Heterolytic { heterolytic_to } => {
            heterolytic_to == left || heterolytic_to == right
        }
    }
}

fn validate_predicate(
    predicate: &GeneralizedCasePredicateRecord,
    domains: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), CatalogueError> {
    match predicate {
        GeneralizedCasePredicateRecord::Always => Ok(()),
        GeneralizedCasePredicateRecord::All { predicates }
        | GeneralizedCasePredicateRecord::Any { predicates } => {
            if predicates.is_empty()
                || predicates
                    .iter()
                    .map(|child| serde_json::to_string(child).expect("predicate serializes"))
                    .collect::<BTreeSet<_>>()
                    .len()
                    != predicates.len()
            {
                return generalized_case_predicate_error(
                    "generalized case predicate has empty or duplicate children",
                );
            }
            for child in predicates {
                validate_predicate(child, domains)?;
            }
            Ok(())
        }
        GeneralizedCasePredicateRecord::Not { predicate } => validate_predicate(predicate, domains),
        GeneralizedCasePredicateRecord::ParameterEquals { parameter, value } => {
            validate_predicate_value(parameter, value, domains)
        }
        GeneralizedCasePredicateRecord::ParameterInSet { parameter, values } => {
            if values.is_empty() {
                return generalized_case_predicate_error("generalized case set predicate is empty");
            }
            for value in values {
                validate_predicate_value(parameter, value, domains)?;
            }
            Ok(())
        }
    }
}

fn validate_predicate_value(
    parameter: &str,
    value: &str,
    domains: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), CatalogueError> {
    if !domains
        .get(parameter)
        .is_some_and(|domain| domain.contains(value))
    {
        return generalized_case_predicate_error(format!(
            "generalized case predicate value `{parameter}={value}` is outside its domain"
        ));
    }
    Ok(())
}

fn predicate_matches(
    predicate: &GeneralizedCasePredicateRecord,
    binding: &BTreeMap<String, String>,
) -> bool {
    match predicate {
        GeneralizedCasePredicateRecord::Always => true,
        GeneralizedCasePredicateRecord::All { predicates } => predicates
            .iter()
            .all(|child| predicate_matches(child, binding)),
        GeneralizedCasePredicateRecord::Any { predicates } => predicates
            .iter()
            .any(|child| predicate_matches(child, binding)),
        GeneralizedCasePredicateRecord::Not { predicate } => !predicate_matches(predicate, binding),
        GeneralizedCasePredicateRecord::ParameterEquals { parameter, value } => {
            binding.get(parameter) == Some(value)
        }
        GeneralizedCasePredicateRecord::ParameterInSet { parameter, values } => binding
            .get(parameter)
            .is_some_and(|value| values.contains(value)),
    }
}

fn enumerate_bindings(
    domains: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<BTreeMap<String, String>> {
    let mut bindings = vec![BTreeMap::new()];
    for (parameter, values) in domains {
        let mut next = Vec::new();
        for binding in &bindings {
            for value in values {
                let mut extended = binding.clone();
                extended.insert(parameter.clone(), value.clone());
                next.push(extended);
            }
        }
        bindings = next;
    }
    bindings
}

pub(super) fn resolve_selector(
    selector: &GeneralizedStructureSelectorRecord,
    binding: &BTreeMap<String, String>,
    structures: &BTreeMap<StructureId, StructureDefinition>,
    structure_traits: &BTreeMap<
        StructureId,
        BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>,
    >,
    applications: &[StructureTemplateApplicationRecord],
) -> Result<BTreeSet<StructureId>, CatalogueError> {
    Ok(match selector {
        GeneralizedStructureSelectorRecord::Exact { structure } => {
            BTreeSet::from([structure.clone()])
        }
        GeneralizedStructureSelectorRecord::Template {
            template,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|(name, argument)| {
                    let value = match argument {
                        GeneralizedArgumentRecord::Literal(value) => value.clone(),
                        GeneralizedArgumentRecord::Parameter(reference) => {
                            binding.get(&reference.parameter).cloned().ok_or_else(|| {
                                CatalogueError::new(
                                    CatalogueErrorCode::InvalidGeneralizedRule,
                                    format!(
                                        "selector parameter `{}` has no finite binding",
                                        reference.parameter
                                    ),
                                )
                            })?
                        }
                    };
                    Ok((name.clone(), value))
                })
                .collect::<Result<BTreeMap<_, _>, CatalogueError>>()?;
            applications
                .iter()
                .filter(|application| {
                    application.template == *template && application.arguments == arguments
                })
                .map(|application| application.id.clone())
                .collect()
        }
        GeneralizedStructureSelectorRecord::Trait { trait_id } => structure_traits
            .iter()
            .filter(|(_, traits)| traits.contains_key(trait_id))
            .map(|(structure, _)| structure.clone())
            .collect(),
        GeneralizedStructureSelectorRecord::StructureParameter { parameter } => {
            let id = binding.get(parameter).ok_or_else(|| {
                CatalogueError::new(
                    CatalogueErrorCode::InvalidGeneralizedRule,
                    format!("structure parameter `{parameter}` has no finite binding"),
                )
            })?;
            let id = StructureId::from_str(id.as_str()).map_err(|error| {
                CatalogueError::new(
                    CatalogueErrorCode::InvalidGeneralizedRule,
                    error.to_string(),
                )
            })?;
            if !structures.contains_key(&id) {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!("structure parameter `{id}` does not resolve"),
                ));
            }
            BTreeSet::from([id])
        }
    })
}

fn roles_on_side(record: &GeneralizedReactionRuleRecord, side: RuleSideRecord) -> BTreeSet<String> {
    record
        .roles
        .iter()
        .filter(|(_, schema)| schema.side == side)
        .map(|(role, _)| role.clone())
        .collect()
}

const fn representation_kind(value: RepresentationRecord) -> RepresentationKind {
    match value {
        RepresentationRecord::Molecular => RepresentationKind::Molecular,
        RepresentationRecord::Ion => RepresentationKind::Ion,
        RepresentationRecord::Ionic => RepresentationKind::Ionic,
        RepresentationRecord::Metallic => RepresentationKind::Metallic,
    }
}

fn require_bound_premise(
    record: &GeneralizedReactionRuleRecord,
    premise: &PremiseId,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<(), CatalogueError> {
    require_premise(premise, premises)?;
    if !record.premise_ids.contains(premise) {
        return generalized_error(format!(
            "generalized rule `{}` does not proof-bind premise `{premise}`",
            record.id
        ));
    }
    Ok(())
}

fn require_bound_premises(
    record: &GeneralizedReactionRuleRecord,
    premises: &BTreeSet<PremiseId>,
) -> Result<(), CatalogueError> {
    if let Some(missing) = premises
        .iter()
        .find(|premise| !record.premise_ids.contains(*premise))
    {
        return generalized_error(format!(
            "generalized rule `{}` does not proof-bind premise `{missing}`",
            record.id
        ));
    }
    Ok(())
}

fn generalized_case_error<T>(
    record: &GeneralizedReactionRuleRecord,
    case: &str,
    message: impl std::fmt::Display,
) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidGeneralizedCase,
        format!("generalized rule `{}` case `{case}` {message}", record.id),
    ))
}

fn generalized_error<T>(message: impl Into<String>) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidGeneralizedRule,
        message,
    ))
}

fn generalized_case_predicate_error<T>(message: impl Into<String>) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidGeneralizedCase,
        message,
    ))
}
