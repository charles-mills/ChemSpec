use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use chem_domain::{ElementSymbol, PremiseId, StructureId, canonical_json};

use super::{
    ApplicabilityRecord, CatalogueError, CleavageAllocationRecord, GeneralizedArgumentRecord,
    GeneralizedCaseSelection, GeneralizedParameterRecord, GeneralizedReactionCaseRecord,
    GeneralizedReactionRuleRecord, GeneralizedStructureSelectorRecord, MappingPairRecord,
    OperationTemplateRecord, PatternRoleInput, PatternTermRecord, ReactionRuleId,
    ReactionRuleRecord, RolePatternMatchBinding, RoleSchemaRecord, RuleSideRecord,
    StructureAutomorphism, ValidatedCatalogueBundle,
};

// Seven-coordinate reviewed structures can have 7! = 5,040 complete
// automorphisms per product instance (10,080 for a balanced pair of IF7).
// This remains a fixed ceiling; it merely admits that finite symmetry class
// without weakening fail-closed enumeration.
const MAX_CERTIFICATE_CANDIDATES: usize = 16_384;

type InstancePatternMatches = BTreeMap<String, Vec<RolePatternMatchBinding>>;

struct EnumeratedInstanceMatches {
    representatives: Vec<InstancePatternMatches>,
    equivalent_match_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralizedRoleInput {
    pub role: String,
    pub structure: StructureId,
    pub coefficient: u32,
    pub side: RuleSideRecord,
    pub representation: super::RepresentationRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralizedElaborationFailureClass {
    InvalidSource,
    Unsupported,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralizedElaborationFailure {
    pub class: GeneralizedElaborationFailureClass,
    pub message: String,
    pub required_feature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElaboratedGeneralizedRule {
    pub rule: ReactionRuleRecord,
    pub parameter_binding: BTreeMap<String, String>,
    pub parameter_premise_ids: BTreeMap<String, BTreeSet<PremiseId>>,
    pub case_id: String,
    pub equivalent_match_count: usize,
    pub matched_sites: BTreeMap<String, BTreeMap<String, String>>,
    pub role_premise_ids: BTreeMap<String, BTreeSet<PremiseId>>,
    pub selected_premise_ids: BTreeSet<PremiseId>,
}

impl ValidatedCatalogueBundle {
    /// Deterministically compiles one validated generalized rule application
    /// into the existing concrete rule-record boundary without executing it.
    ///
    /// # Errors
    ///
    /// Returns a catalogue error only if already validated catalogue state is
    /// internally inconsistent. Request-level failures are returned as typed
    /// invalid, unsupported, or ambiguous outcomes.
    #[allow(clippy::too_many_lines)]
    pub fn elaborate_generalized_rule(
        &self,
        id: &ReactionRuleId,
        inputs: &[GeneralizedRoleInput],
    ) -> Result<Result<ElaboratedGeneralizedRule, GeneralizedElaborationFailure>, CatalogueError>
    {
        let Some(rule) = self.generalized_rule(id) else {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::Unsupported,
                format!("generalized rule `{id}` does not resolve"),
            )));
        };
        let by_role = inputs
            .iter()
            .map(|input| (input.role.clone(), input))
            .collect::<BTreeMap<_, _>>();
        if by_role.len() != inputs.len()
            || by_role.keys().collect::<BTreeSet<_>>() != rule.roles.keys().collect::<BTreeSet<_>>()
        {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::InvalidSource,
                "generalized rule role binding is incomplete or duplicated",
            )));
        }
        for (role, schema) in &rule.roles {
            if by_role[role].coefficient != schema.coefficient
                || by_role[role].side != schema.side
                || by_role[role].representation != schema.representation
            {
                return Ok(Err(failure(
                    GeneralizedElaborationFailureClass::InvalidSource,
                    format!("role `{role}` shape does not match generalized rule"),
                )));
            }
        }

        let domains = self.generalized_parameter_domains(id).ok_or_else(|| {
            CatalogueError::new(
                super::CatalogueErrorCode::InvalidGeneralizedRule,
                format!("generalized rule `{id}` lost its validated parameter domains"),
            )
        })?;
        let mut binding = BTreeMap::new();
        for (role, selector) in &rule.reactants {
            if let Err(result) = infer_selector_binding(
                self,
                selector,
                &by_role[role].structure,
                &mut binding,
                domains,
            ) {
                return Ok(Err(result));
            }
        }
        for (parameter, domain) in domains {
            if !binding.contains_key(parameter) {
                if domain.len() != 1 {
                    return Ok(Err(failure(
                        GeneralizedElaborationFailureClass::Ambiguous,
                        format!("parameter `{parameter}` is not uniquely induced by source roles"),
                    )));
                }
                let Some(value) = domain.first() else {
                    return Err(CatalogueError::new(
                        super::CatalogueErrorCode::InvalidGeneralizedRule,
                        format!("generalized parameter `{parameter}` lost its finite domain"),
                    ));
                };
                binding.insert(parameter.clone(), value.clone());
            }
        }

        let Some(selection) = self.select_generalized_case(id, &binding)? else {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::Unsupported,
                format!("rule `{id}` has no reviewed case for the inferred parameter binding"),
            )));
        };
        let case = match selection {
            GeneralizedCaseSelection::Unsupported(case) => {
                let GeneralizedReactionCaseRecord::Unsupported {
                    required_feature,
                    explanation,
                    ..
                } = case
                else {
                    return Err(CatalogueError::new(
                        super::CatalogueErrorCode::InvalidGeneralizedCase,
                        "generalized case selection has the wrong status",
                    ));
                };
                return Ok(Err(GeneralizedElaborationFailure {
                    class: GeneralizedElaborationFailureClass::Unsupported,
                    message: explanation.clone(),
                    required_feature: Some(required_feature.clone()),
                }));
            }
            GeneralizedCaseSelection::Supported(case) => case,
        };
        let GeneralizedReactionCaseRecord::Supported { products, .. } = case else {
            return Err(CatalogueError::new(
                super::CatalogueErrorCode::InvalidGeneralizedCase,
                "generalized case selection has the wrong status",
            ));
        };
        for (role, selector) in products {
            let resolved = super::generalized::resolve_selector(
                selector,
                &binding,
                self.structures(),
                &self.structure_traits,
                &self.document().structure_applications,
            )?;
            if resolved.len() != 1 || !resolved.contains(&by_role[role].structure) {
                return Ok(Err(failure(
                    GeneralizedElaborationFailureClass::InvalidSource,
                    format!(
                        "authored product role `{role}` does not match selected family product"
                    ),
                )));
            }
        }
        elaborate_supported(self, rule, case, &binding, &by_role)
    }

    /// Derives the reviewed product role inputs a generalized rule would
    /// produce for the given reactant bindings, without requiring the caller
    /// to know the product structures in advance. The parameter binding is
    /// inferred purely from the reactant selectors; the selected supported
    /// case then determines exactly one reviewed structure per product role.
    ///
    /// # Errors
    ///
    /// Returns a catalogue error only if already validated catalogue state is
    /// internally inconsistent. Request-level failures are returned as typed
    /// invalid, unsupported, or ambiguous outcomes.
    #[allow(clippy::too_many_lines)]
    pub fn derive_generalized_products(
        &self,
        id: &ReactionRuleId,
        reactants: &[GeneralizedRoleInput],
    ) -> Result<Result<Vec<GeneralizedRoleInput>, GeneralizedElaborationFailure>, CatalogueError>
    {
        let Some(rule) = self.generalized_rule(id) else {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::Unsupported,
                format!("generalized rule `{id}` does not resolve"),
            )));
        };
        let by_role = reactants
            .iter()
            .map(|input| (input.role.clone(), input))
            .collect::<BTreeMap<_, _>>();
        let reactant_roles = rule
            .roles
            .iter()
            .filter(|(_, schema)| schema.side == RuleSideRecord::Reactant)
            .map(|(role, _)| role)
            .collect::<BTreeSet<_>>();
        if by_role.len() != reactants.len()
            || by_role.keys().collect::<BTreeSet<_>>() != reactant_roles
        {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::InvalidSource,
                "generalized reactant role binding is incomplete or duplicated",
            )));
        }
        for (role, input) in &by_role {
            let schema = &rule.roles[role];
            if input.coefficient != schema.coefficient
                || input.side != schema.side
                || input.representation != schema.representation
            {
                return Ok(Err(failure(
                    GeneralizedElaborationFailureClass::InvalidSource,
                    format!("role `{role}` shape does not match generalized rule"),
                )));
            }
        }
        let domains = self.generalized_parameter_domains(id).ok_or_else(|| {
            CatalogueError::new(
                super::CatalogueErrorCode::InvalidGeneralizedRule,
                format!("generalized rule `{id}` lost its validated parameter domains"),
            )
        })?;
        let mut binding = BTreeMap::new();
        for (role, selector) in &rule.reactants {
            if let Err(result) = infer_selector_binding(
                self,
                selector,
                &by_role[role].structure,
                &mut binding,
                domains,
            ) {
                return Ok(Err(result));
            }
        }
        for (parameter, domain) in domains {
            if !binding.contains_key(parameter) {
                if domain.len() != 1 {
                    return Ok(Err(failure(
                        GeneralizedElaborationFailureClass::Ambiguous,
                        format!("parameter `{parameter}` is not uniquely induced by source roles"),
                    )));
                }
                let Some(value) = domain.first() else {
                    return Err(CatalogueError::new(
                        super::CatalogueErrorCode::InvalidGeneralizedRule,
                        format!("generalized parameter `{parameter}` lost its finite domain"),
                    ));
                };
                binding.insert(parameter.clone(), value.clone());
            }
        }
        let Some(selection) = self.select_generalized_case(id, &binding)? else {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::Unsupported,
                format!("rule `{id}` has no reviewed case for the inferred parameter binding"),
            )));
        };
        let case = match selection {
            GeneralizedCaseSelection::Unsupported(case) => {
                let GeneralizedReactionCaseRecord::Unsupported {
                    required_feature,
                    explanation,
                    ..
                } = case
                else {
                    return Err(CatalogueError::new(
                        super::CatalogueErrorCode::InvalidGeneralizedCase,
                        "generalized case selection has the wrong status",
                    ));
                };
                return Ok(Err(GeneralizedElaborationFailure {
                    class: GeneralizedElaborationFailureClass::Unsupported,
                    message: explanation.clone(),
                    required_feature: Some(required_feature.clone()),
                }));
            }
            GeneralizedCaseSelection::Supported(case) => case,
        };
        let GeneralizedReactionCaseRecord::Supported { products, .. } = case else {
            return Err(CatalogueError::new(
                super::CatalogueErrorCode::InvalidGeneralizedCase,
                "generalized case selection has the wrong status",
            ));
        };
        let mut derived = Vec::new();
        for (role, selector) in products {
            let resolved = super::generalized::resolve_selector(
                selector,
                &binding,
                self.structures(),
                &self.structure_traits,
                &self.document().structure_applications,
            )?;
            let mut resolved = resolved.into_iter();
            let (Some(structure), None) = (resolved.next(), resolved.next()) else {
                return Ok(Err(failure(
                    GeneralizedElaborationFailureClass::Ambiguous,
                    format!("product role `{role}` does not resolve to exactly one structure"),
                )));
            };
            let schema = &rule.roles[role];
            derived.push(GeneralizedRoleInput {
                role: role.clone(),
                structure,
                coefficient: schema.coefficient,
                side: schema.side,
                representation: schema.representation,
            });
        }
        Ok(Ok(derived))
    }
}

