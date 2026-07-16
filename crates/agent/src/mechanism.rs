use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use chem_catalogue::{
    ApplicabilityRecord, AtomRecord, BondDelocalizationRecord, BondElectronOriginRecord,
    BondOrderRecord, BondRecord, CatalogueEnvelope, CleavageAllocationRecord, ComponentRecord,
    ElementValenceRecord, EventModel, GroupRecord, IonicAssociationRecord, MappingPairRecord,
    MetallicDomainRecord, MetallicValenceStateRecord, ModelAssumptionsRecord,
    OperationTemplateRecord, PatternTermRecord, PremiseRecord, PublicationKind, ReactionRuleRecord,
    RepresentationRecord, RequestRelation, ReviewMetadata, ReviewStatus, RoleSchemaRecord,
    RuleSideRecord, SequenceModel, ValencePremiseRecord, ValenceStateRecord,
    ValidatedCatalogueBundle,
};
use chem_domain::{
    ContentDigest, CovalentElectronOrigin, ElectronState, PremiseId, ReactionRuleId,
    RepresentationKind, SpeciesId, StructureDefinition, StructureId,
};
use chem_kernel::{
    ValidatedDynamicFrames, expand_proposed_declaration, inspect_review_candidate_frames,
    validate_review_candidate,
};

use crate::{
    AgentError, LabelledStructure, MechanismCleavageAllocation, MechanismEscalationRequest,
    MechanismEscalationResponse, MechanismHomolytic, MechanismOperation, MechanismSpecies,
    OutcomeSpecies, StructureProposalRequest, StructureProposalResponse, ValidatedStaticOutcome,
    adopt_proposed_structures, structure_proposal_request,
};

const MAX_MECHANISM_REPAIRS: usize = 2;
const MAX_STRUCTURE_REPAIRS: usize = 2;
const DYNAMIC_MECHANISM_VALENCE_PREMISE: &str = "premise.dynamic.mechanism.valence";

#[derive(Debug, Clone)]
pub struct MechanismContext {
    request: MechanismEscalationRequest,
    roles: BTreeMap<String, MechanismRole>,
    reactant_atoms: BTreeMap<String, String>,
    product_atoms: BTreeMap<String, String>,
    reactant_domains: BTreeSet<String>,
    reactant_associations: BTreeSet<String>,
    product_instances: BTreeSet<String>,
}

impl MechanismContext {
    #[must_use]
    pub const fn request(&self) -> &MechanismEscalationRequest {
        &self.request
    }
}

#[derive(Debug, Clone)]
struct MechanismRole {
    species: SpeciesId,
    structure: StructureId,
    coefficient: u32,
    side: RuleSideRecord,
    representation: RepresentationRecord,
}

#[derive(Debug, Clone)]
pub struct EscalatedMechanismOutcome {
    static_outcome: ValidatedStaticOutcome,
    frames: ValidatedDynamicFrames,
    repair_count: usize,
    structure_repair_count: usize,
}

impl EscalatedMechanismOutcome {
    #[must_use]
    pub const fn static_outcome(&self) -> &ValidatedStaticOutcome {
        &self.static_outcome
    }

    #[must_use]
    pub const fn frames(&self) -> &ValidatedDynamicFrames {
        &self.frames
    }

    #[must_use]
    pub const fn repair_count(&self) -> usize {
        self.repair_count
    }

    #[must_use]
    pub const fn structure_repair_count(&self) -> usize {
        self.structure_repair_count
    }

    #[must_use]
    pub const fn total_repair_count(&self) -> usize {
        self.repair_count + self.structure_repair_count
    }

    #[must_use]
    pub const fn disclosure(&self) -> &'static str {
        "Model-proposed representative sequence; structurally validated by ChemSpec"
    }
}

#[derive(Debug, Clone)]
pub enum MechanismEscalationOutcome {
    Animated(Box<EscalatedMechanismOutcome>),
    Unavailable {
        static_outcome: Box<ValidatedStaticOutcome>,
        attempts: usize,
        diagnostic: String,
        retryable: bool,
    },
}

pub trait MechanismProvider {
    /// Returns a complete response for the fixed labelled request. `diagnostic`
    /// is present only for an operation-level repair and cannot alter inputs.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot produce or decode a response.
    fn propose(
        &mut self,
        request: &MechanismEscalationRequest,
        diagnostic: Option<&str>,
    ) -> Result<MechanismEscalationResponse, AgentError>;

    /// Returns one structural graph per requested species. `diagnostic` is
    /// present only for a proposal-level repair and cannot alter the request.
    /// The default declines, so a provider must opt in to structure
    /// escalation explicitly.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot produce or decode a response.
    fn propose_structures(
        &mut self,
        request: &StructureProposalRequest,
        diagnostic: Option<&str>,
    ) -> Result<StructureProposalResponse, AgentError> {
        let _ = (request, diagnostic);
        Err(AgentError::new(
            "structure proposal",
            "provider does not support structure proposals",
        ))
    }
}

