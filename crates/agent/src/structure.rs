//! Structure escalation: adopting model-proposed structural graphs for
//! claimed species absent from the reviewed structure library.
//!
//! A proposal never touches the trusted catalogue. It is compiled into an
//! isolated `Working` catalogue bundle that contains the complete trusted
//! document plus the proposed structures under one provisional premise, then
//! crosses the identical catalogue validation (graph integrity, formula
//! agreement, supported valence states) every reviewed structure crossed.

use std::{collections::BTreeMap, collections::BTreeSet, str::FromStr};

use chem_catalogue::{
    AtomRecord, BondOrderRecord, BondRecord, CatalogueEnvelope, ElementValenceRecord,
    MetallicDomainRecord, MetallicValenceStateRecord, PremiseRecord, PublicationKind,
    ReviewMetadata, ReviewStatus, StructureRecord, ValencePremiseRecord, ValenceStateRecord,
    ValidatedCatalogueBundle,
};
use chem_domain::{ContentDigest, FormulaComposition, Phase, PremiseId, SpeciesId, StructureId};

use crate::{
    AgentError, AgentErrorKind, LabelledStructure, OutcomeSpecies, StructureProposalRequest,
    StructureProposalResponse, StructureProposalSpecies, ValidatedStaticOutcome,
    claim::STRUCTURE_PROPOSAL_SCHEMA_VERSION, identity::model_proposed_species,
};

pub(crate) const DYNAMIC_STRUCTURE_PREMISE: &str = "premise.dynamic.structure";
pub(crate) const GENERATED_STRUCTURE_PREMISE: &str = "premise.generated.structure";

/// An outcome whose formula-only species acquired validated model-proposed
/// structures, together with the isolated working bundle those structures
/// live in. The bundle is required for kernel expansion and never replaces
/// the trusted catalogue.
#[derive(Debug, Clone)]
pub struct AdoptedProposedStructures {
    pub outcome: ValidatedStaticOutcome,
    pub bundle: ValidatedCatalogueBundle,
}

#[derive(Debug, Clone, Copy)]
enum MissingSide {
    Reactant,
    Product,
}

#[derive(Debug, Clone)]
struct MissingSpecies {
    side: MissingSide,
    index: usize,
    species: SpeciesId,
    name: String,
    formula: String,
    phase: Phase,
}

struct DerivedProvisionalStates {
    neutral_valence: Vec<ElementValenceRecord>,
    supported_states: Vec<ValenceStateRecord>,
    metallic_domain_states: Vec<MetallicValenceStateRecord>,
}

fn missing_species(outcome: &ValidatedStaticOutcome) -> Vec<MissingSpecies> {
    fn append_missing(
        missing: &mut Vec<MissingSpecies>,
        species: &[OutcomeSpecies],
        side: MissingSide,
    ) {
        missing.extend(
            species
                .iter()
                .enumerate()
                .filter_map(|(index, product)| match product {
                    OutcomeSpecies::Resolved(species) if species.structure.is_none() => {
                        Some(MissingSpecies {
                            side,
                            index,
                            species: species.id.clone(),
                            name: species.display_name.clone(),
                            formula: species.formula_text.clone(),
                            phase: species.phase,
                        })
                    }
                    OutcomeSpecies::Resolved(_) => None,
                    OutcomeSpecies::FormulaOnly {
                        id,
                        display_name,
                        formula,
                        phase,
                    } => Some(MissingSpecies {
                        side,
                        index,
                        species: id.clone(),
                        name: display_name.clone(),
                        formula: formula.clone(),
                        phase: *phase,
                    }),
                }),
        );
    }
    let mut missing = Vec::new();
    append_missing(&mut missing, outcome.reactants(), MissingSide::Reactant);
    append_missing(&mut missing, outcome.products(), MissingSide::Product);
    missing
}

fn proposal_species(missing: &[MissingSpecies]) -> Vec<StructureProposalSpecies> {
    missing
        .iter()
        .enumerate()
        .map(|(ordinal, species)| StructureProposalSpecies {
            id: format!("DynamicStructure{}", ordinal + 1),
            name: species.name.clone(),
            formula: species.formula.clone(),
        })
        .collect()
}