fn infer_selector_binding(
    catalogue: &ValidatedCatalogueBundle,
    selector: &GeneralizedStructureSelectorRecord,
    actual: &StructureId,
    binding: &mut BTreeMap<String, String>,
    domains: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), GeneralizedElaborationFailure> {
    match selector {
        GeneralizedStructureSelectorRecord::Exact { structure } => {
            if structure != actual {
                return Err(failure(
                    GeneralizedElaborationFailureClass::Unsupported,
                    format!("structure `{actual}` does not satisfy exact selector `{structure}`"),
                ));
            }
        }
        GeneralizedStructureSelectorRecord::Template {
            template,
            arguments,
        } => {
            let Some(application) = catalogue.structure_application(actual) else {
                return Err(failure(
                    GeneralizedElaborationFailureClass::Unsupported,
                    format!("structure `{actual}` is not a reviewed template application"),
                ));
            };
            if &application.template != template {
                return Err(failure(
                    GeneralizedElaborationFailureClass::Unsupported,
                    format!("structure `{actual}` uses a different template"),
                ));
            }
            for (name, argument) in arguments {
                let actual_value = &application.arguments[name];
                match argument {
                    GeneralizedArgumentRecord::Literal(expected) if expected != actual_value => {
                        return Err(failure(
                            GeneralizedElaborationFailureClass::Unsupported,
                            format!("structure `{actual}` has a different template argument"),
                        ));
                    }
                    GeneralizedArgumentRecord::Parameter(reference) => {
                        assign_parameter(&reference.parameter, actual_value, binding, domains)?;
                    }
                    GeneralizedArgumentRecord::Literal(_) => {}
                }
            }
        }
        GeneralizedStructureSelectorRecord::Trait { trait_id } => {
            if catalogue
                .structure_trait_assertion(actual, trait_id)
                .is_none()
            {
                return Err(failure(
                    GeneralizedElaborationFailureClass::Unsupported,
                    format!("structure `{actual}` does not satisfy trait `{trait_id}`"),
                ));
            }
        }
        GeneralizedStructureSelectorRecord::StructureParameter { parameter } => {
            assign_parameter(parameter, &actual.to_string(), binding, domains)?;
        }
    }
    Ok(())
}

