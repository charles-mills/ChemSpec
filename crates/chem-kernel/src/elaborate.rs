use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use chem_catalogue::{
    BinaryElectronStateRecord, BondDelocalizationRecord, BondOrderRecord, CleavageAllocationRecord,
    ElaboratedGeneralizedRule, ElectronStateRecord, EventModel, GeneralizedElaborationFailureClass,
    GeneralizedRoleInput, MetallicJoinAllocationRecord, MetallicReleaseAllocationRecord,
    ObservationPredicate, OperationTemplateRecord, ReactionRuleRecord, RepresentationRecord,
    RuleSideRecord, SequenceModel, TransferElectronStateRecord, TrustedCatalogue,
    ValidatedCatalogueBundle,
};
use chem_domain::{
    AtomGroup, AtomGroupId, AtomId, AtomMapping, AtomMappingId, BondOrder, ClaimId, ContentDigest,
    CovalentDelocalization, CovalentDelocalizationId, EffectiveBondOrder, ElectronAllocation,
    ElectronState, ElectronTransition, ElementSymbol, IonicAssociation, IonicAssociationId,
    MetallicDomainId, MetallicJoinAllocation, MetallicReleaseAllocation, PremiseId, ReactionRuleId,
    ReactionSide, RepresentationKind, StructuralOperation, StructuralOperationId,
    StructuralOperationInput, StructureDefinition, StructureId, StructureInstance,
    StructureInstanceId,
};
use chems_lang::{
    ByteSpan, SourceAst, SourceEquationTerm, SourceObservation, SourceReaction,
    SourceRepresentationKind, SourceStructureBinding, parse_source,
};
use serde_json::Value;

use crate::{
    CatalogueOrigin, CatalogueReference, CatalogueTrust, EvidenceOrigin, EvidencePredicate,
    EvidenceTrust, ExpandedElectronContribution, ExpandedInstance, ExpandedIonicComponent,
    ExpandedOperation, ExpandedStructuralReaction, ExpansionError, Provenance, ReactionSideKind,
    ResolvedApplicability, ResolvedEquationTerm, ResolvedEvidence,
    ResolvedGeneralizedRuleApplication, ResolvedModel, ResolvedObservation, ResolvedReactionClaim,
    ResolvedRuleApplication, ResolvedRuleBinding, ResolvedStructureBinding, SourceOrigin,
    SourceReference, TrustedExpandedStructuralReaction, ValidatedEvidencePacket,
};

/// Elaborates source against a structurally valid but explicitly untrusted
/// catalogue review candidate.
///
/// # Errors
///
/// Returns a typed invalid, unsupported, or system-error result. The returned
/// HIR remains marked `review_candidate` and cannot represent production trust.
pub fn expand_review_candidate(
    source_name: &str,
    source: &str,
    catalogue: &ValidatedCatalogueBundle,
    evidence_json: &[u8],
) -> Result<ExpandedStructuralReaction, ExpansionError> {
    let evidence = ValidatedEvidencePacket::from_json(evidence_json)
        .map_err(|error| ExpansionError::invalid("CHEMS-X020", error.to_string(), None))?;
    expand(
        source_name,
        source,
        catalogue,
        CatalogueTrust::ReviewCandidate,
        &evidence,
    )
}

/// Elaborates source through the production trusted-catalogue boundary.
///
/// # Errors
///
/// Returns a typed invalid, unsupported, or system-error result.
pub fn expand_trusted(
    source_name: &str,
    source: &str,
    catalogue: &TrustedCatalogue,
    evidence_json: &[u8],
) -> Result<TrustedExpandedStructuralReaction, ExpansionError> {
    let evidence = ValidatedEvidencePacket::from_json(evidence_json)
        .map_err(|error| ExpansionError::invalid("CHEMS-X020", error.to_string(), None))?;
    let expanded = expand(
        source_name,
        source,
        catalogue,
        CatalogueTrust::Trusted,
        &evidence,
    )?;
    Ok(TrustedExpandedStructuralReaction { expanded })
}