/// Builds the fixed structure-escalation request for an outcome, or `None`
/// when every species already has a reviewed structure.
#[must_use]
pub fn structure_proposal_request(
    outcome: &ValidatedStaticOutcome,
    catalogue: &ValidatedCatalogueBundle,
) -> Option<StructureProposalRequest> {
    let missing = missing_species(outcome);
    if missing.is_empty() {
        return None;
    }
    let elements = missing
        .iter()
        .filter_map(|species| FormulaComposition::parse(&species.formula).ok())
        .flat_map(|formula| {
            formula
                .elements()
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
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
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let metallic_states = catalogue
        .document()
        .valence_premises
        .iter()
        .flat_map(|premise| premise.metallic_domain_states.iter())
        .filter(|state| elements.contains(&state.element))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(StructureProposalRequest {
        schema_version: STRUCTURE_PROPOSAL_SCHEMA_VERSION,
        species: proposal_species(&missing),
        neutral_valence,
        supported_states,
        metallic_states,
        provisional_states_allowed: true,
    })
}

/// Validates a structure proposal inside an isolated working bundle and
/// upgrades the outcome's formula-only species to resolved species carrying
/// the validated graphs.
///
/// # Errors
///
/// Returns a typed diagnostic when the response does not answer the request
/// exactly, or when any proposed structure fails catalogue validation,
/// element-inventory agreement, or neutral-charge binding. The diagnostic is
/// suitable for one bounded proposal repair.
#[allow(clippy::too_many_lines)]
pub fn adopt_proposed_structures(
    outcome: &ValidatedStaticOutcome,
    request: &StructureProposalRequest,
    response: &StructureProposalResponse,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<AdoptedProposedStructures, AgentError> {
    let missing = missing_species(outcome);
    if missing.len() != request.species.len() {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "structure adoption",
            "request does not describe this outcome's missing species",
        ));
    }
    if request.species != proposal_species(&missing) {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "structure adoption",
            "request does not exactly describe this outcome's missing species",
        ));
    }
    response.validate_wire()?;
    let premise_id = PremiseId::from_str(DYNAMIC_STRUCTURE_PREMISE).map_err(|error| {
        AgentError::from_source(
            AgentErrorKind::InvalidProviderOutput,
            "structure adoption",
            error,
        )
    })?;
    let mut proposals = BTreeMap::new();
    for structure in &response.structures {
        let (id, _) = labelled_id_formula(structure);
        if proposals.insert(id.to_owned(), structure).is_some() {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure adoption",
                format!("duplicate proposed structure id `{id}`"),
            ));
        }
    }
    if proposals.len() != request.species.len() {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "structure adoption",
            "the proposal must contain exactly one structure per requested species",
        ));
    }
    let mut records = Vec::new();
    for species in &request.species {
        let proposal = *proposals.get(&species.id).ok_or_else(|| {
            AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure adoption",
                format!(
                    "no proposed structure uses the requested id `{}`",
                    species.id
                ),
            )
        })?;
        let (_, formula) = labelled_id_formula(proposal);
        if *formula != species.formula {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure adoption",
                format!(
                    "proposed structure `{}` must keep the requested formula `{}`",
                    species.id, species.formula
                ),
            ));
        }
        records.push(structure_record(proposal, &premise_id)?);
    }

    let bundle = validated_working_bundle(records, &response.structures, &premise_id, catalogue)?;

    let mut reactants = outcome.reactants().to_vec();
    let mut products = outcome.products().to_vec();
    for (missing_species, species_request) in missing.iter().zip(&request.species) {
        let structure_id = StructureId::from_str(&species_request.id).map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::InvalidProviderOutput,
                "structure adoption",
                error,
            )
        })?;
        let structure = bundle.structures().get(&structure_id).ok_or_else(|| {
            AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure adoption",
                "validated working bundle lost a proposed structure",
            )
        })?;
        if structure.graph().system_net_charge() != 0 {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure validation",
                format!(
                    "proposed structure `{}` must be net neutral to bind the balanced product",
                    species_request.id
                ),
            ));
        }
        let claimed = FormulaComposition::parse(&species_request.formula).map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::InvalidProviderOutput,
                "structure validation",
                error,
            )
        })?;
        if claimed.elements() != structure.formula().elements() {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure validation",
                format!(
                    "proposed structure `{}` does not contain exactly the atoms of `{}`",
                    species_request.id, species_request.formula
                ),
            ));
        }
        let resolved = model_proposed_species(
            &missing_species.species,
            &missing_species.name,
            &missing_species.formula,
            missing_species.phase,
            structure,
            &bundle,
        )?;
        match missing_species.side {
            MissingSide::Reactant => {
                reactants[missing_species.index] = OutcomeSpecies::Resolved(Box::new(resolved));
            }
            MissingSide::Product => {
                products[missing_species.index] = OutcomeSpecies::Resolved(Box::new(resolved));
            }
        }
    }
    let outcome = outcome.clone().with_adopted_species(reactants, products)?;
    Ok(AdoptedProposedStructures { outcome, bundle })
}