fn assign_parameter(
    parameter: &str,
    value: &str,
    binding: &mut BTreeMap<String, String>,
    domains: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), GeneralizedElaborationFailure> {
    if !domains
        .get(parameter)
        .is_some_and(|domain| domain.contains(value))
    {
        return Err(failure(
            GeneralizedElaborationFailureClass::Unsupported,
            format!("value `{value}` is outside parameter `{parameter}` domain"),
        ));
    }
    if binding
        .insert(parameter.to_owned(), value.to_owned())
        .is_some_and(|previous| previous != value)
    {
        return Err(failure(
            GeneralizedElaborationFailureClass::InvalidSource,
            format!("source roles infer conflicting values for parameter `{parameter}`"),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn elaborate_supported(
    catalogue: &ValidatedCatalogueBundle,
    generalized: &GeneralizedReactionRuleRecord,
    case: &GeneralizedReactionCaseRecord,
    binding: &BTreeMap<String, String>,
    by_role: &BTreeMap<String, &GeneralizedRoleInput>,
) -> Result<Result<ElaboratedGeneralizedRule, GeneralizedElaborationFailure>, CatalogueError> {
    let GeneralizedReactionCaseRecord::Supported {
        id: case_id,
        products,
        patterns,
        correspondence,
        rewrite,
        observation_compatibility,
        ..
    } = case
    else {
        return Err(CatalogueError::new(
            super::CatalogueErrorCode::InvalidGeneralizedCase,
            "generalized supported elaboration received an unsupported case",
        ));
    };
    let element_parameters = generalized
        .parameters
        .iter()
        .filter_map(|(name, parameter)| {
            matches!(parameter, GeneralizedParameterRecord::Element { .. })
                .then_some((name, &binding[name]))
        })
        .map(|(name, value)| {
            ElementSymbol::from_str(value)
                .map(|symbol| (name.clone(), symbol))
                .map_err(|error| {
                    CatalogueError::new(
                        super::CatalogueErrorCode::InvalidGeneralizedRule,
                        format!("validated element parameter is corrupt: {error}"),
                    )
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let matches = match enumerate_instance_matches(
        catalogue,
        generalized,
        patterns,
        by_role,
        &element_parameters,
    )? {
        Ok(matches) => matches,
        Err(result) => return Ok(Err(result)),
    };
    if matches.representatives.is_empty() {
        return Ok(Err(failure(
            GeneralizedElaborationFailureClass::Unsupported,
            "selected generalized case has no graph match",
        )));
    }
    let role_symmetries = match certificate_role_symmetries(catalogue, generalized, by_role)? {
        Ok(symmetries) => symmetries,
        Err(result) => return Ok(Err(result)),
    };
    let mut work = 0_usize;
    let mut classes =
        BTreeMap::<Vec<u8>, (Vec<u8>, ReactionRuleRecord, InstancePatternMatches)>::new();
    for matched in &matches.representatives {
        let concrete = instantiate_rule(
            generalized,
            case_id,
            products,
            patterns,
            correspondence,
            rewrite,
            observation_compatibility,
            by_role,
            matched,
        )?;
        let raw_key = canonical_rule_key(&concrete)?;
        let certificate_key =
            match canonical_certificate_key(&concrete, &role_symmetries, &mut work)? {
                Ok(key) => key,
                Err(result) => return Ok(Err(result)),
            };
        classes
            .entry(certificate_key)
            .and_modify(|representative| {
                if raw_key < representative.0 {
                    *representative = (raw_key.clone(), concrete.clone(), matched.clone());
                }
            })
            .or_insert((raw_key, concrete, matched.clone()));
    }
    if classes.len() != 1 {
        return Ok(Err(failure(
            GeneralizedElaborationFailureClass::Ambiguous,
            "selected generalized case has multiple non-equivalent complete certificates",
        )));
    }
    let Some((_, mut representative, representative_match)) = classes.into_values().next() else {
        return Err(CatalogueError::new(
            super::CatalogueErrorCode::InvalidGeneralizedCase,
            "generalized certificate class vanished after non-empty matching",
        ));
    };
    let provenance =
        generalized_provenance(catalogue, generalized, case, patterns, binding, by_role)?;
    attach_local_premises(&mut representative, &provenance.selected, &provenance.roles)?;
    Ok(Ok(ElaboratedGeneralizedRule {
        rule: representative,
        parameter_binding: binding.clone(),
        parameter_premise_ids: provenance.parameters,
        case_id: case_id.clone(),
        equivalent_match_count: matches.equivalent_match_count,
        matched_sites: matched_sites(&representative_match),
        role_premise_ids: provenance.roles,
        selected_premise_ids: provenance.selected,
    }))
}

struct GeneralizedProvenance {
    parameters: BTreeMap<String, BTreeSet<PremiseId>>,
    roles: BTreeMap<String, BTreeSet<PremiseId>>,
    selected: BTreeSet<PremiseId>,
}

fn generalized_provenance(
    catalogue: &ValidatedCatalogueBundle,
    generalized: &GeneralizedReactionRuleRecord,
    case: &GeneralizedReactionCaseRecord,
    patterns: &BTreeMap<String, super::GraphPatternId>,
    binding: &BTreeMap<String, String>,
    by_role: &BTreeMap<String, &GeneralizedRoleInput>,
) -> Result<GeneralizedProvenance, CatalogueError> {
    let mut selected = generalized.premise_ids.clone();
    selected.extend(case.premise_ids().iter().cloned());
    selected.insert(generalized.applicability.premise_id.clone());
    selected.extend(generalized.model_assumptions.premise_ids.iter().cloned());

    let mut parameters = BTreeMap::new();
    for (name, parameter) in &generalized.parameters {
        let mut premises = BTreeSet::new();
        match parameter {
            GeneralizedParameterRecord::Element { category } => {
                let symbol = ElementSymbol::from_str(&binding[name]).map_err(|error| {
                    CatalogueError::new(
                        super::CatalogueErrorCode::InvalidGeneralizedRule,
                        format!("validated element binding is corrupt: {error}"),
                    )
                })?;
                let membership = catalogue
                    .element_membership_provenance(&symbol, category)
                    .ok_or_else(|| {
                        CatalogueError::new(
                            super::CatalogueErrorCode::InvalidGeneralizedRule,
                            format!("element parameter `{name}` lost membership provenance"),
                        )
                    })?;
                premises.extend(membership.element_premise_ids.iter().cloned());
                premises.extend(membership.category_premise_ids.iter().cloned());
            }
            GeneralizedParameterRecord::Structure { trait_id } => {
                let structure = StructureId::from_str(&binding[name]).map_err(|error| {
                    CatalogueError::new(
                        super::CatalogueErrorCode::InvalidGeneralizedRule,
                        format!("validated structure binding is corrupt: {error}"),
                    )
                })?;
                premises.extend(
                    catalogue
                        .structure_premises(&structure)
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
                premises.extend(
                    catalogue
                        .structural_trait(trait_id)
                        .into_iter()
                        .flat_map(|record| record.premise_ids.iter().cloned()),
                );
                premises.extend(
                    catalogue
                        .structure_trait_assertion(&structure, trait_id)
                        .into_iter()
                        .flat_map(|record| record.premise_ids.iter().cloned()),
                );
            }
            GeneralizedParameterRecord::Enum { .. } => {}
        }
        selected.extend(premises.iter().cloned());
        parameters.insert(name.clone(), premises);
    }

    let mut roles = BTreeMap::new();
    for role in generalized.roles.keys() {
        let mut premises = selected.clone();
        premises.extend(
            catalogue
                .structure_premises(&by_role[role].structure)
                .into_iter()
                .flatten()
                .cloned(),
        );
        if let Some(pattern) = patterns
            .get(role)
            .and_then(|id| catalogue.graph_pattern(id))
        {
            premises.extend(pattern.premise_ids.iter().cloned());
        }
        roles.insert(role.clone(), premises);
    }
    Ok(GeneralizedProvenance {
        parameters,
        roles,
        selected,
    })
}

fn attach_local_premises(
    rule: &mut ReactionRuleRecord,
    selected: &BTreeSet<PremiseId>,
    roles: &BTreeMap<String, BTreeSet<PremiseId>>,
) -> Result<(), CatalogueError> {
    rule.premise_ids.extend(selected.iter().cloned());
    rule.premise_ids.extend(roles.values().flatten().cloned());
    for pair in &mut rule.mapping_template {
        pair.premise_ids.extend(selected.iter().cloned());
        for role in referenced_roles([pair.reactant.as_str(), pair.product.as_str()], roles) {
            pair.premise_ids.extend(roles[role].iter().cloned());
        }
    }
    for operation in &mut rule.operation_template {
        let value = serde_json::to_value(&*operation).map_err(|error| {
            CatalogueError::new(
                super::CatalogueErrorCode::InvalidGeneralizedRule,
                error.to_string(),
            )
        })?;
        let mut strings = Vec::new();
        collect_strings(&value, &mut strings);
        let mut premises = operation.premise_ids().clone();
        premises.extend(selected.iter().cloned());
        for role in referenced_roles(strings.iter().map(String::as_str), roles) {
            premises.extend(roles[role].iter().cloned());
        }
        set_operation_premises(operation, premises);
    }
    Ok(())
}

fn referenced_roles<'a>(
    references: impl IntoIterator<Item = &'a str>,
    roles: &'a BTreeMap<String, BTreeSet<PremiseId>>,
) -> BTreeSet<&'a String> {
    references
        .into_iter()
        .filter_map(|reference| reference.split_once('[').map(|(role, _)| role))
        .filter_map(|role| roles.get_key_value(role).map(|(role, _)| role))
        .collect()
}

fn collect_strings(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => output.push(text.clone()),
        serde_json::Value::Array(values) => {
            for value in values {
                collect_strings(value, output);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                collect_strings(value, output);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

fn set_operation_premises(operation: &mut OperationTemplateRecord, premises: BTreeSet<PremiseId>) {
    match operation {
        OperationTemplateRecord::ReconfigureElectrons { premise_ids, .. }
        | OperationTemplateRecord::CleaveCovalent { premise_ids, .. }
        | OperationTemplateRecord::FormCovalent { premise_ids, .. }
        | OperationTemplateRecord::CleaveDative { premise_ids, .. }
        | OperationTemplateRecord::FormDative { premise_ids, .. }
        | OperationTemplateRecord::ChangeCovalent { premise_ids, .. }
        | OperationTemplateRecord::ChangeCovalentDelocalization { premise_ids, .. }
        | OperationTemplateRecord::AssociateIonic { premise_ids, .. }
        | OperationTemplateRecord::DissociateIonic { premise_ids, .. }
        | OperationTemplateRecord::ReleaseMetallic { premise_ids, .. }
        | OperationTemplateRecord::JoinMetallic { premise_ids, .. }
        | OperationTemplateRecord::TransferElectron { premise_ids, .. }
        | OperationTemplateRecord::AssignProduct { premise_ids, .. } => *premise_ids = premises,
    }
}

fn matched_sites(matched: &InstancePatternMatches) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for (role, instances) in matched {
        for (index, binding) in instances.iter().enumerate() {
            let sites = binding
                .atoms()
                .iter()
                .map(|(name, value)| (name.clone(), value.to_string()))
                .chain(
                    binding
                        .covalent_bonds()
                        .iter()
                        .map(|(name, value)| (name.clone(), value.to_string())),
                )
                .chain(
                    binding
                        .groups()
                        .iter()
                        .map(|(name, value)| (name.clone(), value.to_string())),
                )
                .chain(
                    binding
                        .ionic_associations()
                        .iter()
                        .map(|(name, value)| (name.clone(), value.to_string())),
                )
                .chain(
                    binding
                        .metallic_domains()
                        .iter()
                        .map(|(name, value)| (name.clone(), value.to_string())),
                )
                .collect();
            result.insert(format!("{role}[{}]", index + 1), sites);
        }
    }
    result
}

fn enumerate_instance_matches(
    catalogue: &ValidatedCatalogueBundle,
    generalized: &GeneralizedReactionRuleRecord,
    patterns: &BTreeMap<String, super::GraphPatternId>,
    by_role: &BTreeMap<String, &GeneralizedRoleInput>,
    element_parameters: &BTreeMap<String, ElementSymbol>,
) -> Result<Result<EnumeratedInstanceMatches, GeneralizedElaborationFailure>, CatalogueError> {
    let mut combined = vec![BTreeMap::new()];
    let mut equivalent_match_count = 1_usize;
    for (role, pattern) in patterns {
        if !catalogue.pattern_match_work_is_bounded(pattern, &by_role[role].structure)? {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::Ambiguous,
                format!("role `{role}` graph matching exceeds the work limit"),
            )));
        }
        let raw = catalogue.raw_pattern_matches(
            &[PatternRoleInput {
                role: role.clone(),
                pattern: pattern.clone(),
                structure: by_role[role].structure.clone(),
            }],
            element_parameters,
        )?;
        let raw_role_matches = raw
            .iter()
            .filter_map(|matched| matched.roles().get(role).cloned())
            .collect::<Vec<_>>();
        let mut role_matches = Vec::new();
        for candidate in &raw_role_matches {
            let mut equivalent = false;
            for representative in &role_matches {
                if catalogue
                    .role_pattern_matches_are_automorphism_related(representative, candidate)?
                {
                    equivalent = true;
                    break;
                }
            }
            if !equivalent {
                role_matches.push(candidate.clone());
            }
        }
        let coefficient = generalized.roles[role].coefficient as usize;
        let raw_selection_count = raw_role_matches
            .len()
            .checked_pow(generalized.roles[role].coefficient)
            .ok_or_else(|| {
                CatalogueError::new(
                    super::CatalogueErrorCode::InvalidGeneralizedRule,
                    "equivalent match count exceeds the platform limit",
                )
            })?;
        equivalent_match_count = equivalent_match_count
            .checked_mul(raw_selection_count)
            .ok_or_else(|| {
                CatalogueError::new(
                    super::CatalogueErrorCode::InvalidGeneralizedRule,
                    "equivalent match count exceeds the platform limit",
                )
            })?;
        let selection_count = role_matches
            .len()
            .checked_pow(generalized.roles[role].coefficient)
            .unwrap_or(usize::MAX);
        if selection_count > MAX_CERTIFICATE_CANDIDATES
            || combined
                .len()
                .checked_mul(selection_count)
                .is_none_or(|count| count > MAX_CERTIFICATE_CANDIDATES)
        {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::Ambiguous,
                "independent pattern-instance enumeration exceeds the work limit",
            )));
        }
        let mut selections = vec![Vec::new()];
        for _ in 0..coefficient {
            selections = selections
                .into_iter()
                .flat_map(|prefix| {
                    role_matches.iter().cloned().map(move |matched| {
                        let mut value = prefix.clone();
                        value.push(matched);
                        value
                    })
                })
                .collect();
        }
        let mut next = Vec::with_capacity(combined.len() * selections.len());
        for prefix in &combined {
            for selection in &selections {
                let mut value = prefix.clone();
                value.insert(role.clone(), selection.clone());
                next.push(value);
            }
        }
        combined = next;
    }
    Ok(Ok(EnumeratedInstanceMatches {
        representatives: combined,
        equivalent_match_count,
    }))
}

type RoleSymmetries = Vec<(String, u32, Vec<StructureAutomorphism>)>;

fn certificate_role_symmetries(
    catalogue: &ValidatedCatalogueBundle,
    generalized: &GeneralizedReactionRuleRecord,
    by_role: &BTreeMap<String, &GeneralizedRoleInput>,
) -> Result<Result<RoleSymmetries, GeneralizedElaborationFailure>, CatalogueError> {
    let mut result = Vec::new();
    for (role, schema) in &generalized.roles {
        let Some(automorphisms) = catalogue.structure_automorphisms(&by_role[role].structure)?
        else {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::Ambiguous,
                format!("role `{role}` exceeds the automorphism work limit"),
            )));
        };
        if automorphisms.is_empty() {
            return Ok(Err(failure(
                GeneralizedElaborationFailureClass::InvalidSource,
                format!("role `{role}` has no structure automorphism"),
            )));
        }
        result.push((role.clone(), schema.coefficient, automorphisms));
    }
    Ok(Ok(result))
}