#[allow(clippy::too_many_lines)]
fn expand(
    source_name: &str,
    source: &str,
    catalogue: &ValidatedCatalogueBundle,
    trust: CatalogueTrust,
    evidence: &ValidatedEvidencePacket,
) -> Result<ExpandedStructuralReaction, ExpansionError> {
    let parsed = parse_source(source);
    if !parsed.is_complete() {
        let first = parsed.diagnostics.first();
        return Err(ExpansionError::invalid(
            "CHEMS-X001",
            first.map_or_else(
                || "source is not a complete chems 1 document".to_owned(),
                |diagnostic| format!("{}: {}", diagnostic.code, diagnostic.summary),
            ),
            first.map(|diagnostic| diagnostic.primary_span),
        ));
    }
    let semantic_source_digest = semantic_source_digest(&parsed.ast)?;
    let source_reference = SourceReference {
        name: source_name.to_owned(),
        bytes_digest: ContentDigest::sha256(source.as_bytes()),
        semantic_digest: semantic_source_digest,
    };
    let selected_catalogue = parsed.ast.catalogue.as_ref().ok_or_else(|| {
        ExpansionError::invalid("CHEMS-X001", "catalogue selection is missing", None)
    })?;
    if selected_catalogue.name != catalogue.document().name
        || selected_catalogue.version != catalogue.document().version
    {
        return Err(ExpansionError::unsupported(
            "CHEMS-X002",
            format!(
                "catalogue {}@{} is unavailable; loaded catalogue is {}@{}",
                selected_catalogue.name,
                selected_catalogue.version,
                catalogue.document().name,
                catalogue.document().version
            ),
        ));
    }
    let reaction = parsed.ast.reaction.as_ref().ok_or_else(|| {
        ExpansionError::invalid("CHEMS-X001", "reaction declaration is missing", None)
    })?;

    let mut definitions = BTreeMap::new();
    let reactants = resolve_bindings(
        source_name,
        ReactionSideKind::Reactant,
        &reaction.reactants,
        catalogue,
        &mut definitions,
    )?;
    let products = resolve_bindings(
        source_name,
        ReactionSideKind::Product,
        &reaction.products,
        catalogue,
        &mut definitions,
    )?;
    if reactants.keys().any(|name| products.contains_key(name)) {
        return Err(ExpansionError::invalid(
            "CHEMS-X003",
            "reactant and product binding names must be globally unique",
            Some(reaction.span),
        ));
    }

    let equation = resolve_equation(source_name, reaction, &reactants, &products)?;
    let rule_application = reaction.rule_application.as_ref().ok_or_else(|| {
        ExpansionError::invalid("CHEMS-X004", "rule application is missing", None)
    })?;
    let rule_id = ReactionRuleId::from_str(&rule_application.rule).map_err(|error| {
        ExpansionError::invalid("CHEMS-X004", error.to_string(), Some(rule_application.span))
    })?;
    let selected_rule = select_rule(&rule_id, rule_application, &reactants, &products, catalogue)?;
    let rule = selected_rule.record.as_ref();
    let resolved_rule = resolve_rule(
        source_name,
        rule_application,
        &rule_id,
        rule,
        &reactants,
        &products,
        catalogue.digest(),
        selected_rule.generalized.as_ref(),
    )?;
    validate_applicability(rule, &reactants)?;
    let model = resolve_model(source_name, reaction, rule, catalogue.digest())?;
    let resolved_evidence = resolve_evidence(
        source_name,
        reaction,
        evidence,
        rule,
        &resolved_rule.bindings,
        &reactants,
        &products,
        catalogue.digest(),
    )?;

    let reactant_instances = expand_instances(&reactants, &definitions, catalogue.digest())?;
    let product_instances = expand_instances(&products, &definitions, catalogue.digest())?;
    let reactant_side = ReactionSide::new(
        reactant_instances
            .values()
            .map(|instance| instance.instance.clone()),
    )
    .map_err(system_structural)?;
    let product_side = ReactionSide::new(
        product_instances
            .values()
            .map(|instance| instance.instance.clone()),
    )
    .map_err(system_structural)?;
    let (mapping, mapping_entry_provenance) = expand_mapping(
        reaction,
        rule,
        &resolved_rule.bindings,
        &reactant_side,
        &product_side,
        catalogue.digest(),
    )?;
    let operations = expand_operations(
        source_name,
        reaction,
        rule,
        &resolved_rule.bindings,
        catalogue.digest(),
    )?;

    let atom_provenance = reactant_instances
        .values()
        .chain(product_instances.values())
        .flat_map(|instance| {
            instance.instance.graph().atoms().keys().map(|atom| {
                let mut provenance = instance.provenance.clone();
                provenance.catalogue.insert(catalogue_origin(
                    catalogue.digest(),
                    format!(
                        "structure {} expanded atom {atom}",
                        instance.instance.structure()
                    ),
                    provenance
                        .catalogue
                        .iter()
                        .flat_map(|origin| origin.premises.iter().cloned()),
                ));
                (atom.clone(), provenance)
            })
        })
        .collect();
    let mapping_premises = rule
        .mapping_template
        .iter()
        .flat_map(|entry| entry.premise_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mapping_provenance = Provenance::derived(
        resolved_rule
            .bindings
            .values()
            .flat_map(|binding| binding.provenance.source.iter().cloned()),
        [catalogue_origin(
            catalogue.digest(),
            format!("rule {} atom mapping template", rule.id),
            mapping_premises,
        )],
        [],
    );
    let premises = rule.premise_ids.clone();
    let premise_provenance = premises
        .iter()
        .map(|premise| {
            (
                premise.clone(),
                catalogue_origin(
                    catalogue.digest(),
                    format!("catalogue premise {premise}"),
                    [premise.clone()],
                ),
            )
        })
        .collect();
    let claim = ResolvedReactionClaim {
        source: source_reference,
        catalogue: CatalogueReference {
            name: selected_catalogue.name.clone(),
            version: selected_catalogue.version.clone(),
            digest: catalogue.digest(),
            trust,
        },
        reaction: reaction.name.clone(),
        reactants,
        products,
        equation,
        model,
        evidence: resolved_evidence,
        rule: resolved_rule,
    };
    Ok(ExpandedStructuralReaction {
        schema_version: 1,
        claim,
        reactant_instances,
        product_instances,
        atom_provenance,
        mapping,
        mapping_entry_provenance,
        mapping_provenance,
        operations,
        premises,
        premise_provenance,
    })
}

fn resolve_bindings<'a>(
    source_name: &str,
    side: ReactionSideKind,
    bindings: &'a [SourceStructureBinding],
    catalogue: &'a ValidatedCatalogueBundle,
    definitions: &mut BTreeMap<String, &'a StructureDefinition>,
) -> Result<BTreeMap<String, ResolvedStructureBinding>, ExpansionError> {
    let mut result = BTreeMap::new();
    for binding in bindings {
        let coefficient = positive_u32(&binding.coefficient, binding.span, "coefficient")?;
        let structure_id = StructureId::from_str(&binding.structure).map_err(|error| {
            ExpansionError::invalid("CHEMS-X003", error.to_string(), Some(binding.span))
        })?;
        let definition = catalogue.structure(&structure_id).ok_or_else(|| {
            ExpansionError::unsupported(
                "CHEMS-X011",
                format!("unsupported structure `{structure_id}`"),
            )
        })?;
        let structure_premises = catalogue.structure_premises(&structure_id).ok_or_else(|| {
            ExpansionError::system(
                "CHEMS-X091",
                format!("indexed structure `{structure_id}` has no premise closure"),
            )
        })?;
        let formula = formula_map_from_definition(definition);
        let resolved = ResolvedStructureBinding {
            side,
            name: binding.name.clone(),
            coefficient,
            structure: structure_id.clone(),
            formula,
            representation: definition.representation(),
            provenance: Provenance::derived(
                [source_origin(
                    source_name,
                    binding.span,
                    format!("{side:?} binding {}", binding.name),
                )],
                [catalogue_origin(
                    catalogue.digest(),
                    format!("structure {structure_id}"),
                    structure_premises.iter().cloned(),
                )],
                [],
            ),
        };
        if result.insert(binding.name.clone(), resolved).is_some() {
            return Err(ExpansionError::invalid(
                "CHEMS-X003",
                format!("duplicate binding `{}`", binding.name),
                Some(binding.span),
            ));
        }
        definitions.insert(binding.name.clone(), definition);
    }
    Ok(result)
}

fn resolve_equation(
    source_name: &str,
    reaction: &SourceReaction,
    reactants: &BTreeMap<String, ResolvedStructureBinding>,
    products: &BTreeMap<String, ResolvedStructureBinding>,
) -> Result<Vec<ResolvedEquationTerm>, ExpansionError> {
    let equation = reaction.equation.as_ref().ok_or_else(|| {
        ExpansionError::invalid("CHEMS-X005", "equation is missing", Some(reaction.span))
    })?;
    let mut resolved = Vec::new();
    resolve_equation_side(
        source_name,
        ReactionSideKind::Reactant,
        &equation.reactants,
        reactants,
        &mut resolved,
    )?;
    resolve_equation_side(
        source_name,
        ReactionSideKind::Product,
        &equation.products,
        products,
        &mut resolved,
    )?;
    resolved.sort_by(|left, right| (left.side, &left.binding).cmp(&(right.side, &right.binding)));
    Ok(resolved)
}

fn resolve_equation_side(
    source_name: &str,
    side: ReactionSideKind,
    terms: &[SourceEquationTerm],
    bindings: &BTreeMap<String, ResolvedStructureBinding>,
    output: &mut Vec<ResolvedEquationTerm>,
) -> Result<(), ExpansionError> {
    let mut unmatched = bindings.keys().cloned().collect::<BTreeSet<_>>();
    for term in terms {
        let coefficient = term.coefficient.as_deref().map_or(Ok(1), |value| {
            positive_u32(value, term.span, "equation coefficient")
        })?;
        let formula = parse_formula(&term.formula, term.span)?;
        let representation = source_representation(term.representation);
        let candidates = unmatched
            .iter()
            .filter(|name| {
                let binding = &bindings[*name];
                binding.coefficient == coefficient
                    && binding.formula == formula
                    && binding.representation == representation
            })
            .cloned()
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(ExpansionError::invalid(
                "CHEMS-X005",
                format!(
                    "equation term `{}` has {} matching {side:?} declarations",
                    term.formula,
                    candidates.len()
                ),
                Some(term.span),
            ));
        }
        let binding = candidates[0].clone();
        unmatched.remove(&binding);
        let matched = &bindings[&binding];
        output.push(ResolvedEquationTerm {
            side,
            coefficient,
            formula,
            representation,
            binding,
            provenance: Provenance::derived(
                [source_origin(
                    source_name,
                    term.span,
                    format!("{side:?} equation term"),
                )]
                .into_iter()
                .chain(matched.provenance.source.iter().cloned()),
                matched.provenance.catalogue.iter().cloned(),
                [],
            ),
        });
    }
    if !unmatched.is_empty() {
        return Err(ExpansionError::invalid(
            "CHEMS-X005",
            format!(
                "equation omits declarations: {}",
                unmatched.into_iter().collect::<Vec<_>>().join(", ")
            ),
            None,
        ));
    }
    Ok(())
}