/// Builds a validated working bundle carrying the given structure records on
/// top of the catalogue, deriving any provisional valence states the new
/// structures need.
fn validated_working_bundle(
    records: Vec<chem_catalogue::StructureRecord>,
    labelled: &[LabelledStructure],
    premise_id: &PremiseId,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<ValidatedCatalogueBundle, AgentError> {
    let mut document = catalogue.document().clone();
    document.publication = PublicationKind::Working;
    // The premise must reference resolvable evidence. The trusted document's
    // internal explanatory-model source is the honest anchor: a proposed
    // structure is a modeling assumption, not an empirical claim.
    let evidence = document
        .evidence
        .first()
        .map(|source| source.id.clone())
        .into_iter()
        .collect::<BTreeSet<_>>();
    document.premises.push(PremiseRecord {
        id: premise_id.clone(),
        statement: "Model-proposed dynamic structure awaiting independent review".to_owned(),
        evidence,
        review: ReviewMetadata {
            status: ReviewStatus::Provisional,
            reviewers: Vec::new(),
        },
        rule_version: "1".to_owned(),
    });
    let DerivedProvisionalStates {
        neutral_valence,
        supported_states,
        metallic_domain_states,
    } = derive_provisional_structure_states(labelled, &document.valence_premises)?;
    if !supported_states.is_empty() || !metallic_domain_states.is_empty() {
        document.valence_premises.push(ValencePremiseRecord {
            premise_id: premise_id.clone(),
            neutral_valence,
            supported_states,
            metallic_domain_states,
        });
    }
    document.structures.extend(records);
    let mut envelope = CatalogueEnvelope {
        digest: ContentDigest::sha256(b"uncomputed dynamic working bundle"),
        bundle: document,
    };
    envelope.digest = envelope.computed_digest().map_err(|error| {
        AgentError::from_source(
            AgentErrorKind::InvalidProviderOutput,
            "structure adoption",
            error,
        )
    })?;
    ValidatedCatalogueBundle::validate(envelope).map_err(|error| {
        AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "structure validation",
            format!("proposed structure failed catalogue validation: {error}"),
        )
    })
}