fn canonical_rule_key(rule: &ReactionRuleRecord) -> Result<Vec<u8>, CatalogueError> {
    let value = serde_json::to_value(rule).map_err(|error| {
        CatalogueError::new(
            super::CatalogueErrorCode::InvalidGeneralizedRule,
            error.to_string(),
        )
    })?;
    canonical_json(&value).map_err(|error| {
        CatalogueError::new(
            super::CatalogueErrorCode::InvalidGeneralizedRule,
            error.to_string(),
        )
    })
}

fn canonical_certificate_key(
    rule: &ReactionRuleRecord,
    role_symmetries: &[(String, u32, Vec<StructureAutomorphism>)],
    work: &mut usize,
) -> Result<Result<Vec<u8>, GeneralizedElaborationFailure>, CatalogueError> {
    // Role instances have canonical ordinals, and each structure automorphism
    // changes references inside exactly one such instance. Canonicalize those
    // independent choices one instance at a time. This is equivalent to the
    // former Cartesian-product search, but its work is the sum rather than the
    // product of the automorphism counts (important for M2O5 and larger ions).
    let mut transformed = rule.clone();
    for (role, coefficient, automorphisms) in role_symmetries {
        for ordinal in 1..=*coefficient {
            let instance = format!("{role}[{ordinal}]");
            let mut minimum = None::<(Vec<u8>, ReactionRuleRecord)>;
            for automorphism in automorphisms {
                *work += 1;
                if *work > MAX_CERTIFICATE_CANDIDATES {
                    return Ok(Err(failure(
                        GeneralizedElaborationFailureClass::Ambiguous,
                        "complete-certificate comparison exceeds the work limit",
                    )));
                }
                let mut map = BTreeMap::new();
                map.insert(instance.clone(), instance.clone());
                for (source, target) in automorphism.sites() {
                    map.insert(
                        format!("{instance}.{source}"),
                        format!("{instance}.{target}"),
                    );
                }
                let mut candidate = transformed.clone();
                transform_rule_references(&mut candidate, &map);
                let key = canonical_rule_key(&candidate)?;
                if minimum.as_ref().is_none_or(|(current, _)| key < *current) {
                    minimum = Some((key, candidate));
                }
            }
            let Some((_, canonical)) = minimum else {
                return Err(CatalogueError::new(
                    super::CatalogueErrorCode::InvalidGeneralizedRule,
                    "validated generalized role symmetry set is empty",
                ));
            };
            transformed = canonical;
        }
    }
    Ok(Ok(canonical_rule_key(&transformed)?))
}