/// Compiles a mechanism request only when every declaration species has a
/// validated structural graph. Formula-only species return `Ok(None)` and
/// remain static; no graph is fabricated.
///
/// # Errors
///
/// Returns an error for inconsistent outcome/declaration identities or a
/// structure that cannot be projected into the closed wire representation.
#[allow(clippy::too_many_lines)]
pub fn compile_mechanism_request(
    outcome: &ValidatedStaticOutcome,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<Option<MechanismContext>, AgentError> {
    if !outcome.species_without_structure().is_empty() {
        return Ok(None);
    }
    let mut resolved = BTreeMap::new();
    for species in outcome.reactants() {
        let OutcomeSpecies::Resolved(species) = species else {
            return Ok(None);
        };
        resolved.insert(species.id.clone(), species.as_ref());
    }
    for species in outcome.products() {
        let OutcomeSpecies::Resolved(species) = species else {
            return Ok(None);
        };
        resolved.insert(species.id.clone(), species.as_ref());
    }
    let mut roles = BTreeMap::new();
    let mut reactants = Vec::new();
    let mut products = Vec::new();
    let mut reactant_atoms = BTreeMap::new();
    let mut product_atoms = BTreeMap::new();
    let mut reactant_domains = BTreeSet::new();
    let mut reactant_associations = BTreeSet::new();
    let mut product_instances = BTreeSet::new();
    for (side, terms) in [
        (RuleSideRecord::Reactant, outcome.declaration().reactants()),
        (RuleSideRecord::Product, outcome.declaration().products()),
    ] {
        for (index, term) in terms.iter().enumerate() {
            let role = match side {
                RuleSideRecord::Reactant => format!("reactant{}", index + 1),
                RuleSideRecord::Product => format!("product{}", index + 1),
            };
            let species = resolved.get(term.species()).ok_or_else(|| {
                AgentError::new(
                    "mechanism request",
                    format!(
                        "declaration species `{}` has no resolved identity",
                        term.species()
                    ),
                )
            })?;
            let structure = species.structure.as_ref().ok_or_else(|| {
                AgentError::new(
                    "mechanism request",
                    format!("species `{}` has no validated structure", term.species()),
                )
            })?;
            let labelled = labelled_structure(structure, term.formula_text());
            let representation = representation_record(structure.representation());
            let entry = MechanismSpecies {
                role: role.clone(),
                coefficient: term.coefficient(),
                structure: labelled,
            };
            for instance in 1..=term.coefficient() {
                for atom in structure.graph().atoms().values() {
                    let path = format!("{role}[{instance}].{}", atom.id());
                    let target = match side {
                        RuleSideRecord::Reactant => &mut reactant_atoms,
                        RuleSideRecord::Product => &mut product_atoms,
                    };
                    target.insert(path, atom.element().to_string());
                }
                if side == RuleSideRecord::Reactant {
                    reactant_domains.extend(
                        structure
                            .graph()
                            .metallic_domains()
                            .values()
                            .map(|domain| format!("{role}[{instance}].{}", domain.id())),
                    );
                    reactant_associations.extend(
                        structure
                            .graph()
                            .ionic_associations()
                            .values()
                            .map(|association| format!("{role}[{instance}].{}", association.id())),
                    );
                } else {
                    product_instances.insert(format!("{role}[{instance}]"));
                }
            }
            roles.insert(
                role,
                MechanismRole {
                    species: term.species().clone(),
                    structure: structure.id().clone(),
                    coefficient: term.coefficient(),
                    side,
                    representation,
                },
            );
            match side {
                RuleSideRecord::Reactant => reactants.push(entry),
                RuleSideRecord::Product => products.push(entry),
            }
        }
    }
    let elements = roles
        .values()
        .filter_map(|role| catalogue.structures().get(&role.structure))
        .flat_map(|structure| structure.formula().elements().keys())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    let mut neutral_valence = catalogue
        .document()
        .valence_premises
        .iter()
        .flat_map(|premise| premise.neutral_valence.iter())
        .filter(|entry| elements.contains(&entry.element))
        .cloned()
        .collect::<Vec<_>>();
    neutral_valence.sort_by(|left, right| {
        left.element.cmp(&right.element).then(
            left.neutral_valence_electrons
                .cmp(&right.neutral_valence_electrons),
        )
    });
    neutral_valence.dedup();
    let supported_states = catalogue
        .document()
        .valence_premises
        .iter()
        .flat_map(|premise| premise.supported_states.iter())
        .filter(|state| elements.contains(&state.element))
        .cloned()
        .collect::<Vec<_>>();
    let metallic_states = catalogue
        .document()
        .valence_premises
        .iter()
        .flat_map(|premise| premise.metallic_domain_states.iter())
        .filter(|state| elements.contains(&state.element))
        .cloned()
        .collect::<Vec<_>>();
    Ok(Some(MechanismContext {
        request: MechanismEscalationRequest {
            schema_version: crate::claim::MECHANISM_ESCALATION_SCHEMA_VERSION,
            reaction_id: format!(
                "Dynamic.r{}",
                &outcome.declaration().digest().to_hex()[..24]
            ),
            reactants,
            products,
            reactant_atom_paths: reactant_atoms.keys().cloned().collect(),
            product_atom_paths: product_atoms.keys().cloned().collect(),
            neutral_valence,
            supported_states,
            metallic_states,
            provisional_states_allowed: true,
        },
        roles,
        reactant_atoms,
        product_atoms,
        reactant_domains,
        reactant_associations,
        product_instances,
    }))
}

/// Runs one mechanism proposal plus at most two operation-level repairs.
/// Exhaustion retains the already validated static outcome.
pub fn derive_mechanism<P: MechanismProvider>(
    outcome: ValidatedStaticOutcome,
    catalogue: &ValidatedCatalogueBundle,
    provider: &mut P,
) -> MechanismEscalationOutcome {
    match compile_mechanism_request(&outcome, catalogue) {
        Ok(Some(context)) => propose_mechanism_frames(outcome, catalogue, provider, &context, 0),
        Ok(None) => derive_with_proposed_structures(outcome, catalogue, provider),
        Err(error) => MechanismEscalationOutcome::Unavailable {
            static_outcome: Box::new(outcome),
            attempts: 0,
            diagnostic: error.to_string(),
            retryable: false,
        },
    }
}

fn propose_mechanism_frames<P: MechanismProvider>(
    outcome: ValidatedStaticOutcome,
    catalogue: &ValidatedCatalogueBundle,
    provider: &mut P,
    context: &MechanismContext,
    structure_repair_count: usize,
) -> MechanismEscalationOutcome {
    let mut diagnostic = None;
    for attempt in 0..=MAX_MECHANISM_REPAIRS {
        let response = match provider.propose(&context.request, diagnostic.as_deref()) {
            Ok(response) => response,
            Err(error) => {
                diagnostic = Some(error.to_string());
                continue;
            }
        };
        match compile_mechanism(&outcome, context, &response, catalogue) {
            Ok(frames) => {
                return MechanismEscalationOutcome::Animated(Box::new(EscalatedMechanismOutcome {
                    static_outcome: outcome,
                    frames,
                    repair_count: attempt,
                    structure_repair_count,
                }));
            }
            Err(error) => diagnostic = Some(error.to_string()),
        }
    }
    MechanismEscalationOutcome::Unavailable {
        static_outcome: Box::new(outcome),
        attempts: MAX_MECHANISM_REPAIRS + 1,
        diagnostic: diagnostic.unwrap_or_else(|| "mechanism validation failed".to_owned()),
        retryable: true,
    }
}

/// Structure escalation: products absent from the reviewed structure library
/// receive model-proposed graphs, validated inside an isolated working bundle,
/// before mechanism escalation runs against that bundle. Exhaustion retains
/// the validated static outcome and stays retryable.
fn derive_with_proposed_structures<P: MechanismProvider>(
    outcome: ValidatedStaticOutcome,
    catalogue: &ValidatedCatalogueBundle,
    provider: &mut P,
) -> MechanismEscalationOutcome {
    let Some(request) = structure_proposal_request(&outcome, catalogue) else {
        return MechanismEscalationOutcome::Unavailable {
            static_outcome: Box::new(outcome),
            attempts: 0,
            diagnostic: "mechanism request is incomplete without missing products".to_owned(),
            retryable: false,
        };
    };
    let mut diagnostic: Option<String> = None;
    for structure_attempt in 0..=MAX_STRUCTURE_REPAIRS {
        let response = match provider.propose_structures(&request, diagnostic.as_deref()) {
            Ok(response) => response,
            Err(error) => {
                diagnostic = Some(error.to_string());
                continue;
            }
        };
        match adopt_proposed_structures(&outcome, &request, &response, catalogue) {
            Ok(adopted) => {
                return match compile_mechanism_request(&adopted.outcome, &adopted.bundle) {
                    Ok(Some(context)) => propose_mechanism_frames(
                        adopted.outcome,
                        &adopted.bundle,
                        provider,
                        &context,
                        structure_attempt,
                    ),
                    Ok(None) | Err(_) => MechanismEscalationOutcome::Unavailable {
                        static_outcome: Box::new(adopted.outcome),
                        attempts: 0,
                        diagnostic:
                            "adopted structures did not produce a complete mechanism request"
                                .to_owned(),
                        retryable: true,
                    },
                };
            }
            Err(error) => diagnostic = Some(error.to_string()),
        }
    }
    MechanismEscalationOutcome::Unavailable {
        static_outcome: Box::new(outcome),
        attempts: MAX_STRUCTURE_REPAIRS + 1,
        diagnostic: diagnostic.unwrap_or_else(|| "structure proposal remained invalid".to_owned()),
        retryable: true,
    }
}

/// Revalidates a cached escalation response through the same label, kernel,
/// and frame path used for a live response.
///
/// # Errors
///
/// Returns an error when cached labels, operations, catalogue dependencies,
/// kernel transitions, or frame projection no longer validate.
pub fn validate_escalated_response(
    outcome: ValidatedStaticOutcome,
    response: &MechanismEscalationResponse,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<EscalatedMechanismOutcome, AgentError> {
    let context = compile_mechanism_request(&outcome, catalogue)?.ok_or_else(|| {
        AgentError::new(
            "mechanism cache",
            "cached escalation requires structures for every product",
        )
    })?;
    let frames = compile_mechanism(&outcome, &context, response, catalogue)?;
    Ok(EscalatedMechanismOutcome {
        static_outcome: outcome,
        frames,
        repair_count: 0,
        structure_repair_count: 0,
    })
}

/// Revalidates a cached escalation, re-adopting any cached structure proposal
/// through the identical isolated working-bundle validation first.
///
/// # Errors
///
/// Returns an error when the cached structures or operations no longer
/// validate against the current catalogue and contracts.
pub fn validate_escalated_response_with_structures(
    outcome: ValidatedStaticOutcome,
    structures: Option<&StructureProposalResponse>,
    response: &MechanismEscalationResponse,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<EscalatedMechanismOutcome, AgentError> {
    let Some(structures) = structures else {
        return validate_escalated_response(outcome, response, catalogue);
    };
    let request = structure_proposal_request(&outcome, catalogue).ok_or_else(|| {
        AgentError::new(
            "mechanism cache",
            "cached structure proposal does not correspond to missing products",
        )
    })?;
    let adopted = adopt_proposed_structures(&outcome, &request, structures, catalogue)?;
    validate_escalated_response(adopted.outcome, response, &adopted.bundle)
}

fn compile_mechanism(
    outcome: &ValidatedStaticOutcome,
    context: &MechanismContext,
    response: &MechanismEscalationResponse,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<ValidatedDynamicFrames, AgentError> {
    validate_response_labels(context, response)?;
    let provisional_bundle = provisional_mechanism_bundle(context, response, catalogue)?;
    let catalogue = provisional_bundle.as_ref().unwrap_or(catalogue);
    let premise_ids = mechanism_premises(context, catalogue)?;
    let applicability_premise = premise_ids.first().cloned().ok_or_else(|| {
        AgentError::new("mechanism compile", "catalogue exposes no valence premise")
    })?;
    let role_species = context
        .roles
        .iter()
        .map(|(role, value)| (role.clone(), value.species.clone()))
        .collect::<BTreeMap<_, _>>();
    let roles = context
        .roles
        .iter()
        .map(|(role, value)| {
            (
                role.clone(),
                RoleSchemaRecord {
                    side: value.side,
                    representation: value.representation,
                },
            )
        })
        .collect();
    let terms = |side: RuleSideRecord| {
        context
            .roles
            .iter()
            .filter(|(_, value)| value.side == side)
            .map(|(role, value)| PatternTermRecord {
                role: role.clone(),
                structure_id: value.structure.clone(),
                coefficient: value.coefficient,
            })
            .collect::<Vec<_>>()
    };
    let rule = ReactionRuleRecord {
        id: ReactionRuleId::from_str(&format!(
            "DynamicMechanism.r{}",
            &outcome.declaration().digest().to_hex()[..24]
        ))
        .map_err(|error| AgentError::new("mechanism compile", error.to_string()))?,
        premise_ids: premise_ids.clone(),
        roles,
        reactant_pattern: terms(RuleSideRecord::Reactant),
        product_pattern: terms(RuleSideRecord::Product),
        applicability: ApplicabilityRecord {
            premise_id: applicability_premise,
            request_relation: RequestRelation::Contact,
            reactant_structure_ids: context
                .roles
                .values()
                .filter(|value| value.side == RuleSideRecord::Reactant)
                .map(|value| value.structure.clone())
                .collect(),
            required_context: outcome.declaration().required_context().to_owned(),
        },
        mapping_template: response
            .mapping
            .iter()
            .map(|mapping| MappingPairRecord {
                reactant: mapping.reactant.clone(),
                product: mapping.product.clone(),
                premise_ids: premise_ids.clone(),
            })
            .collect(),
        operation_template: response
            .operations
            .iter()
            .map(|operation| operation_record(operation, &premise_ids))
            .collect(),
        model_assumptions: ModelAssumptionsRecord {
            event: EventModel::Representative,
            sequence: SequenceModel::Explanatory,
            premise_ids: premise_ids.clone(),
        },
        observation_compatibility: Vec::new(),
    };
    let expanded = expand_proposed_declaration(
        &context.request.reaction_id,
        outcome.declaration(),
        &role_species,
        &rule,
        catalogue,
    )
    .map_err(|error| AgentError::new("mechanism expansion", error.to_string()))?;
    let derivation = validate_review_candidate(&expanded, catalogue)
        .map_err(|error| AgentError::new("mechanism validation", error.to_string()))?;
    Ok(inspect_review_candidate_frames(&derivation)
        .map_err(|error| AgentError::new("mechanism frames", error.to_string()))?
        .into_validated_dynamic())
}

#[allow(clippy::too_many_lines)]
fn provisional_mechanism_bundle(
    context: &MechanismContext,
    response: &MechanismEscalationResponse,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<Option<ValidatedCatalogueBundle>, AgentError> {
    let mut neutral = BTreeMap::<String, BTreeSet<u8>>::new();
    let mut reviewed = BTreeSet::new();
    let mut reviewed_metallic = BTreeSet::new();
    for premise in &catalogue.document().valence_premises {
        for entry in &premise.neutral_valence {
            neutral
                .entry(entry.element.clone())
                .or_default()
                .insert(entry.neutral_valence_electrons);
        }
        reviewed.extend(premise.supported_states.iter().cloned());
        reviewed_metallic.extend(premise.metallic_domain_states.iter().cloned());
    }
    let mut provisional = BTreeSet::new();
    let mut used_neutral = BTreeMap::<String, u8>::new();
    for (path, state) in mechanism_electron_states(response) {
        let element = context.reactant_atoms.get(path).ok_or_else(|| {
            AgentError::new(
                "provisional valence",
                format!("operation state references unknown atom `{path}`"),
            )
        })?;
        ElectronState::new(state.0, state.1, state.2).map_err(|error| {
            AgentError::new(
                "provisional valence",
                format!("atom `{path}` has an invalid electron state: {error}"),
            )
        })?;
        let candidates = neutral.get(element).ok_or_else(|| {
            AgentError::new(
                "provisional valence",
                format!("atom `{path}` has no reviewed neutral valence"),
            )
        })?;
        let candidate = candidates.iter().find_map(|neutral_electrons| {
            let bond_sum = i16::from(*neutral_electrons) - i16::from(state.1) - state.0;
            u8::try_from(bond_sum)
                .ok()
                .map(|bond_sum| (*neutral_electrons, bond_sum))
        });
        let Some((neutral_electrons, covalent_bond_order_sum)) = candidate else {
            return Err(AgentError::new(
                "provisional valence",
                format!("atom `{path}` violates the reviewed formal-charge identity"),
            ));
        };
        let record = ValenceStateRecord {
            element: element.clone(),
            formal_charge: state.0,
            non_bonding_electrons: state.1,
            unpaired_electrons: state.2,
            covalent_bond_order_sum,
        };
        if reviewed.contains(&record) {
            continue;
        }
        if used_neutral
            .insert(element.clone(), neutral_electrons)
            .is_some_and(|existing| existing != neutral_electrons)
        {
            return Err(AgentError::new(
                "provisional valence",
                format!("operation states require conflicting neutral valence for `{element}`"),
            ));
        }
        provisional.insert(record);
    }
    let provisional_metallic = derive_provisional_metallic_operation_states(
        context,
        response,
        catalogue,
        &reviewed_metallic,
    )?;
    if provisional.is_empty() && provisional_metallic.is_empty() {
        return Ok(None);
    }
    if provisional.is_empty() {
        let Some(reviewed_anchor) = reviewed.iter().find(|state| {
            provisional_metallic
                .iter()
                .any(|metallic| metallic.element == state.element)
        }) else {
            return Err(AgentError::new(
                "provisional valence",
                "a provisional metallic state has no reviewed covalent anchor",
            ));
        };
        let neutral_electrons = neutral
            .get(&reviewed_anchor.element)
            .and_then(|values| {
                values.iter().find(|value| {
                    i16::from(**value)
                        - i16::from(reviewed_anchor.non_bonding_electrons)
                        - i16::from(reviewed_anchor.covalent_bond_order_sum)
                        == reviewed_anchor.formal_charge
                })
            })
            .copied()
            .ok_or_else(|| {
                AgentError::new(
                    "provisional valence",
                    "reviewed metallic anchor violates its neutral-valence premise",
                )
            })?;
        used_neutral.insert(reviewed_anchor.element.clone(), neutral_electrons);
        provisional.insert(reviewed_anchor.clone());
    }
    let premise_id = PremiseId::from_str(DYNAMIC_MECHANISM_VALENCE_PREMISE)
        .map_err(|error| AgentError::new("provisional valence", error.to_string()))?;
    let mut document = catalogue.document().clone();
    document.publication = PublicationKind::Working;
    let evidence = document
        .evidence
        .first()
        .map(|source| source.id.clone())
        .into_iter()
        .collect::<BTreeSet<_>>();
    document.premises.push(PremiseRecord {
        id: premise_id.clone(),
        statement: "ChemSpec-derived provisional operation valence states".to_owned(),
        evidence,
        review: ReviewMetadata {
            status: ReviewStatus::Provisional,
            reviewers: Vec::new(),
        },
        rule_version: "1".to_owned(),
    });
    document.valence_premises.push(ValencePremiseRecord {
        premise_id,
        neutral_valence: used_neutral
            .into_iter()
            .map(
                |(element, neutral_valence_electrons)| ElementValenceRecord {
                    element,
                    neutral_valence_electrons,
                },
            )
            .collect(),
        supported_states: provisional.into_iter().collect(),
        metallic_domain_states: provisional_metallic.into_iter().collect(),
    });
    let mut envelope = CatalogueEnvelope {
        digest: ContentDigest::sha256(b"uncomputed dynamic mechanism valence bundle"),
        bundle: document,
    };
    envelope.digest = envelope
        .computed_digest()
        .map_err(|error| AgentError::new("provisional valence", error.to_string()))?;
    ValidatedCatalogueBundle::validate(envelope)
        .map(Some)
        .map_err(|error| {
            AgentError::new(
                "provisional valence",
                format!("derived operation states failed working-bundle validation: {error}"),
            )
        })
}

fn derive_provisional_metallic_operation_states(
    context: &MechanismContext,
    response: &MechanismEscalationResponse,
    catalogue: &ValidatedCatalogueBundle,
    reviewed: &BTreeSet<MetallicValenceStateRecord>,
) -> Result<BTreeSet<MetallicValenceStateRecord>, AgentError> {
    let mut provisional = BTreeSet::new();
    for operation in &response.operations {
        let (site, domain, state, site_delta) = match operation {
            MechanismOperation::ReleaseMetallic {
                site,
                domain,
                before,
                ..
            } => (site, domain, before, 0_i8),
            MechanismOperation::JoinMetallic {
                site,
                domain,
                after,
                ..
            } => (site, domain, after, 1_i8),
            _ => continue,
        };
        if state.site.1 != 0 {
            return Err(AgentError::new(
                "provisional valence",
                format!("metallic site `{site}` must have zero local electrons in-domain"),
            ));
        }
        let role = domain.split('[').next().ok_or_else(|| {
            AgentError::new("provisional valence", "metallic domain has no role prefix")
        })?;
        let domain_label = domain
            .split_once("].")
            .map(|(_, label)| label)
            .ok_or_else(|| {
                AgentError::new("provisional valence", "metallic domain path is malformed")
            })?;
        let role = context.roles.get(role).ok_or_else(|| {
            AgentError::new(
                "provisional valence",
                "metallic domain role does not resolve",
            )
        })?;
        let structure = catalogue.structures().get(&role.structure).ok_or_else(|| {
            AgentError::new("provisional valence", "metallic structure does not resolve")
        })?;
        let domain = structure
            .graph()
            .metallic_domains()
            .values()
            .find(|candidate| candidate.id().as_str() == domain_label)
            .ok_or_else(|| {
                AgentError::new(
                    "provisional valence",
                    "metallic domain label does not resolve",
                )
            })?;
        let base_count = u32::try_from(domain.sites().len())
            .map_err(|_| AgentError::new("provisional valence", "metallic site count overflow"))?;
        let site_count = if site_delta == 0 {
            base_count
        } else {
            base_count.checked_add(1).ok_or_else(|| {
                AgentError::new("provisional valence", "metallic site count overflow")
            })?
        };
        if site_count == 0
            || state.domain_electrons == 0
            || state.domain_electrons % site_count != 0
        {
            return Err(AgentError::new(
                "provisional valence",
                format!(
                    "metallic domain `{}` has inconsistent site electrons",
                    domain.id()
                ),
            ));
        }
        let element = context.reactant_atoms.get(site).ok_or_else(|| {
            AgentError::new(
                "provisional valence",
                "metallic operation site does not resolve",
            )
        })?;
        let record = MetallicValenceStateRecord {
            element: element.clone(),
            site_formal_charge: state.site.0,
            site_local_electrons: state.site.1,
            delocalized_electrons_per_site: state.domain_electrons / site_count,
        };
        if !reviewed.contains(&record) {
            provisional.insert(record);
        }
    }
    Ok(provisional)
}

fn push_binary_states<'a>(
    states: &mut Vec<(&'a str, chem_catalogue::ElectronStateRecord)>,
    left: &'a str,
    right: &'a str,
    before: &chem_catalogue::BinaryElectronStateRecord,
    after: &chem_catalogue::BinaryElectronStateRecord,
) {
    states.extend([
        (left, before.left),
        (right, before.right),
        (left, after.left),
        (right, after.right),
    ]);
}

fn mechanism_electron_states(
    response: &MechanismEscalationResponse,
) -> Vec<(&str, chem_catalogue::ElectronStateRecord)> {
    let mut states = Vec::new();
    for operation in &response.operations {
        match operation {
            MechanismOperation::ReconfigureElectrons {
                atom,
                before,
                after,
            } => {
                states.extend([(atom.as_str(), *before), (atom.as_str(), *after)]);
            }
            MechanismOperation::CleaveCovalent {
                edge,
                before,
                after,
                ..
            }
            | MechanismOperation::FormCovalent {
                edge,
                before,
                after,
                ..
            } => push_binary_states(&mut states, &edge.0, &edge.1, before, after),
            MechanismOperation::CleaveDative {
                donor,
                acceptor,
                before,
                after,
                ..
            }
            | MechanismOperation::FormDative {
                donor,
                acceptor,
                before,
                after,
            }
            | MechanismOperation::ChangeCovalent {
                edge: (donor, acceptor),
                before,
                after,
                ..
            } => push_binary_states(&mut states, donor, acceptor, before, after),
            MechanismOperation::TransferElectron {
                donor,
                acceptor,
                before,
                after,
                ..
            } => {
                states.extend([
                    (donor.as_str(), before.donor),
                    (acceptor.as_str(), before.acceptor),
                    (donor.as_str(), after.donor),
                    (acceptor.as_str(), after.acceptor),
                ]);
            }
            MechanismOperation::ReleaseMetallic {
                site,
                before,
                after,
                ..
            }
            | MechanismOperation::JoinMetallic {
                site,
                before,
                after,
                ..
            } => {
                states.extend([(site.as_str(), before.site), (site.as_str(), after.site)]);
            }
            MechanismOperation::AssociateIonic { .. }
            | MechanismOperation::DissociateIonic { .. }
            | MechanismOperation::AssignProduct { .. } => {}
        }
    }
    states
}

fn mechanism_premises(
    context: &MechanismContext,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<BTreeSet<PremiseId>, AgentError> {
    let mut premises = catalogue
        .document()
        .valence_premises
        .iter()
        .map(|record| record.premise_id.clone())
        .collect::<BTreeSet<_>>();
    for role in context.roles.values() {
        let closure = catalogue
            .structure_premises(&role.structure)
            .ok_or_else(|| AgentError::new("mechanism compile", "structure premise disappeared"))?;
        premises.extend(closure.iter().cloned());
    }
    Ok(premises)
}

fn validate_response_labels(
    context: &MechanismContext,
    response: &MechanismEscalationResponse,
) -> Result<(), AgentError> {
    if response.mapping.len() != context.reactant_atoms.len()
        || response.mapping.len() != context.product_atoms.len()
    {
        return Err(AgentError::new(
            "mechanism mapping",
            "mapping must cover every reactant and product atom exactly once",
        ));
    }
    let mut reactants = BTreeSet::new();
    let mut products = BTreeSet::new();
    for mapping in &response.mapping {
        let reactant_element = context.reactant_atoms.get(&mapping.reactant);
        let product_element = context.product_atoms.get(&mapping.product);
        if reactant_element.is_none()
            || product_element.is_none()
            || reactant_element != product_element
            || !reactants.insert(&mapping.reactant)
            || !products.insert(&mapping.product)
        {
            return Err(AgentError::new(
                "mechanism mapping",
                "mapping contains an unknown, duplicate, or element-changing atom label",
            ));
        }
    }
    for operation in &response.operations {
        validate_operation_labels(context, operation)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_operation_labels(
    context: &MechanismContext,
    operation: &MechanismOperation,
) -> Result<(), AgentError> {
    let atom = |value: &str| {
        if context.reactant_atoms.contains_key(value) {
            Ok(())
        } else {
            Err(AgentError::new(
                "mechanism operation",
                format!("unknown atom label `{value}`"),
            ))
        }
    };
    match operation {
        MechanismOperation::ReconfigureElectrons { atom: value, .. } => atom(value),
        MechanismOperation::CleaveCovalent { edge, .. }
        | MechanismOperation::FormCovalent { edge, .. } => {
            atom(&edge.0)?;
            atom(&edge.1)
        }
        MechanismOperation::CleaveDative {
            donor, acceptor, ..
        }
        | MechanismOperation::FormDative {
            donor, acceptor, ..
        }
        | MechanismOperation::TransferElectron {
            donor, acceptor, ..
        } => {
            atom(donor)?;
            atom(acceptor)
        }
        MechanismOperation::ChangeCovalent { edge, .. } => {
            atom(&edge.0)?;
            atom(&edge.1)
        }
        MechanismOperation::AssociateIonic { components, .. } => {
            for value in components.iter().flatten() {
                atom(value)?;
            }
            Ok(())
        }
        MechanismOperation::DissociateIonic { association } => {
            if context.reactant_associations.contains(association) {
                Ok(())
            } else {
                Err(AgentError::new(
                    "mechanism operation",
                    format!("unknown ionic association `{association}`"),
                ))
            }
        }
        MechanismOperation::ReleaseMetallic { site, domain, .. }
        | MechanismOperation::JoinMetallic { site, domain, .. } => {
            atom(site)?;
            if context.reactant_domains.contains(domain) {
                Ok(())
            } else {
                Err(AgentError::new(
                    "mechanism operation",
                    format!("unknown metallic domain `{domain}`"),
                ))
            }
        }
        MechanismOperation::AssignProduct { atoms, product } => {
            for value in atoms {
                atom(value)?;
            }
            if context.product_instances.contains(product) {
                Ok(())
            } else {
                Err(AgentError::new(
                    "mechanism operation",
                    format!("unknown product instance `{product}`"),
                ))
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn operation_record(
    operation: &MechanismOperation,
    premise_ids: &BTreeSet<PremiseId>,
) -> OperationTemplateRecord {
    let premises = || premise_ids.clone();
    match operation {
        MechanismOperation::ReconfigureElectrons {
            atom,
            before,
            after,
        } => OperationTemplateRecord::ReconfigureElectrons {
            premise_ids: premises(),
            atom: atom.clone(),
            before: *before,
            after: *after,
        },
        MechanismOperation::CleaveCovalent {
            edge,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::CleaveCovalent {
            premise_ids: premises(),
            edge: edge.clone(),
            allocation: cleavage(allocation),
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::FormCovalent {
            edge,
            electron_contribution,
            before,
            after,
        } => OperationTemplateRecord::FormCovalent {
            premise_ids: premises(),
            edge: edge.clone(),
            electron_contribution: electron_contribution.clone(),
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::CleaveDative {
            donor,
            acceptor,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::CleaveDative {
            premise_ids: premises(),
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            allocation: cleavage(allocation),
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::FormDative {
            donor,
            acceptor,
            before,
            after,
        } => OperationTemplateRecord::FormDative {
            premise_ids: premises(),
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::ChangeCovalent {
            edge,
            old_order,
            new_order,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::ChangeCovalent {
            premise_ids: premises(),
            edge: edge.clone(),
            old_order: *old_order,
            new_order: *new_order,
            allocation: cleavage(allocation),
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::AssociateIonic {
            label,
            components,
            component_charges,
        } => OperationTemplateRecord::AssociateIonic {
            premise_ids: premises(),
            label: label.clone(),
            components: components.clone(),
            component_charges: component_charges.clone(),
        },
        MechanismOperation::DissociateIonic { association } => {
            OperationTemplateRecord::DissociateIonic {
                premise_ids: premises(),
                association: association.clone(),
            }
        }
        MechanismOperation::ReleaseMetallic {
            site,
            domain,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::ReleaseMetallic {
            premise_ids: premises(),
            site: site.clone(),
            domain: domain.clone(),
            allocation: *allocation,
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::JoinMetallic {
            site,
            domain,
            allocation,
            before,
            after,
        } => OperationTemplateRecord::JoinMetallic {
            premise_ids: premises(),
            site: site.clone(),
            domain: domain.clone(),
            allocation: *allocation,
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::TransferElectron {
            count,
            donor,
            acceptor,
            before,
            after,
        } => OperationTemplateRecord::TransferElectron {
            premise_ids: premises(),
            count: *count,
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            before: before.clone(),
            after: after.clone(),
        },
        MechanismOperation::AssignProduct { atoms, product } => {
            OperationTemplateRecord::AssignProduct {
                premise_ids: premises(),
                atoms: atoms.clone(),
                product: product.clone(),
            }
        }
    }
}

fn cleavage(value: &MechanismCleavageAllocation) -> CleavageAllocationRecord {
    match value {
        MechanismCleavageAllocation::Homolytic(MechanismHomolytic::Homolytic) => {
            CleavageAllocationRecord::Homolytic("homolytic".to_owned())
        }
        MechanismCleavageAllocation::Heterolytic { heterolytic_to } => {
            CleavageAllocationRecord::Heterolytic {
                heterolytic_to: heterolytic_to.clone(),
            }
        }
    }
}

fn labelled_structure(structure: &StructureDefinition, formula: &str) -> LabelledStructure {
    let graph = structure.graph();
    let atoms = || graph.atoms().values().map(atom_record).collect::<Vec<_>>();
    let bonds = || {
        graph
            .covalent_bonds()
            .values()
            .map(bond_record)
            .collect::<Vec<_>>()
    };
    let groups = || {
        graph
            .groups()
            .values()
            .map(|group| GroupRecord {
                label: group.id().to_string(),
                atoms: group.atoms().iter().map(ToString::to_string).collect(),
            })
            .collect::<Vec<_>>()
    };
    match structure.representation() {
        RepresentationKind::Molecular => LabelledStructure::Molecular {
            id: structure.id().to_string(),
            formula: formula.to_owned(),
            atoms: atoms(),
            bonds: bonds(),
            groups: groups(),
        },
        RepresentationKind::Ion => LabelledStructure::Ion {
            id: structure.id().to_string(),
            formula: formula.to_owned(),
            atoms: atoms(),
            bonds: bonds(),
            groups: groups(),
        },
        RepresentationKind::Ionic => {
            let components = graph
                .groups()
                .values()
                .map(|group| {
                    let member_ids = group.atoms();
                    ComponentRecord {
                        label: group.id().to_string(),
                        atoms: member_ids
                            .iter()
                            .filter_map(|id| graph.atoms().get(id))
                            .map(atom_record)
                            .collect(),
                        bonds: graph
                            .covalent_bonds()
                            .values()
                            .filter(|bond| {
                                member_ids.contains(bond.left())
                                    && member_ids.contains(bond.right())
                            })
                            .map(bond_record)
                            .collect(),
                        groups: Vec::new(),
                    }
                })
                .collect();
            let associations = graph
                .ionic_associations()
                .values()
                .map(|association| IonicAssociationRecord {
                    label: association.id().to_string(),
                    components: association
                        .components()
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                })
                .collect();
            LabelledStructure::Ionic {
                id: structure.id().to_string(),
                formula: formula.to_owned(),
                components,
                associations,
            }
        }
        RepresentationKind::Metallic => LabelledStructure::Metallic {
            id: structure.id().to_string(),
            formula: formula.to_owned(),
            sites: atoms(),
            domains: graph
                .metallic_domains()
                .values()
                .map(|domain| MetallicDomainRecord {
                    label: domain.id().to_string(),
                    sites: domain.sites().iter().map(ToString::to_string).collect(),
                    delocalized_electrons: domain.delocalized_electrons(),
                })
                .collect(),
        },
    }
}

fn atom_record(atom: &chem_domain::Atom) -> AtomRecord {
    AtomRecord {
        label: atom.id().to_string(),
        element: atom.element().to_string(),
        formal_charge: atom.electrons().formal_charge(),
        non_bonding_electrons: atom.electrons().non_bonding_electrons(),
        unpaired_electrons: atom.electrons().unpaired_electrons(),
    }
}

fn bond_record(bond: &chem_domain::CovalentBond) -> BondRecord {
    BondRecord {
        left: bond.left().to_string(),
        right: bond.right().to_string(),
        order: bond_order(bond.order()),
        electron_origin: match bond.electron_origin() {
            CovalentElectronOrigin::Shared => BondElectronOriginRecord::Shared,
            CovalentElectronOrigin::Dative { donor, acceptor } => {
                BondElectronOriginRecord::Dative {
                    donor: donor.to_string(),
                    acceptor: acceptor.to_string(),
                }
            }
        },
        delocalization: bond.delocalization().map(|value| BondDelocalizationRecord {
            domain: value.domain().to_string(),
            effective_order: chem_catalogue::EffectiveBondOrderRecord {
                numerator: value.effective_order().numerator(),
                denominator: value.effective_order().denominator(),
            },
        }),
    }
}

const fn bond_order(value: chem_domain::BondOrder) -> BondOrderRecord {
    match value {
        chem_domain::BondOrder::Single => BondOrderRecord::Single,
        chem_domain::BondOrder::Double => BondOrderRecord::Double,
        chem_domain::BondOrder::Triple => BondOrderRecord::Triple,
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use chem_catalogue::TrustedCatalogue;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        ClaimMode, CompiledClaimOutcome, FamilyMatchOutcome, ReactantInput, ReactionBuildRequest,
        ReactionClaim, compile_claim_outcome, match_reviewed_family, reviewed_species_registry,
    };

    fn trusted() -> TrustedCatalogue {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        TrustedCatalogue::from_canonical_json(
            &std::fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json"))
                .expect("catalogue"),
            &std::fs::read(root.join("catalogue/trusted/core-chemistry/review.json"))
                .expect("review"),
        )
        .expect("trusted catalogue")
    }

    fn static_outcome_for(
        trusted: &TrustedCatalogue,
        reactants: [(&str, Vec<u8>); 2],
        products: &Value,
    ) -> ValidatedStaticOutcome {
        let identities = reviewed_species_registry(trusted).expect("identities");
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": products,
            "required_context": "representative educational outcome under the reviewed standard-outcome premise",
            "observations": [], "sources": [], "ambiguity": null
        });
        let claim = ReactionClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("claim contract");
        let compiled = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: reactants
                    .map(|(display, atomic_numbers)| ReactantInput {
                        display: display.into(),
                        atomic_numbers,
                        species_id: None,
                    })
                    .to_vec(),
                selected_context: None,
            },
            claim,
            &identities,
        )
        .expect("compiled outcome");
        let CompiledClaimOutcome::Static(outcome) = compiled else {
            panic!("static outcome: {compiled:?}")
        };
        outcome
    }

    fn static_outcome(trusted: &TrustedCatalogue, products: &Value) -> ValidatedStaticOutcome {
        static_outcome_for(
            trusted,
            [("LithiumMetal", vec![3]), ("H2O", vec![1, 1, 8])],
            products,
        )
    }

    fn lithium_hydroxide_outcome(trusted: &TrustedCatalogue) -> ValidatedStaticOutcome {
        static_outcome(
            trusted,
            &json!([
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ]),
        )
    }

    fn rewrite_paths(value: &mut Value, roles: &BTreeMap<String, String>) {
        match value {
            Value::Object(fields) => {
                fields.remove("premise_ids");
                for value in fields.values_mut() {
                    rewrite_paths(value, roles);
                }
            }
            Value::Array(values) => {
                for value in values {
                    rewrite_paths(value, roles);
                }
            }
            Value::String(text) => {
                for (source, target) in roles {
                    if let Some(suffix) = text.strip_prefix(&format!("{source}[")) {
                        *text = format!("{target}[{suffix}");
                        break;
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    fn valid_response(
        outcome: &ValidatedStaticOutcome,
        trusted: &TrustedCatalogue,
    ) -> MechanismEscalationResponse {
        let context = compile_mechanism_request(outcome, trusted)
            .expect("request")
            .expect("structural request");
        let matched = match_reviewed_family(outcome, trusted).expect("family match");
        let FamilyMatchOutcome::Matched(family) = matched else {
            panic!("reviewed family: {matched:?}")
        };
        let role_paths = family
            .role_species()
            .iter()
            .map(|(family_role, species)| {
                let request_role = context
                    .roles
                    .iter()
                    .find_map(|(role, value)| (&value.species == species).then(|| role.clone()))
                    .expect("request role for family species");
                (family_role.clone(), request_role)
            })
            .collect::<BTreeMap<_, _>>();
        let mut response = json!({
            "schema_version": crate::claim::MECHANISM_ESCALATION_SCHEMA_VERSION,
            "mapping": family.selected().rule.mapping_template,
            "operations": family.selected().rule.operation_template,
        });
        rewrite_paths(&mut response, &role_paths);
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&response).expect("response JSON"),
        )
        .expect("response contract")
    }

    #[derive(Default)]
    struct FakeProvider {
        responses: VecDeque<MechanismEscalationResponse>,
        structure_responses: VecDeque<StructureProposalResponse>,
        diagnostics: Vec<Option<String>>,
        structure_diagnostics: Vec<Option<String>>,
    }

    impl MechanismProvider for FakeProvider {
        fn propose(
            &mut self,
            _request: &MechanismEscalationRequest,
            diagnostic: Option<&str>,
        ) -> Result<MechanismEscalationResponse, AgentError> {
            self.diagnostics.push(diagnostic.map(str::to_owned));
            self.responses
                .pop_front()
                .ok_or_else(|| AgentError::new("fake provider", "no response"))
        }

        fn propose_structures(
            &mut self,
            _request: &StructureProposalRequest,
            diagnostic: Option<&str>,
        ) -> Result<StructureProposalResponse, AgentError> {
            self.structure_diagnostics
                .push(diagnostic.map(str::to_owned));
            self.structure_responses
                .pop_front()
                .ok_or_else(|| AgentError::new("fake provider", "no structure response"))
        }
    }

    /// A provider whose structure escalation always declines, mirroring the
    /// trait default.
    #[derive(Default)]
    struct MechanismOnlyProvider {
        mechanism_calls: usize,
    }

    impl MechanismProvider for MechanismOnlyProvider {
        fn propose(
            &mut self,
            _request: &MechanismEscalationRequest,
            _diagnostic: Option<&str>,
        ) -> Result<MechanismEscalationResponse, AgentError> {
            self.mechanism_calls += 1;
            Err(AgentError::new("fake provider", "no response"))
        }
    }

    #[test]
    fn reviewed_response_crosses_escalated_kernel_on_first_try() {
        let trusted = trusted();
        let outcome = lithium_hydroxide_outcome(&trusted);
        let response = valid_response(&outcome, &trusted);
        let mut provider = FakeProvider {
            responses: VecDeque::from([response]),
            ..FakeProvider::default()
        };
        let result = derive_mechanism(outcome, &trusted, &mut provider);
        let MechanismEscalationOutcome::Animated(animated) = result else {
            panic!("expected animation: {result:?}")
        };
        assert_eq!(animated.repair_count(), 0);
        assert!(!animated.frames().frames().is_empty());
        assert_eq!(provider.diagnostics, [None]);
        assert!(
            provider.structure_diagnostics.is_empty(),
            "a fully reviewed registry hit must never request structures"
        );
    }

    #[test]
    fn invalid_operation_is_repaired_without_changing_the_request() {
        let trusted = trusted();
        let outcome = lithium_hydroxide_outcome(&trusted);
        let valid = valid_response(&outcome, &trusted);
        let mut invalid = valid.clone();
        invalid.mapping[0].reactant = "reactant99[1].unknown".into();
        let mut provider = FakeProvider {
            responses: VecDeque::from([invalid, valid]),
            ..FakeProvider::default()
        };
        let result = derive_mechanism(outcome, &trusted, &mut provider);
        let MechanismEscalationOutcome::Animated(animated) = result else {
            panic!("expected repaired animation: {result:?}")
        };
        assert_eq!(animated.repair_count(), 1);
        assert_eq!(provider.diagnostics.len(), 2);
        assert!(provider.diagnostics[0].is_none());
        assert!(
            provider.diagnostics[1]
                .as_deref()
                .is_some_and(|value| value.contains("unknown"))
        );
    }

    #[test]
    fn exhausted_escalation_retains_static_outcome_and_retry() {
        let trusted = trusted();
        let outcome = lithium_hydroxide_outcome(&trusted);
        let mut invalid = valid_response(&outcome, &trusted);
        invalid.mapping[0].product = "product99[1].unknown".into();
        let mut provider = FakeProvider {
            responses: VecDeque::from([invalid.clone(), invalid.clone(), invalid]),
            ..FakeProvider::default()
        };
        let result = derive_mechanism(outcome, &trusted, &mut provider);
        let MechanismEscalationOutcome::Unavailable {
            static_outcome,
            attempts,
            retryable,
            diagnostic,
        } = result
        else {
            panic!("expected unavailable: {result:?}")
        };
        assert_eq!(attempts, 3);
        assert!(retryable);
        assert!(diagnostic.contains("unknown"));
        assert!(static_outcome.equation().contains("LiOH"));
    }

    #[test]
    fn formula_only_product_escalates_structures_and_stays_retryable() {
        let trusted = trusted();
        let outcome = static_outcome(
            &trusted,
            &json!([
                {"name":"lithium hydride candidate","formula":"LiH","phase":"solid","identity_hints":[]},
                {"name":"oxygen","formula":"O2","phase":"gas","identity_hints":[]}
            ]),
        );
        let mut provider = MechanismOnlyProvider::default();
        let result = derive_mechanism(outcome, &trusted, &mut provider);
        let MechanismEscalationOutcome::Unavailable {
            attempts,
            retryable,
            diagnostic,
            ..
        } = result
        else {
            panic!("expected unavailable: {result:?}")
        };
        assert_eq!(attempts, MAX_STRUCTURE_REPAIRS + 1);
        assert!(retryable, "a missing structure must remain retryable");
        assert!(diagnostic.contains("structure proposal"));
        assert_eq!(
            provider.mechanism_calls, 0,
            "mechanism escalation must wait for validated structures"
        );
    }

    #[test]
    fn one_structure_request_covers_missing_reactants_and_products() {
        let trusted = trusted();
        let outcome = static_outcome_for(
            &trusted,
            [("CH4", vec![6, 1, 1, 1, 1]), ("O2", vec![8, 8])],
            &json!([
                {"name":"methanol","formula":"CH4O","phase":"liquid","identity_hints":[]}
            ]),
        );
        let request = structure_proposal_request(&outcome, &trusted)
            .expect("both missing sides share one request");
        assert_eq!(
            request
                .species
                .iter()
                .map(|species| species.formula.as_str())
                .collect::<Vec<_>>(),
            ["CH4", "CH4O"]
        );
    }

    fn hydrogen_peroxide_outcome(trusted: &TrustedCatalogue) -> ValidatedStaticOutcome {
        static_outcome_for(
            trusted,
            [("H2", vec![1, 1]), ("O2", vec![8, 8])],
            &json!([
                {"name":"hydrogen peroxide","formula":"H2O2","phase":"liquid","identity_hints":[]}
            ]),
        )
    }

    fn hydrogen_peroxide_structure() -> StructureProposalResponse {
        StructureProposalResponse::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "structures": [{
                    "representation": "molecular",
                    "id": "DynamicStructure1",
                    "formula": "H2O2",
                    "atoms": [
                        {"label":"h1","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0},
                        {"label":"o1","element":"O","formal_charge":0,"non_bonding_electrons":4,"unpaired_electrons":0},
                        {"label":"o2","element":"O","formal_charge":0,"non_bonding_electrons":4,"unpaired_electrons":0},
                        {"label":"h2","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
                    ],
                    "bonds": [
                        {"left":"h1","right":"o1","order":"single"},
                        {"left":"o1","right":"o2","order":"single"},
                        {"left":"o2","right":"h2","order":"single"}
                    ],
                    "groups": []
                }]
            }))
            .expect("structure JSON"),
        )
        .expect("structure contract")
    }

    fn methane_outcome(trusted: &TrustedCatalogue) -> ValidatedStaticOutcome {
        static_outcome_for(
            trusted,
            [("CH4", vec![6, 1, 1, 1, 1]), ("O2", vec![8, 8])],
            &json!([
                {"name":"carbon dioxide","formula":"CO2","phase":"gas","identity_hints":[]},
                {"name":"water","formula":"H2O","phase":"gas","identity_hints":[]}
            ]),
        )
    }

    fn methane_structure() -> StructureProposalResponse {
        StructureProposalResponse::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "structures": [{
                    "representation": "molecular",
                    "id": "DynamicStructure1",
                    "formula": "CH4",
                    "atoms": [
                        {"label":"c","element":"C","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0},
                        {"label":"h1","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0},
                        {"label":"h2","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0},
                        {"label":"h3","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0},
                        {"label":"h4","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
                    ],
                    "bonds": [
                        {"left":"c","right":"h1","order":"single"},
                        {"left":"c","right":"h2","order":"single"},
                        {"left":"c","right":"h3","order":"single"},
                        {"left":"c","right":"h4","order":"single"}
                    ],
                    "groups": []
                }]
            }))
            .expect("structure JSON"),
        )
        .expect("structure contract")
    }

    #[test]
    fn chemspec_derives_provisional_operation_states_from_reviewed_neutral_valence() {
        let trusted = trusted();
        let outcome = methane_outcome(&trusted);
        let request =
            structure_proposal_request(&outcome, &trusted).expect("methane structure request");
        let adopted = adopt_proposed_structures(&outcome, &request, &methane_structure(), &trusted)
            .expect("methane structure validates");
        let context = compile_mechanism_request(&adopted.outcome, &adopted.bundle)
            .expect("mechanism request")
            .expect("all structures present");
        let carbon = context
            .reactant_atoms
            .iter()
            .find_map(|(path, element)| (element == "C").then_some(path.clone()))
            .expect("carbon path");
        let response = MechanismEscalationResponse {
            schema_version: crate::claim::MECHANISM_ESCALATION_SCHEMA_VERSION,
            mapping: vec![crate::MechanismMapping {
                reactant: carbon.clone(),
                product: context.request.product_atom_paths[0].clone(),
            }],
            operations: vec![MechanismOperation::ReconfigureElectrons {
                atom: carbon,
                before: chem_catalogue::ElectronStateRecord(0, 0, 0),
                after: chem_catalogue::ElectronStateRecord(0, 1, 1),
            }],
        };
        let bundle = provisional_mechanism_bundle(&context, &response, &adopted.bundle)
            .expect("derived provisional state")
            .expect("uncurated carbon radical adds a working bundle");
        assert!(bundle.document().valence_premises.iter().any(|premise| {
            premise.supported_states.iter().any(|state| {
                state.element == "C"
                    && state.formal_charge == 0
                    && state.non_bonding_electrons == 1
                    && state.unpaired_electrons == 1
                    && state.covalent_bond_order_sum == 3
            })
        }));
    }

    #[test]
    fn impossible_provisional_operation_state_fails_with_identity_diagnostic() {
        let trusted = trusted();
        let outcome = methane_outcome(&trusted);
        let request =
            structure_proposal_request(&outcome, &trusted).expect("methane structure request");
        let adopted = adopt_proposed_structures(&outcome, &request, &methane_structure(), &trusted)
            .expect("methane structure validates");
        let context = compile_mechanism_request(&adopted.outcome, &adopted.bundle)
            .expect("mechanism request")
            .expect("all structures present");
        let carbon = context
            .reactant_atoms
            .iter()
            .find_map(|(path, element)| (element == "C").then_some(path.clone()))
            .expect("carbon path");
        let response = MechanismEscalationResponse {
            schema_version: crate::claim::MECHANISM_ESCALATION_SCHEMA_VERSION,
            mapping: vec![crate::MechanismMapping {
                reactant: carbon.clone(),
                product: context.request.product_atom_paths[0].clone(),
            }],
            operations: vec![MechanismOperation::ReconfigureElectrons {
                atom: carbon,
                before: chem_catalogue::ElectronStateRecord(0, 0, 0),
                after: chem_catalogue::ElectronStateRecord(99, 1, 1),
            }],
        };
        let error = provisional_mechanism_bundle(&context, &response, &adopted.bundle)
            .expect_err("impossible state must fail");
        assert_eq!(error.stage(), "provisional valence");
        assert!(error.to_string().contains("formal-charge identity"));
    }

    /// Builds the peroxide mechanism over the exact labels of the adopted
    /// request, the same way a live provider reads them from its prompt.
    fn hydrogen_peroxide_mechanism(
        adopted: &crate::AdoptedProposedStructures,
    ) -> MechanismEscalationResponse {
        let context = compile_mechanism_request(&adopted.outcome, &adopted.bundle)
            .expect("request")
            .expect("complete structural request");
        let request = context.request();
        let species_atoms = |formula: &str, element: &str, entries: &[MechanismSpecies]| {
            entries
                .iter()
                .find_map(|entry| match &entry.structure {
                    LabelledStructure::Molecular {
                        formula: found,
                        atoms,
                        ..
                    } if found == formula => Some(
                        atoms
                            .iter()
                            .filter(|atom| atom.element == element)
                            .map(|atom| format!("{}[1].{}", entry.role, atom.label))
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no molecular species with formula {formula}"))
        };
        let h = species_atoms("H2", "H", &request.reactants);
        let o = species_atoms("O2", "O", &request.reactants);
        let product_h = species_atoms("H2O2", "H", &request.products);
        let product_o = species_atoms("H2O2", "O", &request.products);
        let product_instance = product_h[0]
            .split('.')
            .next()
            .expect("product instance prefix")
            .to_owned();
        let response = json!({
            "schema_version": crate::claim::MECHANISM_ESCALATION_SCHEMA_VERSION,
            "mapping": [
                {"reactant": h[0], "product": product_h[0]},
                {"reactant": h[1], "product": product_h[1]},
                {"reactant": o[0], "product": product_o[0]},
                {"reactant": o[1], "product": product_o[1]}
            ],
            "operations": [
                {"kind":"reconfigure_electrons","atom":o[0],"before":[0,4,0],"after":[0,4,2]},
                {"kind":"reconfigure_electrons","atom":o[0],"before":[0,4,2],"after":[0,4,0]},
                {"kind":"cleave_covalent","edge":[h[0],h[1],"single"],"allocation":"homolytic",
                 "before":{"left":[0,0,0],"right":[0,0,0]},"after":{"left":[0,1,1],"right":[0,1,1]}},
                {"kind":"change_covalent","edge":[o[0],o[1]],"old_order":"double","new_order":"single","allocation":"homolytic",
                 "before":{"left":[0,4,0],"right":[0,4,0]},"after":{"left":[0,5,1],"right":[0,5,1]}},
                {"kind":"form_covalent","edge":[h[0],o[0],"single"],"electron_contribution":{"left":1,"right":1},
                 "before":{"left":[0,1,1],"right":[0,5,1]},"after":{"left":[0,0,0],"right":[0,4,0]}},
                {"kind":"form_covalent","edge":[h[1],o[1],"single"],"electron_contribution":{"left":1,"right":1},
                 "before":{"left":[0,1,1],"right":[0,5,1]},"after":{"left":[0,0,0],"right":[0,4,0]}},
                {"kind":"assign_product","atoms":[h[0],h[1],o[0],o[1]],"product":product_instance}
            ]
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&response).expect("response JSON"),
        )
        .expect("response contract")
    }

    #[test]
    fn proposed_structure_unlocks_full_escalated_animation() {
        let trusted = trusted();
        let outcome = hydrogen_peroxide_outcome(&trusted);
        assert!(
            !outcome.products_without_structure().is_empty(),
            "test premise: H2O2 must be absent from the reviewed structure library"
        );

        let structures = hydrogen_peroxide_structure();
        let request =
            crate::structure_proposal_request(&outcome, &trusted).expect("structure request");
        let adopted = crate::adopt_proposed_structures(&outcome, &request, &structures, &trusted)
            .expect("proposed structure crosses catalogue validation");
        assert!(adopted.outcome.products_without_structure().is_empty());
        assert!(
            !adopted
                .bundle
                .document()
                .valence_premises
                .iter()
                .any(|premise| {
                    premise.supported_states.iter().any(|state| {
                        state.element == "O"
                            && state.formal_charge == 0
                            && state.non_bonding_electrons == 4
                            && state.unpaired_electrons == 2
                            && state.covalent_bond_order_sum == 2
                    })
                }),
            "the radical transition must be admitted by mechanism-time derivation, not pre-authored"
        );
        let mechanism = hydrogen_peroxide_mechanism(&adopted);

        let mut provider = FakeProvider {
            responses: VecDeque::from([mechanism.clone()]),
            structure_responses: VecDeque::from([structures.clone()]),
            diagnostics: Vec::new(),
            structure_diagnostics: Vec::new(),
        };
        let result = derive_mechanism(outcome.clone(), &trusted, &mut provider);
        let MechanismEscalationOutcome::Animated(animated) = result else {
            panic!("expected escalated animation: {result:?}")
        };
        assert!(!animated.frames().frames().is_empty());
        assert_eq!(animated.structure_repair_count(), 0);
        assert_eq!(animated.total_repair_count(), 0);
        assert_eq!(
            animated.frames().trust(),
            chem_kernel::DerivationTrust::ReviewCandidate
        );
        assert!(
            animated
                .static_outcome()
                .products_without_structure()
                .is_empty(),
            "the adopted product must carry its validated structure"
        );
        assert_eq!(provider.structure_diagnostics, [None]);
        assert_eq!(provider.diagnostics, [None]);

        // The cached-recipe replay must revalidate through the identical path.
        let replayed = validate_escalated_response_with_structures(
            outcome,
            Some(&structures),
            &mechanism,
            &trusted,
        )
        .expect("cached escalation with structures revalidates");
        assert!(!replayed.frames().frames().is_empty());
    }
}