struct SelectedRule<'a> {
    record: Cow<'a, ReactionRuleRecord>,
    generalized: Option<ElaboratedGeneralizedRule>,
}

fn select_rule<'a>(
    rule_id: &ReactionRuleId,
    application: &chems_lang::SourceRuleApplication,
    reactants: &BTreeMap<String, ResolvedStructureBinding>,
    products: &BTreeMap<String, ResolvedStructureBinding>,
    catalogue: &'a ValidatedCatalogueBundle,
) -> Result<SelectedRule<'a>, ExpansionError> {
    if let Some(rule) = catalogue.rule(rule_id) {
        return Ok(SelectedRule {
            record: Cow::Borrowed(rule.record()),
            generalized: None,
        });
    }
    if catalogue.generalized_rule(rule_id).is_none() {
        return Err(ExpansionError::unsupported(
            "CHEMS-X010",
            format!("unsupported reaction rule `{rule_id}`"),
        ));
    }
    let inputs = application
        .bindings
        .iter()
        .map(|binding| {
            let (side, resolved) = if let Some(resolved) = reactants.get(&binding.value) {
                (RuleSideRecord::Reactant, resolved)
            } else if let Some(resolved) = products.get(&binding.value) {
                (RuleSideRecord::Product, resolved)
            } else {
                return Err(ExpansionError::invalid(
                    "CHEMS-X012",
                    format!("rule role refers to unknown binding `{}`", binding.value),
                    Some(binding.span),
                ));
            };
            Ok(GeneralizedRoleInput {
                role: binding.role.clone(),
                structure: resolved.structure.clone(),
                coefficient: resolved.coefficient,
                side,
                representation: representation_record(resolved.representation),
            })
        })
        .collect::<Result<Vec<_>, ExpansionError>>()?;
    let selected = catalogue
        .elaborate_generalized_rule(rule_id, &inputs)
        .map_err(|error| ExpansionError::system("CHEMS-X095", error.to_string()))?;
    match selected {
        Ok(generalized) => Ok(SelectedRule {
            record: Cow::Owned(generalized.rule.clone()),
            generalized: Some(generalized),
        }),
        Err(failure) => {
            let message = failure
                .required_feature
                .map_or(failure.message.clone(), |feature| {
                    format!("{} (requires {feature})", failure.message)
                });
            match failure.class {
                GeneralizedElaborationFailureClass::InvalidSource => Err(ExpansionError::invalid(
                    "CHEMS-X013",
                    message,
                    Some(application.span),
                )),
                GeneralizedElaborationFailureClass::Unsupported => {
                    Err(ExpansionError::unsupported("CHEMS-X015", message))
                }
                GeneralizedElaborationFailureClass::Ambiguous => {
                    Err(ExpansionError::ambiguous("CHEMS-X016", message))
                }
            }
        }
    }
}