/// Extends the catalogue with any outcome structures it does not already
/// carry (programmatically generated identities), so mechanism compilation
/// can premise them exactly like adopted proposals.
///
/// # Errors
///
/// Returns an error when a generated structure fails working-bundle
/// validation.
pub fn bundle_with_outcome_structures(
    outcome: &ValidatedStaticOutcome,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<ValidatedCatalogueBundle, AgentError> {
    let extra = outcome
        .reactants()
        .iter()
        .chain(outcome.products())
        .filter_map(|species| match species {
            OutcomeSpecies::Resolved(resolved) => resolved
                .structure
                .as_ref()
                .map(|structure| (structure, resolved.formula_text.clone())),
            OutcomeSpecies::FormulaOnly { .. } => None,
        })
        .filter(|(structure, _)| !catalogue.structures().contains_key(structure.id()))
        .collect::<Vec<_>>();
    if extra.is_empty() {
        return Ok(catalogue.clone());
    }
    let premise_id = PremiseId::from_str(GENERATED_STRUCTURE_PREMISE).map_err(|error| {
        AgentError::from_source(
            AgentErrorKind::InvalidProviderOutput,
            "structure adoption",
            error,
        )
    })?;
    let labelled = extra
        .iter()
        .map(|(structure, formula)| {
            strip_component_prefixes(crate::mechanism::labelled_structure(structure, formula))
        })
        .collect::<Vec<_>>();
    let records = labelled
        .iter()
        .map(|structure| structure_record(structure, &premise_id))
        .collect::<Result<Vec<_>, AgentError>>()?;
    validated_working_bundle(records, &labelled, &premise_id, catalogue)
}

/// Catalogue validation qualifies ionic component atoms as
/// `<component>.<atom>`. A graph whose ids already follow that convention
/// must be recorded with the prefixes stripped so validation reproduces the
/// identical ids.
fn strip_component_prefixes(mut structure: LabelledStructure) -> LabelledStructure {
    if let LabelledStructure::Ionic { components, .. } = &mut structure {
        for component in components {
            let prefix = format!("{}.", component.label);
            let strip = |label: &mut String| {
                if let Some(rest) = label.strip_prefix(&prefix) {
                    *label = rest.to_owned();
                }
            };
            for atom in &mut component.atoms {
                strip(&mut atom.label);
            }
            for bond in &mut component.bonds {
                strip(&mut bond.left);
                strip(&mut bond.right);
            }
        }
    }
    structure
}

#[allow(clippy::too_many_lines)]
fn derive_provisional_structure_states(
    structures: &[LabelledStructure],
    reviewed: &[ValencePremiseRecord],
) -> Result<DerivedProvisionalStates, AgentError> {
    let mut neutral = BTreeMap::<String, BTreeSet<u8>>::new();
    let mut reviewed_states = BTreeSet::new();
    let mut reviewed_metallic = BTreeSet::new();
    for premise in reviewed {
        for entry in &premise.neutral_valence {
            neutral
                .entry(entry.element.clone())
                .or_default()
                .insert(entry.neutral_valence_electrons);
        }
        reviewed_states.extend(premise.supported_states.iter().cloned());
        reviewed_metallic.extend(premise.metallic_domain_states.iter().cloned());
    }
    // Elements outside the catalogue's premises get their neutral valence
    // from the periodic table itself: it is physics, not sourced data.
    for structure in structures {
        let atoms: Vec<&AtomRecord> = match structure {
            LabelledStructure::Molecular { atoms, .. } | LabelledStructure::Ion { atoms, .. } => {
                atoms.iter().collect()
            }
            LabelledStructure::Ionic { components, .. } => components
                .iter()
                .flat_map(|component| component.atoms.iter())
                .collect(),
            LabelledStructure::Metallic { sites, .. } => sites.iter().collect(),
        };
        for atom in atoms {
            // Added alongside any reviewed values: generated structures use
            // the plain periodic-table valence, which can differ from a
            // reviewed transition-metal convention.
            if let Some(electrons) = chem_domain::valence_electrons_of(&atom.element) {
                neutral
                    .entry(atom.element.clone())
                    .or_default()
                    .insert(electrons);
            }
        }
    }
    let mut provisional = BTreeSet::new();
    let mut provisional_metallic = BTreeSet::new();
    let mut used_neutral = BTreeMap::new();
    for structure in structures {
        match structure {
            LabelledStructure::Molecular { atoms, bonds, .. }
            | LabelledStructure::Ion { atoms, bonds, .. } => derive_component_states(
                atoms,
                bonds,
                &neutral,
                &reviewed_states,
                &mut provisional,
                &mut used_neutral,
            )?,
            LabelledStructure::Ionic { components, .. } => {
                for component in components {
                    derive_component_states(
                        &component.atoms,
                        &component.bonds,
                        &neutral,
                        &reviewed_states,
                        &mut provisional,
                        &mut used_neutral,
                    )?;
                }
            }
            LabelledStructure::Metallic { sites, domains, .. } => {
                derive_metallic_states(
                    sites,
                    domains,
                    &reviewed_metallic,
                    &mut provisional_metallic,
                )?;
            }
        }
    }
    if provisional.is_empty() && !provisional_metallic.is_empty() {
        let reviewed_anchor = reviewed_states
            .iter()
            .find(|state| {
                provisional_metallic
                    .iter()
                    .any(|metallic| metallic.element == state.element)
            })
            .ok_or_else(|| {
                AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "provisional valence",
                    "a provisional metallic state has no reviewed covalent anchor",
                )
            })?;
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
                    AgentErrorKind::InvalidProviderOutput,
                    "provisional valence",
                    "reviewed metallic anchor violates its neutral-valence premise",
                )
            })?;
        used_neutral.insert(reviewed_anchor.element.clone(), neutral_electrons);
        provisional.insert(reviewed_anchor.clone());
    }
    let neutral_valence = used_neutral
        .into_iter()
        .map(
            |(element, neutral_valence_electrons)| ElementValenceRecord {
                neutral_valence_electrons,
                element,
            },
        )
        .collect();
    Ok(DerivedProvisionalStates {
        neutral_valence,
        supported_states: provisional.into_iter().collect(),
        metallic_domain_states: provisional_metallic.into_iter().collect(),
    })
}