fn transform_rule_references(rule: &mut ReactionRuleRecord, map: &BTreeMap<String, String>) {
    for pair in &mut rule.mapping_template {
        transform_reference(&mut pair.reactant, map);
        transform_reference(&mut pair.product, map);
    }
    for operation in &mut rule.operation_template {
        transform_operation_references(operation, map);
    }
}

#[allow(clippy::too_many_lines)]
fn transform_operation_references(
    operation: &mut OperationTemplateRecord,
    map: &BTreeMap<String, String>,
) {
    match operation {
        OperationTemplateRecord::ReconfigureElectrons { atom, .. } => {
            transform_reference(atom, map);
        }
        OperationTemplateRecord::CleaveCovalent {
            edge, allocation, ..
        } => {
            transform_reference(&mut edge.0, map);
            transform_reference(&mut edge.1, map);
            transform_allocation_reference(allocation, map);
        }
        OperationTemplateRecord::FormCovalent { edge, .. } => {
            transform_reference(&mut edge.0, map);
            transform_reference(&mut edge.1, map);
        }
        OperationTemplateRecord::CleaveDative {
            donor,
            acceptor,
            allocation,
            ..
        } => {
            transform_reference(donor, map);
            transform_reference(acceptor, map);
            transform_allocation_reference(allocation, map);
        }
        OperationTemplateRecord::FormDative {
            donor, acceptor, ..
        }
        | OperationTemplateRecord::TransferElectron {
            donor, acceptor, ..
        } => {
            transform_reference(donor, map);
            transform_reference(acceptor, map);
        }
        OperationTemplateRecord::ChangeCovalent {
            edge, allocation, ..
        } => {
            transform_reference(&mut edge.0, map);
            transform_reference(&mut edge.1, map);
            transform_allocation_reference(allocation, map);
        }
        OperationTemplateRecord::ChangeCovalentDelocalization { edge, .. } => {
            transform_reference(&mut edge.0, map);
            transform_reference(&mut edge.1, map);
        }
        OperationTemplateRecord::AssociateIonic { components, .. } => {
            for component in components {
                for site in component {
                    transform_reference(site, map);
                }
            }
        }
        OperationTemplateRecord::DissociateIonic { association, .. } => {
            transform_reference(association, map);
        }
        OperationTemplateRecord::ReleaseMetallic { site, domain, .. }
        | OperationTemplateRecord::JoinMetallic { site, domain, .. } => {
            transform_reference(site, map);
            transform_reference(domain, map);
        }
        OperationTemplateRecord::AssignProduct { atoms, product, .. } => {
            for atom in atoms {
                transform_reference(atom, map);
            }
            transform_reference(product, map);
        }
    }
}