const fn representation_record(value: RepresentationKind) -> RepresentationRecord {
    match value {
        RepresentationKind::Molecular => RepresentationRecord::Molecular,
        RepresentationKind::Ion => RepresentationRecord::Ion,
        RepresentationKind::Ionic => RepresentationRecord::Ionic,
        RepresentationKind::Metallic => RepresentationRecord::Metallic,
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn resolve_rule(
    source_name: &str,
    application: &chems_lang::SourceRuleApplication,
    rule_id: &ReactionRuleId,
    rule: &ReactionRuleRecord,
    reactants: &BTreeMap<String, ResolvedStructureBinding>,
    products: &BTreeMap<String, ResolvedStructureBinding>,
    catalogue_digest: ContentDigest,
    generalized: Option<&ElaboratedGeneralizedRule>,
) -> Result<ResolvedRuleApplication, ExpansionError> {
    let mut bindings = BTreeMap::new();
    for binding in &application.bindings {
        let schema = rule.roles.get(&binding.role).ok_or_else(|| {
            ExpansionError::invalid(
                "CHEMS-X012",
                format!("rule has no role `{}`", binding.role),
                Some(binding.span),
            )
        })?;
        let (side, resolved) = match schema.side {
            RuleSideRecord::Reactant => (ReactionSideKind::Reactant, reactants.get(&binding.value)),
            RuleSideRecord::Product => (ReactionSideKind::Product, products.get(&binding.value)),
        };
        let resolved = resolved.ok_or_else(|| {
            ExpansionError::invalid(
                "CHEMS-X012",
                format!(
                    "role `{}` refers to wrong-side or unknown binding `{}`",
                    binding.role, binding.value
                ),
                Some(binding.span),
            )
        })?;
        if resolved.representation != catalogue_representation(schema.representation) {
            return Err(ExpansionError::invalid(
                "CHEMS-X012",
                format!("role `{}` representation does not match rule", binding.role),
                Some(binding.span),
            ));
        }
        let pattern = pattern_for_role(rule, &binding.role).ok_or_else(|| {
            ExpansionError::system(
                "CHEMS-X092",
                format!("role `{}` has no pattern", binding.role),
            )
        })?;
        if resolved.structure != pattern.structure_id || resolved.coefficient != pattern.coefficient
        {
            return Err(ExpansionError::invalid(
                "CHEMS-X013",
                format!(
                    "binding `{}` does not satisfy role `{}` pattern",
                    binding.value, binding.role
                ),
                Some(binding.span),
            ));
        }
        let resolved_role = ResolvedRuleBinding {
            role: binding.role.clone(),
            binding: binding.value.clone(),
            side,
            provenance: Provenance::derived(
                [source_origin(
                    source_name,
                    binding.span,
                    format!("rule role {}", binding.role),
                )],
                [catalogue_origin(
                    catalogue_digest,
                    format!("rule {rule_id} role {}", binding.role),
                    [rule.applicability.premise_id.clone()],
                )]
                .into_iter()
                .chain(generalized.and_then(|selected| {
                    selected
                        .role_premise_ids
                        .get(&binding.role)
                        .map(|premises| {
                            catalogue_origin(
                                catalogue_digest,
                                format!("generalized rule {rule_id} role {}", binding.role),
                                premises.iter().cloned(),
                            )
                        })
                })),
                [],
            ),
        };
        if bindings
            .insert(binding.role.clone(), resolved_role)
            .is_some()
        {
            return Err(ExpansionError::invalid(
                "CHEMS-X012",
                format!("duplicate rule role `{}`", binding.role),
                Some(binding.span),
            ));
        }
    }
    let expected_roles = rule.roles.keys().cloned().collect::<BTreeSet<_>>();
    if bindings.keys().cloned().collect::<BTreeSet<_>>() != expected_roles {
        return Err(ExpansionError::invalid(
            "CHEMS-X012",
            "rule role binding is incomplete",
            Some(application.span),
        ));
    }
    let expected_values = reactants
        .keys()
        .chain(products.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let actual_values = bindings
        .values()
        .map(|binding| binding.binding.clone())
        .collect::<BTreeSet<_>>();
    if expected_values != actual_values || actual_values.len() != bindings.len() {
        return Err(ExpansionError::invalid(
            "CHEMS-X012",
            "rule roles must bind every declaration exactly once",
            Some(application.span),
        ));
    }
    Ok(ResolvedRuleApplication {
        rule: rule_id.clone(),
        bindings,
        applicability: ResolvedApplicability {
            request_relation: rule.applicability.request_relation,
            required_context: rule.applicability.required_context.clone(),
            reactant_structures: rule.applicability.reactant_structure_ids.clone(),
            premise: rule.applicability.premise_id.clone(),
            provenance: Provenance::derived(
                [source_origin(
                    source_name,
                    application.span,
                    "rule applicability request",
                )],
                [catalogue_origin(
                    catalogue_digest,
                    format!("rule {rule_id} applicability"),
                    [rule.applicability.premise_id.clone()],
                )]
                .into_iter()
                .chain(generalized.map(|selected| {
                    catalogue_origin(
                        catalogue_digest,
                        format!("generalized rule {rule_id} selected applicability"),
                        selected
                            .selected_premise_ids
                            .iter()
                            .cloned()
                            .chain(selected.role_premise_ids.values().flatten().cloned()),
                    )
                })),
                [],
            ),
        },
        generalized: generalized.map(|selected| ResolvedGeneralizedRuleApplication {
            parameters: selected.parameter_binding.clone(),
            parameter_premises: selected.parameter_premise_ids.clone(),
            case_id: selected.case_id.clone(),
            equivalent_match_count: selected.equivalent_match_count,
            matched_sites: selected.matched_sites.clone(),
            role_premises: selected.role_premise_ids.clone(),
            provenance: Provenance::derived(
                [source_origin(
                    source_name,
                    application.span,
                    "generalized rule parameter and case selection",
                )],
                [catalogue_origin(
                    catalogue_digest,
                    format!("generalized rule {rule_id} case {}", selected.case_id),
                    selected.selected_premise_ids.iter().cloned(),
                )],
                [],
            ),
        }),
        provenance: Provenance::derived(
            [source_origin(
                source_name,
                application.span,
                "rule application",
            )],
            [catalogue_origin(
                catalogue_digest,
                format!("rule {rule_id}"),
                [rule.applicability.premise_id.clone()],
            )]
            .into_iter()
            .chain(generalized.map(|selected| {
                catalogue_origin(
                    catalogue_digest,
                    format!("generalized rule {rule_id} selected certificate"),
                    selected
                        .selected_premise_ids
                        .iter()
                        .cloned()
                        .chain(selected.role_premise_ids.values().flatten().cloned()),
                )
            })),
            [],
        ),
    })
}

fn validate_applicability(
    rule: &ReactionRuleRecord,
    reactants: &BTreeMap<String, ResolvedStructureBinding>,
) -> Result<(), ExpansionError> {
    let actual = reactants
        .values()
        .map(|binding| binding.structure.clone())
        .collect::<BTreeSet<_>>();
    if actual != rule.applicability.reactant_structure_ids {
        return Err(ExpansionError::unsupported(
            "CHEMS-X014",
            format!(
                "rule `{}` is not applicable to the declared reactants",
                rule.id
            ),
        ));
    }
    Ok(())
}

fn resolve_model(
    source_name: &str,
    reaction: &SourceReaction,
    rule: &ReactionRuleRecord,
    catalogue_digest: ContentDigest,
) -> Result<ResolvedModel, ExpansionError> {
    let model = reaction.model.as_ref().ok_or_else(|| {
        ExpansionError::invalid(
            "CHEMS-X006",
            "model declaration is missing",
            Some(reaction.span),
        )
    })?;
    let event = match model.event {
        chems_lang::SourceEventModel::Representative => EventModel::Representative,
    };
    let sequence = match model.sequence {
        chems_lang::SourceSequenceModel::Explanatory => SequenceModel::Explanatory,
    };
    if event != rule.model_assumptions.event || sequence != rule.model_assumptions.sequence {
        return Err(ExpansionError::invalid(
            "CHEMS-X006",
            "source model disclosure does not match the selected rule",
            Some(model.span),
        ));
    }
    Ok(ResolvedModel {
        event,
        sequence,
        provenance: Provenance::derived(
            [source_origin(source_name, model.span, "model disclosure")],
            [catalogue_origin(
                catalogue_digest,
                format!("rule {} model assumptions", rule.id),
                rule.model_assumptions.premise_ids.iter().cloned(),
            )],
            [],
        ),
    })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn resolve_evidence(
    source_name: &str,
    reaction: &SourceReaction,
    evidence: &ValidatedEvidencePacket,
    rule: &ReactionRuleRecord,
    role_bindings: &BTreeMap<String, ResolvedRuleBinding>,
    reactants: &BTreeMap<String, ResolvedStructureBinding>,
    products: &BTreeMap<String, ResolvedStructureBinding>,
    catalogue_digest: ContentDigest,
) -> Result<ResolvedEvidence, ExpansionError> {
    let block = reaction.observations.as_ref().ok_or_else(|| {
        ExpansionError::invalid(
            "CHEMS-X021",
            "observation block is missing",
            Some(reaction.span),
        )
    })?;
    if block.evidence != evidence.reference().name()
        || block.version != evidence.reference().version()
    {
        return Err(ExpansionError::invalid(
            "CHEMS-X021",
            format!(
                "source selects {}@{} but supplied packet is {}",
                block.evidence,
                block.version,
                evidence.reference().qualified()
            ),
            Some(block.span),
        ));
    }
    let mut observations = Vec::new();
    let mut seen_claims = BTreeSet::new();
    for authored in &block.entries {
        let (predicate, subject, value, claim_text, span, expected_side, evidence_role) =
            match authored {
                SourceObservation::GasEvolves { gas, claim, span } => (
                    ObservationPredicate::Evolves,
                    gas,
                    None,
                    claim,
                    *span,
                    ReactionSideKind::Product,
                    "gas",
                ),
                SourceObservation::ReactantDisappears {
                    reactant,
                    claim,
                    span,
                } => (
                    ObservationPredicate::Disappears,
                    reactant,
                    None,
                    claim,
                    *span,
                    ReactionSideKind::Reactant,
                    "reactant",
                ),
                SourceObservation::ProductForms {
                    product,
                    claim,
                    span,
                } => (
                    ObservationPredicate::Forms,
                    product,
                    None,
                    claim,
                    *span,
                    ReactionSideKind::Product,
                    "product",
                ),
                SourceObservation::ProductColour {
                    product,
                    colour,
                    claim,
                    span,
                } => (
                    ObservationPredicate::Colour,
                    product,
                    Some(colour.clone()),
                    claim,
                    *span,
                    ReactionSideKind::Product,
                    "product",
                ),
            };
        let subject_binding = reactants
            .get(subject)
            .or_else(|| products.get(subject))
            .ok_or_else(|| {
                ExpansionError::invalid(
                    "CHEMS-X022",
                    format!("observation subject `{subject}` is not declared"),
                    Some(span),
                )
            })?;
        if subject_binding.side != expected_side {
            return Err(ExpansionError::invalid(
                "CHEMS-X022",
                format!("observation subject `{subject}` is on the wrong side"),
                Some(span),
            ));
        }
        let claim_id = ClaimId::from_str(claim_text).map_err(|error| {
            ExpansionError::invalid("CHEMS-X022", error.to_string(), Some(span))
        })?;
        if !seen_claims.insert(claim_id.clone()) {
            return Err(ExpansionError::invalid(
                "CHEMS-X022",
                format!("claim `{claim_id}` is referenced more than once"),
                Some(span),
            ));
        }
        let claim = evidence.claim(&claim_id).ok_or_else(|| {
            ExpansionError::invalid(
                "CHEMS-X023",
                format!("evidence packet has no claim `{claim_id}`"),
                Some(span),
            )
        })?;
        let compatibility = rule
            .observation_compatibility
            .iter()
            .find(|compatibility| {
                compatibility.predicate == predicate
                    && compatibility.value == value
                    && role_bindings
                        .get(&compatibility.subject_role)
                        .is_some_and(|binding| binding.binding == *subject)
            })
            .ok_or_else(|| {
                ExpansionError::unsupported(
                    "CHEMS-X024",
                    format!(
                        "rule `{}` does not support the authored observation",
                        rule.id
                    ),
                )
            })?;
        if claim.predicate != evidence_predicate(predicate)
            || claim.subject != compatibility.evidence_subject
            || claim.subject_role != evidence_role
        {
            return Err(ExpansionError::invalid(
                "CHEMS-X023",
                format!("claim `{claim_id}` does not match the authored observation and rule fact"),
                Some(span),
            ));
        }
        let evidence_origin = EvidenceOrigin {
            packet: evidence.reference().qualified(),
            packet_digest: evidence.digest(),
            claim: claim_id.clone(),
            sources: claim.sources.iter().cloned().collect(),
        };
        observations.push(ResolvedObservation {
            predicate,
            subject_binding: subject.clone(),
            value,
            claim: claim_id,
            evidence_subject: claim.subject.clone(),
            provenance: Provenance::derived(
                [source_origin(
                    source_name,
                    span,
                    format!("observation {subject}"),
                )],
                [catalogue_origin(
                    catalogue_digest,
                    format!("rule {} observation compatibility", rule.id),
                    [compatibility.premise_id.clone()],
                )],
                [evidence_origin],
            ),
        });
    }
    observations.sort_by(|left, right| {
        (&left.claim, left.predicate, &left.subject_binding).cmp(&(
            &right.claim,
            right.predicate,
            &right.subject_binding,
        ))
    });
    Ok(ResolvedEvidence {
        packet: evidence.reference().clone(),
        digest: evidence.digest(),
        trust: EvidenceTrust::ExternalUntrusted,
        observations,
    })
}

fn expand_instances(
    bindings: &BTreeMap<String, ResolvedStructureBinding>,
    definitions: &BTreeMap<String, &StructureDefinition>,
    catalogue_digest: ContentDigest,
) -> Result<BTreeMap<String, ExpandedInstance>, ExpansionError> {
    let mut result = BTreeMap::new();
    for binding in bindings.values() {
        let definition = definitions.get(&binding.name).ok_or_else(|| {
            ExpansionError::system(
                "CHEMS-X093",
                format!("binding `{}` lost its structure definition", binding.name),
            )
        })?;
        for ordinal in 1..=binding.coefficient {
            let instance_text = format!("{}[{ordinal}]", binding.name);
            let instance_id = StructureInstanceId::from_str(&instance_text).map_err(system_id)?;
            let relabeling = definition
                .graph()
                .atoms()
                .keys()
                .map(|atom| {
                    let expanded = AtomId::from_str(&format!("{instance_text}.{}", atom.as_str()))
                        .map_err(system_id)?;
                    Ok((atom.clone(), expanded))
                })
                .collect::<Result<Vec<_>, ExpansionError>>()?;
            let instance = StructureInstance::instantiate(instance_id, definition, relabeling)
                .map_err(system_structural)?;
            let expanded = ExpandedInstance {
                binding: binding.name.clone(),
                ordinal,
                instance,
                provenance: Provenance::derived(
                    binding.provenance.source.iter().cloned(),
                    binding
                        .provenance
                        .catalogue
                        .iter()
                        .cloned()
                        .chain([catalogue_origin(
                            catalogue_digest,
                            format!("coefficient instance {instance_text}"),
                            binding
                                .provenance
                                .catalogue
                                .iter()
                                .flat_map(|origin| origin.premises.iter().cloned()),
                        )]),
                    [],
                ),
            };
            result.insert(instance_text, expanded);
        }
    }
    Ok(result)
}

fn expand_mapping(
    reaction: &SourceReaction,
    rule: &ReactionRuleRecord,
    role_bindings: &BTreeMap<String, ResolvedRuleBinding>,
    reactants: &ReactionSide,
    products: &ReactionSide,
    catalogue_digest: ContentDigest,
) -> Result<(AtomMapping, BTreeMap<AtomId, CatalogueOrigin>), ExpansionError> {
    let mut entries = Vec::new();
    let mut provenance = BTreeMap::new();
    for (index, entry) in rule.mapping_template.iter().enumerate() {
        let reactant = AtomId::from_str(&expand_template_atom(&entry.reactant, role_bindings)?)
            .map_err(system_id)?;
        let product = AtomId::from_str(&expand_template_atom(&entry.product, role_bindings)?)
            .map_err(system_id)?;
        provenance.insert(
            reactant.clone(),
            catalogue_origin(
                catalogue_digest,
                format!("rule {} mapping pair {}", rule.id, index + 1),
                entry.premise_ids.iter().cloned(),
            ),
        );
        entries.push((reactant, product));
    }
    let mapping = AtomMapping::new(
        AtomMappingId::from_str(&format!("mapping.{}", reaction.name)).map_err(system_id)?,
        entries,
        reactants,
        products,
    )
    .map_err(system_structural)?;
    Ok((mapping, provenance))
}

fn expand_operations(
    source_name: &str,
    reaction: &SourceReaction,
    rule: &ReactionRuleRecord,
    role_bindings: &BTreeMap<String, ResolvedRuleBinding>,
    catalogue_digest: ContentDigest,
) -> Result<Vec<ExpandedOperation>, ExpansionError> {
    rule.operation_template
        .iter()
        .enumerate()
        .map(|(index, template)| {
            let ordinal = u32::try_from(index + 1)
                .map_err(|_| ExpansionError::system("CHEMS-X094", "operation count exceeds u32"))?;
            let id = StructuralOperationId::from_str(&format!("operation[{ordinal}]"))
                .map_err(system_id)?;
            let (input, ionic_components) =
                expand_operation_input(ordinal, template, role_bindings)?;
            let electron_contribution = match template {
                OperationTemplateRecord::FormCovalent {
                    electron_contribution,
                    ..
                } => Some(ExpandedElectronContribution {
                    left: electron_contribution.left,
                    right: electron_contribution.right,
                }),
                _ => None,
            };
            let operation = StructuralOperation::new(id, input).map_err(system_structural)?;
            let source_origins = role_bindings
                .values()
                .flat_map(|binding| binding.provenance.source.iter().cloned())
                .chain([source_origin(
                    source_name,
                    reaction
                        .rule_application
                        .as_ref()
                        .map_or(reaction.span, |application| application.span),
                    format!("operation template {ordinal}"),
                )]);
            Ok(ExpandedOperation {
                ordinal,
                operation,
                electron_contribution,
                ionic_components,
                provenance: Provenance::derived(
                    source_origins,
                    [catalogue_origin(
                        catalogue_digest,
                        format!("rule {} operation template {ordinal}", rule.id),
                        template.premise_ids().iter().cloned(),
                    )],
                    [],
                ),
            })
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn expand_operation_input(
    ordinal: u32,
    template: &OperationTemplateRecord,
    bindings: &BTreeMap<String, ResolvedRuleBinding>,
) -> Result<(StructuralOperationInput, Vec<ExpandedIonicComponent>), ExpansionError> {
    let none = Vec::new();
    match template {
        OperationTemplateRecord::ReconfigureElectrons {
            atom,
            before,
            after,
            ..
        } => {
            let atom = atom_ref(atom, bindings)?;
            Ok((
                StructuralOperationInput::ReconfigureElectrons {
                    transition: ElectronTransition::new(
                        atom,
                        electron_state(*before)?,
                        electron_state(*after)?,
                    ),
                },
                none,
            ))
        }
        OperationTemplateRecord::CleaveCovalent {
            edge,
            allocation,
            before,
            after,
            ..
        } => {
            let left = atom_ref(&edge.0, bindings)?;
            let right = atom_ref(&edge.1, bindings)?;
            Ok((
                StructuralOperationInput::CleaveCovalent {
                    left: left.clone(),
                    right: right.clone(),
                    expected_order: bond_order(edge.2),
                    allocation: electron_allocation(allocation, bindings)?,
                    transitions: binary_transitions(&left, &right, before, after)?,
                },
                none,
            ))
        }
        OperationTemplateRecord::FormCovalent {
            edge,
            before,
            after,
            ..
        } => {
            let left = atom_ref(&edge.0, bindings)?;
            let right = atom_ref(&edge.1, bindings)?;
            Ok((
                StructuralOperationInput::FormCovalent {
                    left: left.clone(),
                    right: right.clone(),
                    order: bond_order(edge.2),
                    transitions: binary_transitions(&left, &right, before, after)?,
                },
                none,
            ))
        }
        OperationTemplateRecord::CleaveDative {
            donor,
            acceptor,
            allocation,
            before,
            after,
            ..
        } => {
            let donor = atom_ref(donor, bindings)?;
            let acceptor = atom_ref(acceptor, bindings)?;
            Ok((
                StructuralOperationInput::CleaveDative {
                    donor: donor.clone(),
                    acceptor: acceptor.clone(),
                    allocation: electron_allocation(allocation, bindings)?,
                    transitions: binary_transitions(&donor, &acceptor, before, after)?,
                },
                none,
            ))
        }
        OperationTemplateRecord::FormDative {
            donor,
            acceptor,
            before,
            after,
            ..
        } => {
            let donor = atom_ref(donor, bindings)?;
            let acceptor = atom_ref(acceptor, bindings)?;
            Ok((
                StructuralOperationInput::FormDative {
                    donor: donor.clone(),
                    acceptor: acceptor.clone(),
                    transitions: binary_transitions(&donor, &acceptor, before, after)?,
                },
                none,
            ))
        }
        OperationTemplateRecord::ChangeCovalent {
            edge,
            old_order,
            new_order,
            allocation,
            before,
            after,
            ..
        } => {
            let left = atom_ref(&edge.0, bindings)?;
            let right = atom_ref(&edge.1, bindings)?;
            Ok((
                StructuralOperationInput::ChangeCovalent {
                    left: left.clone(),
                    right: right.clone(),
                    old_order: bond_order(*old_order),
                    new_order: bond_order(*new_order),
                    allocation: electron_allocation(allocation, bindings)?,
                    transitions: binary_transitions(&left, &right, before, after)?,
                },
                none,
            ))
        }
        OperationTemplateRecord::ChangeCovalentDelocalization {
            edge,
            expected,
            replacement,
            ..
        } => {
            let left = atom_ref(&edge.0, bindings)?;
            let right = atom_ref(&edge.1, bindings)?;
            Ok((
                StructuralOperationInput::ChangeCovalentDelocalization {
                    left,
                    right,
                    expected: expected.as_ref().map(delocalization).transpose()?,
                    replacement: replacement.as_ref().map(delocalization).transpose()?,
                },
                none,
            ))
        }
        OperationTemplateRecord::AssociateIonic {
            label,
            components,
            component_charges,
            ..
        } => {
            let mut expanded = Vec::new();
            let mut group_ids = Vec::new();
            for (index, (atoms, charge)) in components.iter().zip(component_charges).enumerate() {
                let group_id =
                    AtomGroupId::from_str(&format!("ionic[{ordinal}].component[{}]", index + 1))
                        .map_err(system_id)?;
                let group = AtomGroup::new(
                    group_id.clone(),
                    atoms
                        .iter()
                        .map(|atom| atom_ref(atom, bindings))
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(system_structural)?;
                group_ids.push(group_id);
                expanded.push(ExpandedIonicComponent {
                    group,
                    expected_charge: *charge,
                });
            }
            let association = IonicAssociation::new(
                IonicAssociationId::from_str(&format!("ionic[{ordinal}].{label}"))
                    .map_err(system_id)?,
                group_ids,
            )
            .map_err(system_structural)?;
            Ok((
                StructuralOperationInput::AssociateIonic { association },
                expanded,
            ))
        }
        OperationTemplateRecord::DissociateIonic { association, .. } => Ok((
            StructuralOperationInput::DissociateIonic {
                association: IonicAssociationId::from_str(&expand_template_reference(
                    association,
                    bindings,
                )?)
                .map_err(system_id)?,
            },
            none,
        )),
        OperationTemplateRecord::ReleaseMetallic {
            site,
            domain,
            allocation,
            before,
            after,
            ..
        } => {
            let site = atom_ref(site, bindings)?;
            Ok((
                StructuralOperationInput::ReleaseMetallic {
                    site: site.clone(),
                    domain: MetallicDomainId::from_str(&expand_template_reference(
                        domain, bindings,
                    )?)
                    .map_err(system_id)?,
                    allocation: match allocation {
                        MetallicReleaseAllocationRecord::RetainElectron => {
                            MetallicReleaseAllocation::RetainElectron
                        }
                        MetallicReleaseAllocationRecord::LeaveElectron => {
                            MetallicReleaseAllocation::LeaveElectron
                        }
                    },
                    transition: ElectronTransition::new(
                        site,
                        electron_state(before.site)?,
                        electron_state(after.site)?,
                    ),
                    domain_electrons_before: before.domain_electrons,
                    domain_electrons_after: after.domain_electrons,
                },
                none,
            ))
        }
        OperationTemplateRecord::JoinMetallic {
            site,
            domain,
            allocation,
            before,
            after,
            ..
        } => {
            let site = atom_ref(site, bindings)?;
            Ok((
                StructuralOperationInput::JoinMetallic {
                    site: site.clone(),
                    domain: MetallicDomainId::from_str(&expand_template_reference(
                        domain, bindings,
                    )?)
                    .map_err(system_id)?,
                    allocation: match allocation {
                        MetallicJoinAllocationRecord::DonateElectron => {
                            MetallicJoinAllocation::DonateElectron
                        }
                    },
                    transition: ElectronTransition::new(
                        site,
                        electron_state(before.site)?,
                        electron_state(after.site)?,
                    ),
                    domain_electrons_before: before.domain_electrons,
                    domain_electrons_after: after.domain_electrons,
                },
                none,
            ))
        }
        OperationTemplateRecord::TransferElectron {
            count,
            donor,
            acceptor,
            before,
            after,
            ..
        } => {
            let donor = atom_ref(donor, bindings)?;
            let acceptor = atom_ref(acceptor, bindings)?;
            Ok((
                StructuralOperationInput::TransferElectron {
                    donor: donor.clone(),
                    acceptor: acceptor.clone(),
                    count: *count,
                    transitions: transfer_transitions(&donor, &acceptor, before, after)?,
                },
                none,
            ))
        }
        OperationTemplateRecord::AssignProduct { atoms, product, .. } => Ok((
            StructuralOperationInput::AssignProduct {
                atoms: atoms
                    .iter()
                    .map(|atom| atom_ref(atom, bindings))
                    .collect::<Result<Vec<_>, _>>()?,
                product: StructureInstanceId::from_str(&expand_instance_reference(
                    product, bindings,
                )?)
                .map_err(system_id)?,
            },
            none,
        )),
    }
}

fn binary_transitions(
    left: &AtomId,
    right: &AtomId,
    before: &BinaryElectronStateRecord,
    after: &BinaryElectronStateRecord,
) -> Result<Vec<ElectronTransition>, ExpansionError> {
    Ok(vec![
        ElectronTransition::new(
            left.clone(),
            electron_state(before.left)?,
            electron_state(after.left)?,
        ),
        ElectronTransition::new(
            right.clone(),
            electron_state(before.right)?,
            electron_state(after.right)?,
        ),
    ])
}

fn transfer_transitions(
    donor: &AtomId,
    acceptor: &AtomId,
    before: &TransferElectronStateRecord,
    after: &TransferElectronStateRecord,
) -> Result<Vec<ElectronTransition>, ExpansionError> {
    Ok(vec![
        ElectronTransition::new(
            donor.clone(),
            electron_state(before.donor)?,
            electron_state(after.donor)?,
        ),
        ElectronTransition::new(
            acceptor.clone(),
            electron_state(before.acceptor)?,
            electron_state(after.acceptor)?,
        ),
    ])
}

fn electron_state(state: ElectronStateRecord) -> Result<ElectronState, ExpansionError> {
    ElectronState::new(state.0, state.1, state.2).map_err(system_structural)
}

fn electron_allocation(
    allocation: &CleavageAllocationRecord,
    bindings: &BTreeMap<String, ResolvedRuleBinding>,
) -> Result<ElectronAllocation, ExpansionError> {
    match allocation {
        CleavageAllocationRecord::Homolytic(value) if value == "homolytic" => {
            Ok(ElectronAllocation::Homolytic)
        }
        CleavageAllocationRecord::Heterolytic { heterolytic_to } => Ok(
            ElectronAllocation::HeterolyticTo(atom_ref(heterolytic_to, bindings)?),
        ),
        CleavageAllocationRecord::Homolytic(value) => Err(ExpansionError::system(
            "CHEMS-X095",
            format!("invalid validated allocation `{value}`"),
        )),
    }
}

fn atom_ref(
    reference: &str,
    bindings: &BTreeMap<String, ResolvedRuleBinding>,
) -> Result<AtomId, ExpansionError> {
    AtomId::from_str(&expand_template_atom(reference, bindings)?).map_err(system_id)
}

fn expand_template_atom(
    reference: &str,
    bindings: &BTreeMap<String, ResolvedRuleBinding>,
) -> Result<String, ExpansionError> {
    let expanded = expand_template_reference(reference, bindings)?;
    if !expanded.contains('.') {
        return Err(ExpansionError::system(
            "CHEMS-X096",
            format!("template atom `{reference}` lacks a local path"),
        ));
    }
    Ok(expanded)
}

fn expand_template_reference(
    reference: &str,
    bindings: &BTreeMap<String, ResolvedRuleBinding>,
) -> Result<String, ExpansionError> {
    let (instance, suffix) = reference.split_once('.').ok_or_else(|| {
        ExpansionError::system(
            "CHEMS-X096",
            format!("malformed template reference `{reference}`"),
        )
    })?;
    Ok(format!(
        "{}.{}",
        expand_instance_reference(instance, bindings)?,
        suffix
    ))
}

fn expand_instance_reference(
    reference: &str,
    bindings: &BTreeMap<String, ResolvedRuleBinding>,
) -> Result<String, ExpansionError> {
    let open = reference.find('[').ok_or_else(|| {
        ExpansionError::system(
            "CHEMS-X096",
            format!("malformed template instance `{reference}`"),
        )
    })?;
    let role = &reference[..open];
    let ordinal = reference
        .get(open..)
        .ok_or_else(|| ExpansionError::system("CHEMS-X096", "invalid template instance"))?;
    let binding = bindings.get(role).ok_or_else(|| {
        ExpansionError::system("CHEMS-X096", format!("template role `{role}` is unbound"))
    })?;
    Ok(format!("{}{ordinal}", binding.binding))
}

fn semantic_source_digest(ast: &SourceAst) -> Result<ContentDigest, ExpansionError> {
    let mut normalized = ast.clone();
    normalized.production_trace.clear();
    normalized.comments.clear();
    if let Some(reaction) = &mut normalized.reaction {
        reaction
            .reactants
            .sort_by(|left, right| left.name.cmp(&right.name));
        reaction
            .products
            .sort_by(|left, right| left.name.cmp(&right.name));
        if let Some(equation) = &mut reaction.equation {
            equation.reactants.sort_by(equation_term_order);
            equation.products.sort_by(equation_term_order);
        }
        if let Some(observations) = &mut reaction.observations {
            observations.entries.sort_by_key(observation_order);
        }
        if let Some(application) = &mut reaction.rule_application {
            application
                .bindings
                .sort_by(|left, right| left.role.cmp(&right.role));
        }
    }
    let mut value = serde_json::to_value(normalized)
        .map_err(|error| ExpansionError::system("CHEMS-X090", error.to_string()))?;
    strip_spans(&mut value);
    let bytes = chem_domain::canonical_json(&value)
        .map_err(|error| ExpansionError::system("CHEMS-X090", error.to_string()))?;
    Ok(ContentDigest::sha256(&bytes))
}

fn equation_term_order(
    left: &SourceEquationTerm,
    right: &SourceEquationTerm,
) -> std::cmp::Ordering {
    (
        &left.formula,
        source_representation_rank(left.representation),
        &left.coefficient,
    )
        .cmp(&(
            &right.formula,
            source_representation_rank(right.representation),
            &right.coefficient,
        ))
}

const fn source_representation_rank(value: SourceRepresentationKind) -> u8 {
    match value {
        SourceRepresentationKind::Molecular => 0,
        SourceRepresentationKind::Ion => 1,
        SourceRepresentationKind::Ionic => 2,
        SourceRepresentationKind::Metallic => 3,
    }
}

fn observation_order(observation: &SourceObservation) -> (String, String, String) {
    match observation {
        SourceObservation::GasEvolves { gas, claim, .. } => {
            (claim.clone(), "evolves".to_owned(), gas.clone())
        }
        SourceObservation::ReactantDisappears {
            reactant, claim, ..
        } => (claim.clone(), "disappears".to_owned(), reactant.clone()),
        SourceObservation::ProductForms { product, claim, .. } => {
            (claim.clone(), "forms".to_owned(), product.clone())
        }
        SourceObservation::ProductColour { product, claim, .. } => {
            (claim.clone(), "colour".to_owned(), product.clone())
        }
    }
}

fn strip_spans(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("span");
            for child in object.values_mut() {
                strip_spans(child);
            }
        }
        Value::Array(array) => {
            for child in array {
                strip_spans(child);
            }
        }
        _ => {}
    }
}

fn parse_formula(source: &str, span: ByteSpan) -> Result<BTreeMap<String, u64>, ExpansionError> {
    FormulaParser::new(source, span).parse()
}

struct FormulaParser<'a> {
    source: &'a str,
    bytes: &'a [u8],
    index: usize,
    span: ByteSpan,
}

impl<'a> FormulaParser<'a> {
    fn new(source: &'a str, span: ByteSpan) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            index: 0,
            span,
        }
    }

    fn parse(mut self) -> Result<BTreeMap<String, u64>, ExpansionError> {
        let mut result = self.segment(None)?;
        while self.index < self.bytes.len() {
            self.expect(b'.')?;
            let multiplier = self.number()?.unwrap_or(1);
            let segment = self.segment(None)?;
            merge_formula(&mut result, segment, multiplier, self.span)?;
        }
        if result.is_empty() {
            return self.error("formula is empty");
        }
        Ok(result)
    }

    fn segment(&mut self, terminator: Option<u8>) -> Result<BTreeMap<String, u64>, ExpansionError> {
        let mut result = BTreeMap::new();
        let start = self.index;
        while self.index < self.bytes.len()
            && self.bytes[self.index] != b'.'
            && Some(self.bytes[self.index]) != terminator
        {
            if self.bytes[self.index] == b'(' {
                self.index += 1;
                let nested = self.segment(Some(b')'))?;
                self.expect(b')')?;
                let multiplier = self.number()?.unwrap_or(1);
                merge_formula(&mut result, nested, multiplier, self.span)?;
            } else if self.bytes[self.index].is_ascii_uppercase() {
                let element_start = self.index;
                self.index += 1;
                if self
                    .bytes
                    .get(self.index)
                    .is_some_and(u8::is_ascii_lowercase)
                {
                    self.index += 1;
                }
                let element = &self.source[element_start..self.index];
                ElementSymbol::from_str(element).map_err(|error| {
                    ExpansionError::invalid("CHEMS-X005", error.to_string(), Some(self.span))
                })?;
                let count = self.number()?.unwrap_or(1);
                let entry = result.entry(element.to_owned()).or_insert(0_u64);
                *entry = entry.checked_add(count).ok_or_else(|| {
                    ExpansionError::invalid("CHEMS-X005", "formula count overflow", Some(self.span))
                })?;
            } else {
                return self.error("invalid formula token");
            }
        }
        if self.index == start {
            return self.error("empty formula segment");
        }
        Ok(result)
    }

    fn number(&mut self) -> Result<Option<u64>, ExpansionError> {
        let start = self.index;
        while self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
            self.index += 1;
        }
        if start == self.index {
            return Ok(None);
        }
        let value = self.source[start..self.index].parse::<u64>().map_err(|_| {
            ExpansionError::invalid("CHEMS-X005", "formula count overflow", Some(self.span))
        })?;
        if value == 0 {
            return self.error("formula count must be positive");
        }
        Ok(Some(value))
    }

    fn expect(&mut self, byte: u8) -> Result<(), ExpansionError> {
        if self.bytes.get(self.index) == Some(&byte) {
            self.index += 1;
            Ok(())
        } else {
            self.error("malformed formula grouping")
        }
    }

    fn error<T>(&self, message: &str) -> Result<T, ExpansionError> {
        Err(ExpansionError::invalid(
            "CHEMS-X005",
            format!("{message} in `{}`", self.source),
            Some(self.span),
        ))
    }
}

fn merge_formula(
    target: &mut BTreeMap<String, u64>,
    source: BTreeMap<String, u64>,
    multiplier: u64,
    span: ByteSpan,
) -> Result<(), ExpansionError> {
    for (element, count) in source {
        let count = count.checked_mul(multiplier).ok_or_else(|| {
            ExpansionError::invalid("CHEMS-X005", "formula count overflow", Some(span))
        })?;
        let entry = target.entry(element).or_insert(0);
        *entry = entry.checked_add(count).ok_or_else(|| {
            ExpansionError::invalid("CHEMS-X005", "formula count overflow", Some(span))
        })?;
    }
    Ok(())
}

fn positive_u32(value: &str, span: ByteSpan, label: &str) -> Result<u32, ExpansionError> {
    let parsed = value.parse::<u32>().map_err(|_| {
        ExpansionError::invalid("CHEMS-X003", format!("{label} exceeds u32"), Some(span))
    })?;
    if parsed == 0 {
        Err(ExpansionError::invalid(
            "CHEMS-X003",
            format!("{label} must be positive"),
            Some(span),
        ))
    } else {
        Ok(parsed)
    }
}

fn formula_map_from_definition(definition: &StructureDefinition) -> BTreeMap<String, u64> {
    definition
        .formula()
        .elements()
        .iter()
        .map(|(element, count)| (element.as_str().to_owned(), *count))
        .collect()
}

fn source_representation(value: SourceRepresentationKind) -> RepresentationKind {
    match value {
        SourceRepresentationKind::Molecular => RepresentationKind::Molecular,
        SourceRepresentationKind::Ion => RepresentationKind::Ion,
        SourceRepresentationKind::Ionic => RepresentationKind::Ionic,
        SourceRepresentationKind::Metallic => RepresentationKind::Metallic,
    }
}

fn catalogue_representation(value: RepresentationRecord) -> RepresentationKind {
    match value {
        RepresentationRecord::Molecular => RepresentationKind::Molecular,
        RepresentationRecord::Ion => RepresentationKind::Ion,
        RepresentationRecord::Ionic => RepresentationKind::Ionic,
        RepresentationRecord::Metallic => RepresentationKind::Metallic,
    }
}

fn bond_order(value: BondOrderRecord) -> BondOrder {
    match value {
        BondOrderRecord::Single => BondOrder::Single,
        BondOrderRecord::Double => BondOrder::Double,
        BondOrderRecord::Triple => BondOrder::Triple,
    }
}

fn delocalization(
    value: &BondDelocalizationRecord,
) -> Result<CovalentDelocalization, ExpansionError> {
    let domain = CovalentDelocalizationId::from_str(&value.domain).map_err(system_id)?;
    let effective_order = EffectiveBondOrder::new(
        value.effective_order.numerator,
        value.effective_order.denominator,
    )
    .map_err(system_structural)?;
    Ok(CovalentDelocalization::new(domain, effective_order))
}

fn pattern_for_role<'a>(
    rule: &'a ReactionRuleRecord,
    role_name: &str,
) -> Option<&'a chem_catalogue::PatternTermRecord> {
    rule.reactant_pattern
        .iter()
        .chain(&rule.product_pattern)
        .find(|term| term.role == role_name)
}

fn evidence_predicate(value: ObservationPredicate) -> EvidencePredicate {
    match value {
        ObservationPredicate::Evolves => EvidencePredicate::Evolves,
        ObservationPredicate::Disappears => EvidencePredicate::Disappears,
        ObservationPredicate::Forms => EvidencePredicate::Forms,
        ObservationPredicate::Colour => EvidencePredicate::Colour,
    }
}

fn source_origin(source: &str, span: ByteSpan, construct: impl Into<String>) -> SourceOrigin {
    SourceOrigin {
        source: source.to_owned(),
        construct: construct.into(),
        span,
    }
}

fn catalogue_origin(
    digest: ContentDigest,
    record: impl Into<String>,
    premises: impl IntoIterator<Item = PremiseId>,
) -> CatalogueOrigin {
    CatalogueOrigin {
        catalogue_digest: digest,
        record: record.into(),
        premises: premises.into_iter().collect(),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn system_structural(error: chem_domain::StructuralError) -> ExpansionError {
    ExpansionError::system("CHEMS-X097", error.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn system_id(error: chem_domain::IdError) -> ExpansionError {
    ExpansionError::system("CHEMS-X098", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_resolution_handles_groups_and_adduct_multipliers() {
        assert_eq!(
            parse_formula("Ca(OH)2", ByteSpan::new(0, 7)).unwrap(),
            BTreeMap::from([
                ("Ca".to_owned(), 1),
                ("H".to_owned(), 2),
                ("O".to_owned(), 2),
            ])
        );
        assert_eq!(
            parse_formula("CuSO4.5H2O", ByteSpan::new(0, 11)).unwrap(),
            BTreeMap::from([
                ("Cu".to_owned(), 1),
                ("H".to_owned(), 10),
                ("O".to_owned(), 9),
                ("S".to_owned(), 1),
            ])
        );
    }
}