fn derive_component_states(
    atoms: &[AtomRecord],
    bonds: &[BondRecord],
    neutral: &BTreeMap<String, BTreeSet<u8>>,
    reviewed: &BTreeSet<ValenceStateRecord>,
    provisional: &mut BTreeSet<ValenceStateRecord>,
    used_neutral: &mut BTreeMap<String, u8>,
) -> Result<(), AgentError> {
    let mut bond_sums = atoms
        .iter()
        .map(|atom| (atom.label.as_str(), 0_u8))
        .collect::<BTreeMap<_, _>>();
    for bond in bonds {
        let order = match bond.order {
            BondOrderRecord::Single => 1,
            BondOrderRecord::Double => 2,
            BondOrderRecord::Triple => 3,
        };
        for label in [&bond.left, &bond.right] {
            let sum = bond_sums.get_mut(label.as_str()).ok_or_else(|| {
                AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "provisional valence",
                    format!("bond endpoint `{label}` is not a proposed atom"),
                )
            })?;
            *sum = sum.checked_add(order).ok_or_else(|| {
                AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "provisional valence",
                    "covalent bond-order sum overflow",
                )
            })?;
        }
    }
    for atom in atoms {
        if atom.unpaired_electrons > atom.non_bonding_electrons {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "provisional valence",
                format!(
                    "atom `{}` has more unpaired than non-bonding electrons",
                    atom.label
                ),
            ));
        }
        let neutral_candidates = neutral.get(&atom.element).ok_or_else(|| {
            AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "provisional valence",
                format!("atom `{}` has no reviewed neutral valence", atom.label),
            )
        })?;
        let bond_sum = bond_sums[atom.label.as_str()];
        let neutral_electrons = neutral_candidates.iter().copied().find(|neutral| {
            i16::from(*neutral) - i16::from(atom.non_bonding_electrons) - i16::from(bond_sum)
                == atom.formal_charge
        });
        let Some(neutral_electrons) = neutral_electrons else {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "provisional valence",
                format!(
                    "atom `{}` violates formal-charge identity for every reviewed neutral valence",
                    atom.label
                ),
            ));
        };
        if used_neutral
            .insert(atom.element.clone(), neutral_electrons)
            .is_some_and(|existing| existing != neutral_electrons)
        {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "provisional valence",
                format!(
                    "proposed states require conflicting neutral valence for `{}`",
                    atom.element
                ),
            ));
        }
        let state = ValenceStateRecord {
            element: atom.element.clone(),
            formal_charge: atom.formal_charge,
            non_bonding_electrons: atom.non_bonding_electrons,
            unpaired_electrons: atom.unpaired_electrons,
            covalent_bond_order_sum: bond_sum,
        };
        if !reviewed.contains(&state) {
            provisional.insert(state);
        }
    }
    Ok(())
}

fn derive_metallic_states(
    sites: &[AtomRecord],
    domains: &[MetallicDomainRecord],
    reviewed: &BTreeSet<MetallicValenceStateRecord>,
    provisional: &mut BTreeSet<MetallicValenceStateRecord>,
) -> Result<(), AgentError> {
    let by_label = sites
        .iter()
        .map(|site| (site.label.as_str(), site))
        .collect::<BTreeMap<_, _>>();
    for domain in domains {
        let site_count = u32::try_from(domain.sites.len()).map_err(|_| {
            AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "provisional valence",
                "metallic site overflow",
            )
        })?;
        if site_count == 0
            || domain.delocalized_electrons == 0
            || domain.delocalized_electrons % site_count != 0
        {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "provisional valence",
                format!(
                    "metallic domain `{}` has inconsistent site electrons",
                    domain.label
                ),
            ));
        }
        let per_site = domain.delocalized_electrons / site_count;
        let expected_site_charge = i16::try_from(per_site).map_err(|_| {
            AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "provisional valence",
                format!("metallic domain `{}` site charge overflow", domain.label),
            )
        })?;
        for label in &domain.sites {
            let site = by_label.get(label.as_str()).ok_or_else(|| {
                AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "provisional valence",
                    format!("metallic site `{label}` does not resolve"),
                )
            })?;
            if site.non_bonding_electrons != 0 {
                return Err(AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "provisional valence",
                    format!("metallic site `{label}` must have zero local electrons"),
                ));
            }
            if site.formal_charge != expected_site_charge {
                return Err(AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "provisional valence",
                    format!(
                        "metallic site `{label}` formal_charge must equal its {per_site} delocalized electrons"
                    ),
                ));
            }
            let state = MetallicValenceStateRecord {
                element: site.element.clone(),
                site_formal_charge: site.formal_charge,
                site_local_electrons: site.non_bonding_electrons,
                delocalized_electrons_per_site: per_site,
            };
            if !reviewed.contains(&state) {
                provisional.insert(state);
            }
        }
    }
    Ok(())
}