fn transform_allocation_reference(
    allocation: &mut CleavageAllocationRecord,
    map: &BTreeMap<String, String>,
) {
    if let CleavageAllocationRecord::Heterolytic { heterolytic_to } = allocation {
        transform_reference(heterolytic_to, map);
    }
}

fn transform_reference(reference: &mut String, map: &BTreeMap<String, String>) {
    if let Some(replacement) = map.get(reference) {
        *reference = replacement.clone();
    }
}

#[allow(clippy::too_many_arguments)]
fn instantiate_rule(
    generalized: &GeneralizedReactionRuleRecord,
    _case_id: &str,
    _products: &BTreeMap<String, GeneralizedStructureSelectorRecord>,
    _patterns: &BTreeMap<String, super::GraphPatternId>,
    correspondence: &[MappingPairRecord],
    rewrite: &[OperationTemplateRecord],
    observations: &[super::ObservationCompatibilityRecord],
    by_role: &BTreeMap<String, &GeneralizedRoleInput>,
    matched: &InstancePatternMatches,
) -> Result<ReactionRuleRecord, CatalogueError> {
    let roles = generalized
        .roles
        .iter()
        .map(|(role, schema)| {
            (
                role.clone(),
                RoleSchemaRecord {
                    side: schema.side,
                    representation: schema.representation,
                },
            )
        })
        .collect();
    let terms = |side| {
        generalized
            .roles
            .iter()
            .filter(|(_, schema)| schema.side == side)
            .map(|(role, schema)| PatternTermRecord {
                role: role.clone(),
                structure_id: by_role[role].structure.clone(),
                coefficient: schema.coefficient,
            })
            .collect()
    };
    Ok(ReactionRuleRecord {
        id: generalized.id.clone(),
        premise_ids: generalized.premise_ids.clone(),
        roles,
        reactant_pattern: terms(RuleSideRecord::Reactant),
        product_pattern: terms(RuleSideRecord::Product),
        applicability: ApplicabilityRecord {
            premise_id: generalized.applicability.premise_id.clone(),
            request_relation: generalized.applicability.request_relation,
            reactant_structure_ids: generalized
                .roles
                .iter()
                .filter(|(_, schema)| schema.side == RuleSideRecord::Reactant)
                .map(|(role, _)| by_role[role].structure.clone())
                .collect(),
            required_context: generalized.applicability.required_context.clone(),
        },
        mapping_template: correspondence
            .iter()
            .map(|pair| {
                Ok(MappingPairRecord {
                    reactant: instantiate_reference(&pair.reactant, matched)?,
                    product: pair.product.clone(),
                    premise_ids: pair.premise_ids.clone(),
                })
            })
            .collect::<Result<Vec<_>, CatalogueError>>()?,
        operation_template: rewrite
            .iter()
            .map(|operation| instantiate_operation(operation, matched))
            .collect::<Result<Vec<_>, CatalogueError>>()?,
        model_assumptions: generalized.model_assumptions.clone(),
        observation_compatibility: observations.to_vec(),
    })
}