const fn labelled_id_formula(structure: &LabelledStructure) -> (&String, &String) {
    match structure {
        LabelledStructure::Molecular { id, formula, .. }
        | LabelledStructure::Ion { id, formula, .. }
        | LabelledStructure::Ionic { id, formula, .. }
        | LabelledStructure::Metallic { id, formula, .. } => (id, formula),
    }
}

fn structure_record(
    proposal: &LabelledStructure,
    premise_id: &PremiseId,
) -> Result<StructureRecord, AgentError> {
    let structure_id = |id: &str| {
        StructureId::from_str(id).map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::InvalidProviderOutput,
                "structure adoption",
                error,
            )
        })
    };
    Ok(match proposal.clone() {
        LabelledStructure::Molecular {
            id,
            formula,
            atoms,
            bonds,
            groups,
        } => StructureRecord::Molecular {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            atoms,
            bonds,
            groups,
            traits: Vec::new(),
        },
        LabelledStructure::Ion {
            id,
            formula,
            atoms,
            bonds,
            groups,
        } => StructureRecord::Ion {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            atoms,
            bonds,
            groups,
            traits: Vec::new(),
        },
        LabelledStructure::Ionic {
            id,
            formula,
            components,
            associations,
        } => StructureRecord::Ionic {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            components,
            associations,
            traits: Vec::new(),
        },
        LabelledStructure::Metallic {
            id,
            formula,
            sites,
            domains,
        } => StructureRecord::Metallic {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            sites,
            domains,
            traits: Vec::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copper_premise() -> ValencePremiseRecord {
        ValencePremiseRecord {
            premise_id: PremiseId::from_str("premise.test.copper").expect("premise id"),
            neutral_valence: vec![ElementValenceRecord {
                element: "Cu".to_owned(),
                neutral_valence_electrons: 11,
            }],
            supported_states: vec![ValenceStateRecord {
                element: "Cu".to_owned(),
                formal_charge: 0,
                non_bonding_electrons: 11,
                unpaired_electrons: 11,
                covalent_bond_order_sum: 0,
            }],
            metallic_domain_states: vec![MetallicValenceStateRecord {
                element: "Cu".to_owned(),
                site_formal_charge: 11,
                site_local_electrons: 0,
                delocalized_electrons_per_site: 11,
            }],
        }
    }

    fn copper_structure(formal_charge: i16) -> StructureProposalResponse {
        StructureProposalResponse {
            schema_version: crate::claim::STRUCTURE_PROPOSAL_SCHEMA_VERSION,
            structures: vec![LabelledStructure::Metallic {
                id: "DynamicStructure1".to_owned(),
                formula: "Cu".to_owned(),
                sites: vec![AtomRecord {
                    label: "cu1".to_owned(),
                    element: "Cu".to_owned(),
                    formal_charge,
                    non_bonding_electrons: 0,
                    unpaired_electrons: 0,
                }],
                domains: vec![MetallicDomainRecord {
                    label: "metal1".to_owned(),
                    sites: vec!["cu1".to_owned()],
                    delocalized_electrons: 11,
                }],
            }],
        }
    }

    #[test]
    fn metallic_sites_use_domain_valence_instead_of_covalent_identity() {
        let derived = derive_provisional_structure_states(
            &copper_structure(11).structures,
            &[copper_premise()],
        )
        .expect("reviewed copper metal state");
        assert!(derived.neutral_valence.is_empty());
        assert!(derived.supported_states.is_empty());
        assert!(derived.metallic_domain_states.is_empty());
    }

    #[test]
    fn metallic_site_charge_must_balance_its_domain_electrons() {
        let Err(error) = derive_provisional_structure_states(
            &copper_structure(0).structures,
            &[copper_premise()],
        ) else {
            panic!("neutral local site charge cannot balance the electron domain");
        };
        assert!(
            error
                .to_string()
                .contains("formal_charge must equal its 11")
        );
    }
}