#[allow(clippy::too_many_lines)]
fn instantiate_operation(
    operation: &OperationTemplateRecord,
    matched: &InstancePatternMatches,
) -> Result<OperationTemplateRecord, CatalogueError> {
    let reference = |value: &str| instantiate_reference(value, matched);
    Ok(match operation {
        OperationTemplateRecord::ReconfigureElectrons {
            premise_ids,
            atom,
            before,
            after,
        } => OperationTemplateRecord::ReconfigureElectrons {
            premise_ids: premise_ids.clone(),
            atom: reference(atom)?,
            before: *before,
            after: *after,
        },
        OperationTemplateRecord::CleaveCovalent {
            premise_ids,
            edge,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::CleaveCovalent {
            premise_ids: premise_ids.clone(),
            edge: (reference(&edge.0)?, reference(&edge.1)?, edge.2),
            allocation: instantiate_allocation(allocation, matched)?,
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::FormCovalent {
            premise_ids,
            edge,
            electron_contribution,
            before,
            after,
        } => OperationTemplateRecord::FormCovalent {
            premise_ids: premise_ids.clone(),
            edge: (reference(&edge.0)?, reference(&edge.1)?, edge.2),
            electron_contribution: electron_contribution.clone(),
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::CleaveDative {
            premise_ids,
            donor,
            acceptor,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::CleaveDative {
            premise_ids: premise_ids.clone(),
            donor: reference(donor)?,
            acceptor: reference(acceptor)?,
            allocation: instantiate_allocation(allocation, matched)?,
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::FormDative {
            premise_ids,
            donor,
            acceptor,
            before,
            after,
        } => OperationTemplateRecord::FormDative {
            premise_ids: premise_ids.clone(),
            donor: reference(donor)?,
            acceptor: reference(acceptor)?,
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::ChangeCovalent {
            premise_ids,
            edge,
            old_order,
            new_order,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::ChangeCovalent {
            premise_ids: premise_ids.clone(),
            edge: (reference(&edge.0)?, reference(&edge.1)?),
            old_order: *old_order,
            new_order: *new_order,
            allocation: instantiate_allocation(allocation, matched)?,
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::ChangeCovalentDelocalization {
            premise_ids,
            edge,
            expected,
            replacement,
        } => OperationTemplateRecord::ChangeCovalentDelocalization {
            premise_ids: premise_ids.clone(),
            edge: (reference(&edge.0)?, reference(&edge.1)?),
            expected: expected.clone(),
            replacement: replacement.clone(),
        },
        OperationTemplateRecord::AssociateIonic {
            premise_ids,
            label,
            components,
            component_charges,
        } => OperationTemplateRecord::AssociateIonic {
            premise_ids: premise_ids.clone(),
            label: label.clone(),
            components: components
                .iter()
                .map(|component| {
                    component
                        .iter()
                        .map(|site| reference(site))
                        .collect::<Result<Vec<_>, CatalogueError>>()
                })
                .collect::<Result<Vec<_>, CatalogueError>>()?,
            component_charges: component_charges.clone(),
        },
        OperationTemplateRecord::DissociateIonic {
            premise_ids,
            association,
        } => OperationTemplateRecord::DissociateIonic {
            premise_ids: premise_ids.clone(),
            association: reference(association)?,
        },
        OperationTemplateRecord::ReleaseMetallic {
            premise_ids,
            site,
            domain,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::ReleaseMetallic {
            premise_ids: premise_ids.clone(),
            site: reference(site)?,
            domain: reference(domain)?,
            allocation: *allocation,
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::JoinMetallic {
            premise_ids,
            site,
            domain,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::JoinMetallic {
            premise_ids: premise_ids.clone(),
            site: reference(site)?,
            domain: reference(domain)?,
            allocation: *allocation,
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::TransferElectron {
            premise_ids,
            count,
            donor,
            acceptor,
            before,
            after,
        } => OperationTemplateRecord::TransferElectron {
            premise_ids: premise_ids.clone(),
            count: *count,
            donor: reference(donor)?,
            acceptor: reference(acceptor)?,
            before: before.clone(),
            after: after.clone(),
        },
        OperationTemplateRecord::AssignProduct {
            premise_ids,
            atoms,
            product,
        } => OperationTemplateRecord::AssignProduct {
            premise_ids: premise_ids.clone(),
            atoms: atoms
                .iter()
                .map(|atom| reference(atom))
                .collect::<Result<Vec<_>, CatalogueError>>()?,
            product: product.clone(),
        },
    })
}

fn instantiate_allocation(
    allocation: &CleavageAllocationRecord,
    matched: &InstancePatternMatches,
) -> Result<CleavageAllocationRecord, CatalogueError> {
    Ok(match allocation {
        CleavageAllocationRecord::Homolytic(value) => {
            CleavageAllocationRecord::Homolytic(value.clone())
        }
        CleavageAllocationRecord::Heterolytic { heterolytic_to } => {
            CleavageAllocationRecord::Heterolytic {
                heterolytic_to: instantiate_reference(heterolytic_to, matched)?,
            }
        }
    })
}

fn instantiate_reference(
    reference: &str,
    matched: &InstancePatternMatches,
) -> Result<String, CatalogueError> {
    let Some((instance, site)) = reference.split_once('.') else {
        return Ok(reference.to_owned());
    };
    let Some((role, ordinal)) = instance.split_once('[') else {
        return Ok(reference.to_owned());
    };
    let Some(ordinal) = ordinal
        .strip_suffix(']')
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
    else {
        return Err(CatalogueError::new(
            super::CatalogueErrorCode::InvalidGeneralizedCase,
            format!("validated generalized reference `{reference}` has an invalid ordinal"),
        ));
    };
    let Some(role_binding) = matched
        .get(role)
        .and_then(|instances| instances.get(ordinal))
    else {
        return Ok(reference.to_owned());
    };
    let resolved = role_binding.resolved_site(site).ok_or_else(|| {
        CatalogueError::new(
            super::CatalogueErrorCode::InvalidGeneralizedCase,
            format!("validated generalized rewrite site `{reference}` no longer resolves"),
        )
    })?;
    Ok(format!("{instance}.{resolved}"))
}

fn failure(
    class: GeneralizedElaborationFailureClass,
    message: impl Into<String>,
) -> GeneralizedElaborationFailure {
    GeneralizedElaborationFailure {
        class,
        message: message.into(),
        required_feature: None,
    }
}
