use std::{collections::BTreeMap, str::FromStr};

use chem_catalogue::{
    ExplosiveWaterContactVariantRecord, ValidatedCatalogueBundle, WaterContactBehaviourRecord,
};
use chem_domain::{
    BronstedAcidProfile, Charge, ContentDigest, ElementInventory, ElementSymbol,
    ExternalIdentifier, FormulaComposition, Phase, ReactionDeclaration, RepresentationKind,
    ResolvedSpecies, SpeciesAmbiguity, SpeciesId, SpeciesQuery, SpeciesRegistry, SpeciesResolution,
    StructureDefinition, StructureId, UnbalancedReactionTerm, classify_bronsted_acid,
    generate_structure, symbol_of,
};
use num_bigint::BigUint;

use crate::{
    AgentError, AgentErrorKind, ClaimDisposition, ClaimIdentityHint, ClaimIdentityHintKind,
    ClaimInput, ClaimObservationPredicate, ClaimPhase, ClaimProduct, ProviderClaim,
    ReactionBuildRequest, ReactionClaim,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeProvenance {
    Reviewed,
    Derived,
    ModelAsserted,
}

/// Deterministic macroscopic process established from checked reaction
/// structure and phase data. Presentation may consume this classification;
/// it must not rediscover the process from names or renderer assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroscopicProcess {
    /// Two structurally validated soluble ionic reactants exchange partners,
    /// producing one exact solid product in an aqueous mobile phase.
    AqueousPrecipitation,
    /// Two validated mobile reactants generate one exact gaseous product.
    GasEvolutionLiquidLiquid,
    /// One validated solid reactant and one mobile reactant generate one exact
    /// gaseous product.
    GasEvolutionSolidLiquid,
    /// A solid metal transfers into an aqueous ionic product while the
    /// solution's original metal cation becomes a different solid metal.
    MetalDisplacement,
    /// A reviewed exact-material water-contact capability with an exact
    /// solid-metal/liquid-water/aqueous-ion/gas layout.
    ExplosiveMetalWater(ExplosiveWaterContactVariantRecord),
    /// Exactly two validated solid reactants combine into one validated solid
    /// product after more-specific macroscopic processes have been excluded.
    SolidSolidSynthesis,
    /// Exactly one typed solid and one typed gaseous reactant combine into one
    /// gaseous product.
    SolidGasSynthesis,
    /// Exactly two typed gaseous reactants combine into one gaseous product.
    GasGasSynthesis,
    CompleteCombustion,
    /// A validated C/H(/O) fuel reacts with dioxygen and carbon monoxide is
    /// one of the exact gaseous products.
    IncompleteCombustion,
    SolventEvaporationCrystallization,
    /// A solid metallic reactant combines with gaseous dioxygen to form a
    /// validated solid product at the exposed metal surface.
    SurfaceOxidation,
}

/// Conservative educational colour families for simple hydrated ions.
///
/// This is intentionally a small closed set. Ligand-dependent or
/// concentration-dependent colours are left unknown so presentation falls
/// back to colourless unless `.chems` or reviewed catalogue data is more
/// specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroscopicColour {
    White,
    Cream,
    Yellow,
    Blue,
    RedBrown,
    PaleBlue,
    PaleGreen,
    YellowBrown,
    Pink,
    Green,
    CopperMetal,
    GoldMetal,
}

impl MacroscopicColour {
    /// Stylised sRGB representative used by the macroscopic renderer.
    #[must_use]
    pub const fn srgb(self) -> [u8; 3] {
        match self {
            Self::White => [0xf0, 0xf5, 0xfa],
            Self::Cream => [0xf0, 0xe0, 0xad],
            Self::Yellow => [0xef, 0xd1, 0x47],
            Self::Blue => [0x4f, 0x92, 0xd0],
            Self::RedBrown => [0xa6, 0x4b, 0x32],
            Self::PaleBlue => [0x63, 0x9d, 0xd0],
            Self::PaleGreen => [0x8d, 0xb1, 0x83],
            Self::YellowBrown => [0xc4, 0x91, 0x48],
            Self::Pink => [0xd1, 0x8d, 0xa5],
            Self::Green => [0x74, 0xa2, 0x78],
            Self::CopperMetal => [0xb8, 0x6a, 0x47],
            Self::GoldMetal => [0xd4, 0xaf, 0x37],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutcomeSpecies {
    Resolved(Box<ResolvedSpecies>),
    FormulaOnly {
        id: SpeciesId,
        display_name: String,
        formula: String,
        phase: Phase,
    },
}

impl OutcomeSpecies {
    #[must_use]
    pub const fn has_structure(&self) -> bool {
        matches!(self, Self::Resolved(species) if species.structure.is_some())
    }

    #[must_use]
    pub fn id(&self) -> &SpeciesId {
        match self {
            Self::Resolved(species) => &species.id,
            Self::FormulaOnly { id, .. } => id,
        }
    }

    /// Returns a structure-derived proton-donor profile when this species has
    /// crossed structural validation. Formula-only identities deliberately do
    /// not gain an acid classification.
    #[must_use]
    pub fn bronsted_acid_profile(&self) -> Option<BronstedAcidProfile> {
        match self {
            Self::Resolved(species) => species.structure.as_ref().map(classify_bronsted_acid),
            Self::FormulaOnly { .. } => None,
        }
    }

    #[must_use]
    pub const fn phase(&self) -> Phase {
        match self {
            Self::Resolved(species) => species.phase,
            Self::FormulaOnly { phase, .. } => *phase,
        }
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        match self {
            Self::Resolved(species) => &species.display_name,
            Self::FormulaOnly { display_name, .. } => display_name,
        }
    }

    #[must_use]
    pub fn representation(&self) -> Option<RepresentationKind> {
        match self {
            Self::Resolved(species) => species
                .structure
                .as_ref()
                .map(StructureDefinition::representation),
            Self::FormulaOnly { .. } => None,
        }
    }
}

/// Structurally checked static capability. It deliberately exposes no frame
/// construction or playback API.
#[derive(Debug, Clone)]
pub struct ValidatedStaticOutcome {
    declaration: ReactionDeclaration,
    reactants: Vec<OutcomeSpecies>,
    products: Vec<OutcomeSpecies>,
    macroscopic_phases: Box<MacroscopicPhases>,
    claim: ReactionClaim,
    claim_provenance: OutcomeProvenance,
    equation: String,
    macroscopic_process: Option<MacroscopicProcess>,
}

#[derive(Debug, Clone, Default)]
struct MacroscopicPhases {
    reactants: BTreeMap<SpeciesId, Phase>,
    products: BTreeMap<SpeciesId, Phase>,
}

impl ValidatedStaticOutcome {
    #[must_use]
    pub const fn declaration(&self) -> &ReactionDeclaration {
        &self.declaration
    }

    #[must_use]
    pub fn reactants(&self) -> &[OutcomeSpecies] {
        &self.reactants
    }

    #[must_use]
    pub fn products(&self) -> &[OutcomeSpecies] {
        &self.products
    }

    #[must_use]
    pub const fn claim(&self) -> &ReactionClaim {
        &self.claim
    }

    #[must_use]
    pub const fn claim_provenance(&self) -> OutcomeProvenance {
        self.claim_provenance
    }

    /// Recovers the cacheable provider capability only when this outcome
    /// originated at the provider boundary. Solver outcomes deliberately
    /// cannot be converted into provider claims.
    #[must_use]
    pub fn provider_claim(&self) -> Option<ProviderClaim> {
        ProviderClaim::from_compiled(self.claim.clone())
    }

    #[must_use]
    pub fn equation(&self) -> &str {
        &self.equation
    }

    #[must_use]
    pub const fn macroscopic_process(&self) -> Option<MacroscopicProcess> {
        self.macroscopic_process
    }

    /// Exact carbon count of the validated C/H(/O) fuel for a combustion
    /// process. Presentation may use this composition fact for a generic fuel
    /// palette without parsing names or formula strings.
    #[must_use]
    pub fn combustion_fuel_carbon_count(&self) -> Option<u64> {
        if !matches!(
            self.macroscopic_process,
            Some(MacroscopicProcess::CompleteCombustion | MacroscopicProcess::IncompleteCombustion)
        ) {
            return None;
        }
        let [first, second] = self.declaration.reactants() else {
            return None;
        };
        let fuel = if is_dioxygen(first) {
            second
        } else if is_dioxygen(second) {
            first
        } else {
            return None;
        };
        fuel.formula()
            .elements()
            .iter()
            .find(|(element, _)| element.as_str() == "C")
            .map(|(_, count)| *count)
    }

    /// Returns the exact claimed product phase after static validation, plus
    /// process-established reactant phase where the closed classification
    /// proves it. Other unknown phases remain unknown.
    #[must_use]
    pub fn macroscopic_phase(&self, species: &OutcomeSpecies) -> Phase {
        if let Some(index) = self
            .products
            .iter()
            .position(|product| product.id() == species.id())
        {
            let resolved_phase = self
                .macroscopic_phases
                .products
                .get(species.id())
                .copied()
                .unwrap_or_else(|| species.phase());
            let claimed = claim_phase(self.claim.products[index].phase);
            return if claimed != Phase::Unknown {
                claimed
            } else if resolved_phase != Phase::Unknown {
                resolved_phase
            } else if self.macroscopic_process == Some(MacroscopicProcess::SolidSolidSynthesis) {
                solid_synthesis_reactant_phase(species).unwrap_or(Phase::Unknown)
            } else {
                Phase::Unknown
            };
        }
        let resolved_phase = self
            .macroscopic_phases
            .reactants
            .get(species.id())
            .copied()
            .unwrap_or_else(|| species.phase());
        let Some(term) = self
            .declaration
            .reactants()
            .iter()
            .find(|term| term.species() == species.id())
        else {
            return resolved_phase;
        };
        match self.macroscopic_process {
            Some(
                MacroscopicProcess::AqueousPrecipitation
                | MacroscopicProcess::GasEvolutionLiquidLiquid,
            ) => Phase::Aqueous,
            Some(
                MacroscopicProcess::GasEvolutionSolidLiquid | MacroscopicProcess::MetalDisplacement,
            ) => gas_evolution_reactant_phase(species).unwrap_or(resolved_phase),
            Some(MacroscopicProcess::SolidSolidSynthesis) => {
                solid_synthesis_reactant_phase(species).unwrap_or(resolved_phase)
            }
            Some(MacroscopicProcess::SolidGasSynthesis | MacroscopicProcess::GasGasSynthesis) => {
                resolved_phase
            }
            Some(
                MacroscopicProcess::CompleteCombustion
                | MacroscopicProcess::IncompleteCombustion
                | MacroscopicProcess::SurfaceOxidation,
            ) if is_dioxygen(term) => Phase::Gas,
            Some(
                MacroscopicProcess::CompleteCombustion
                | MacroscopicProcess::IncompleteCombustion
                | MacroscopicProcess::SolventEvaporationCrystallization
                | MacroscopicProcess::SurfaceOxidation
                | MacroscopicProcess::ExplosiveMetalWater(_),
            )
            | None => resolved_phase,
        }
    }

    /// Returns a conservative structure-derived colour for a simple aqueous
    /// ion or a reviewed common precipitate family. Exact `.chems`
    /// observations and reviewed catalogue RGB records remain
    /// higher-authority presentation inputs.
    #[must_use]
    pub fn macroscopic_colour(&self, species: &OutcomeSpecies) -> Option<MacroscopicColour> {
        let phase = self.macroscopic_phase(species);
        let OutcomeSpecies::Resolved(species) = species else {
            return None;
        };
        let structure = species.structure.as_ref()?;
        if phase == Phase::Solid && structure.representation() == RepresentationKind::Metallic {
            let elements = structure.formula().elements();
            if elements.len() != 1 {
                return None;
            }
            return match elements.keys().next()?.as_str() {
                "Cu" => Some(MacroscopicColour::CopperMetal),
                "Au" => Some(MacroscopicColour::GoldMetal),
                _ => None,
            };
        }
        let salt = crate::solve::ionic_salt(structure)?;
        match phase {
            Phase::Aqueous => match (salt.cation.as_str(), salt.cation_charge) {
                ("Cu", 2) => Some(MacroscopicColour::PaleBlue),
                ("Fe", 2) => Some(MacroscopicColour::PaleGreen),
                ("Fe", 3) => Some(MacroscopicColour::YellowBrown),
                ("Co", 2) => Some(MacroscopicColour::Pink),
                ("Ni", 2) => Some(MacroscopicColour::Green),
                _ => None,
            },
            Phase::Solid
                if self.macroscopic_process == Some(MacroscopicProcess::AqueousPrecipitation) =>
            {
                precipitate_colour(&salt)
            }
            Phase::Unknown | Phase::Solid | Phase::Liquid | Phase::Gas => None,
        }
    }

    #[must_use]
    pub fn species_without_structure(&self) -> Vec<String> {
        self.reactants
            .iter()
            .chain(&self.products)
            .filter(|product| !product.has_structure())
            .map(|product| match product {
                OutcomeSpecies::Resolved(species) => {
                    format!("{} ({})", species.display_name, species.formula_text)
                }
                OutcomeSpecies::FormulaOnly {
                    display_name,
                    formula,
                    ..
                } => format!("{display_name} ({formula})"),
            })
            .collect()
    }

    #[must_use]
    pub fn products_without_structure(&self) -> Vec<String> {
        self.products
            .iter()
            .filter(|product| !product.has_structure())
            .map(|product| match product {
                OutcomeSpecies::Resolved(species) => {
                    format!("{} ({})", species.display_name, species.formula_text)
                }
                OutcomeSpecies::FormulaOnly {
                    display_name,
                    formula,
                    ..
                } => format!("{display_name} ({formula})"),
            })
            .collect()
    }

    /// Replaces both sides with structurally adopted equivalents. Species ids
    /// and order must stay identical so the balanced declaration remains
    /// valid unchanged.
    pub(crate) fn with_adopted_species(
        mut self,
        reactants: Vec<OutcomeSpecies>,
        products: Vec<OutcomeSpecies>,
    ) -> Result<Self, AgentError> {
        if reactants.len() != self.reactants.len()
            || reactants
                .iter()
                .zip(&self.reactants)
                .any(|(adopted, existing)| adopted.id() != existing.id())
            || products.len() != self.products.len()
            || products
                .iter()
                .zip(&self.products)
                .any(|(adopted, existing)| adopted.id() != existing.id())
        {
            return Err(AgentError::new(
                AgentErrorKind::CompilationFailure,
                "structure adoption",
                "adopted species must preserve side, identity, and order",
            ));
        }
        self.reactants = reactants;
        self.products = products;
        let reclassified = classify_macroscopic_process(
            &self.declaration,
            &self.reactants,
            &self.products,
            &self.macroscopic_phases.reactants,
            &self.macroscopic_phases.products,
            &self.claim,
            None,
        );
        // Structural adoption preserves side, exact identity, and order above.
        // It has no catalogue handle, so retain an already catalogue-authorized
        // process capability rather than silently downgrading it to a
        // name/formula-derived fallback.
        self.macroscopic_process = reclassified.or(self.macroscopic_process);
        Ok(self)
    }

    pub(crate) fn mark_reviewed(mut self) -> Self {
        self.claim_provenance = OutcomeProvenance::Reviewed;
        self
    }
}

#[derive(Debug, Clone)]
pub enum CompiledClaimOutcome {
    Static(ValidatedStaticOutcome),
    NoReaction(ReactionClaim),
    Ambiguous(ReactionClaim),
    Unsupported(ReactionClaim),
}

#[derive(Debug, Clone)]
pub struct ReactantIdentityAmbiguity {
    pub reactant_index: usize,
    pub query: SpeciesQuery,
    pub alternatives: Vec<ResolvedSpecies>,
}

#[derive(Debug, Clone)]
pub enum RequestIdentityResolution {
    Resolved(Vec<OutcomeSpecies>),
    Ambiguous(ReactantIdentityAmbiguity),
}

/// Resolves request and claimed product identities, proves request binding,
/// balances exactly, and constructs the private static capability.
///
/// # Errors
///
/// Returns a typed error for unresolved/ambiguous reactants, request identity
/// mismatch, invalid product formulae, or any exact balance failure.
#[allow(clippy::too_many_lines)]
pub fn compile_claim_outcome(
    request: &ReactionBuildRequest,
    claim: impl Into<ClaimInput>,
    identities: &SpeciesRegistry,
) -> Result<CompiledClaimOutcome, AgentError> {
    compile_claim_outcome_inner(request, claim, identities, None)
}

/// Compiles a claim while allowing reviewed catalogue isomorphism to collapse
/// duplicate aliases of the same exact reactant structure.
///
/// # Errors
///
/// Returns the same typed validation errors as [`compile_claim_outcome`].
pub fn compile_claim_outcome_with_catalogue(
    request: &ReactionBuildRequest,
    claim: impl Into<ClaimInput>,
    identities: &SpeciesRegistry,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<CompiledClaimOutcome, AgentError> {
    compile_claim_outcome_inner(request, claim, identities, Some(catalogue))
}

#[allow(clippy::too_many_lines)]
fn compile_claim_outcome_inner(
    request: &ReactionBuildRequest,
    claim: impl Into<ClaimInput>,
    identities: &SpeciesRegistry,
    catalogue: Option<&ValidatedCatalogueBundle>,
) -> Result<CompiledClaimOutcome, AgentError> {
    let claim = claim.into();
    let solver_authored = matches!(claim, ClaimInput::Solved(_));
    let claim = claim.into_claim();
    validate_request_shape(request)?;
    if !claim.reactant_phases.is_empty() && claim.reactant_phases.len() != request.reactants.len() {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "reaction claim",
            "reactant phases must be empty for a legacy claim or contain exactly one phase for each requested reactant in request order",
        ));
    }
    validate_selected_context_binding(request, &claim)?;
    let local_aqueous_electrolysis =
        request.selected_context.as_deref() == Some("electricity") && solver_authored;
    let local_net_water_electrolysis = local_aqueous_electrolysis
        && matches!(
            claim.products.as_slice(),
            [hydrogen, oxygen] if hydrogen.formula == "H2" && oxygen.formula == "O2"
        );
    match claim.disposition {
        ClaimDisposition::NoReaction => return Ok(CompiledClaimOutcome::NoReaction(claim)),
        ClaimDisposition::Ambiguous => return Ok(CompiledClaimOutcome::Ambiguous(claim)),
        ClaimDisposition::Unsupported => return Ok(CompiledClaimOutcome::Unsupported(claim)),
        ClaimDisposition::Reaction => {}
    }
    let mut reactants = if let Some(catalogue) = catalogue {
        match resolve_request_identities_with_catalogue(request, identities, catalogue)? {
            RequestIdentityResolution::Resolved(species) => species,
            RequestIdentityResolution::Ambiguous(ambiguity) => {
                return Err(AgentError::new(
                    AgentErrorKind::IdentityFailure,
                    "request identity",
                    format!(
                        "reactant `{}` resolves to multiple identities",
                        request.reactants[ambiguity.reactant_index].display
                    ),
                ));
            }
        }
    } else {
        resolve_request_species(request, identities)?
    };
    let products = claim
        .products
        .iter()
        .map(|product| {
            let formula = ascii_formula_key(&product.formula);
            if let Some(species) = resolve_claim_product(product, &formula, identities) {
                Ok(OutcomeSpecies::Resolved(Box::new(species.clone())))
            } else if let Some(generated) =
                generated_product(product, &formula, claim_phase(product.phase))
            {
                Ok(generated)
            } else {
                let digest = generated_species_identity_digest(&product.name, &formula).to_hex();
                let id = SpeciesId::from_str(&format!("dynamic.s{}", &digest[..24])).map_err(
                    |error| {
                        AgentError::from_source(
                            AgentErrorKind::IdentityFailure,
                            "outcome identity",
                            error,
                        )
                    },
                )?;
                FormulaComposition::parse(&formula).map_err(|error| {
                    AgentError::from_source(
                        AgentErrorKind::CompilationFailure,
                        "outcome formula",
                        error,
                    )
                })?;
                Ok(OutcomeSpecies::FormulaOnly {
                    id,
                    display_name: product.name.clone(),
                    formula,
                    phase: claim_phase(product.phase),
                })
            }
        })
        .collect::<Result<Vec<_>, AgentError>>()?;
    let product_terms = products
        .iter()
        .map(outcome_term)
        .collect::<Result<Vec<_>, AgentError>>()?;
    let balance = |reactants: &[OutcomeSpecies]| {
        ReactionDeclaration::balance(
            reactants
                .iter()
                .map(outcome_term)
                .collect::<Result<Vec<_>, AgentError>>()?,
            product_terms.clone(),
            claim.required_context.clone(),
        )
        .map_err(|error| {
            AgentError::from_source(AgentErrorKind::CompilationFailure, "outcome balance", error)
        })
    };
    let declaration = match balance(&reactants) {
        Ok(declaration) => declaration,
        Err(original) if local_aqueous_electrolysis => {
            let water_claim = ClaimProduct {
                name: "Water".to_owned(),
                formula: "H2O".to_owned(),
                phase: ClaimPhase::Liquid,
                identity_hints: Vec::new(),
            };
            let water = resolve_claim_product(&water_claim, "H2O", identities)
                .map(|species| OutcomeSpecies::Resolved(Box::new(species.clone())))
                .or_else(|| generated_product(&water_claim, "H2O", Phase::Liquid))
                .ok_or(original)?;
            if local_net_water_electrolysis {
                // An active-metal oxoanion electrolyte is unchanged overall.
                // Keep it as request context and validate the net chemical
                // change instead of inventing duplicate reactant/product terms.
                reactants = vec![water];
            } else {
                reactants.push(water);
            }
            balance(&reactants)?
        }
        Err(error) => return Err(error),
    };
    let mut macroscopic_phases = MacroscopicPhases::default();
    for (index, species) in reactants.iter().enumerate() {
        let phase = resolved_macroscopic_phase(species, catalogue)
            .filter(|phase| *phase != Phase::Unknown)
            .or_else(|| {
                claim
                    .reactant_phases
                    .get(index)
                    .copied()
                    .map(claim_phase)
                    .filter(|phase| *phase != Phase::Unknown)
            });
        if let Some(phase) = phase
            && phase != Phase::Unknown
        {
            macroscopic_phases
                .reactants
                .insert(species.id().clone(), phase);
        }
    }
    for (species, product) in products.iter().zip(&claim.products) {
        let claimed = claim_phase(product.phase);
        let phase = (claimed != Phase::Unknown)
            .then_some(claimed)
            .or_else(|| resolved_macroscopic_phase(species, catalogue));
        if let Some(phase) = phase
            && phase != Phase::Unknown
        {
            macroscopic_phases
                .products
                .insert(species.id().clone(), phase);
        }
    }
    let claim_provenance = if solver_authored {
        OutcomeProvenance::Derived
    } else {
        OutcomeProvenance::ModelAsserted
    };
    let equation = format_equation(&declaration);
    let macroscopic_process = classify_macroscopic_process(
        &declaration,
        &reactants,
        &products,
        &macroscopic_phases.reactants,
        &macroscopic_phases.products,
        &claim,
        catalogue,
    );
    Ok(CompiledClaimOutcome::Static(ValidatedStaticOutcome {
        declaration,
        reactants,
        products,
        macroscopic_phases: Box::new(macroscopic_phases),
        claim,
        claim_provenance,
        equation,
        macroscopic_process,
    }))
}

fn classify_macroscopic_process(
    declaration: &ReactionDeclaration,
    reactants: &[OutcomeSpecies],
    products: &[OutcomeSpecies],
    reactant_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    product_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    claim: &ReactionClaim,
    catalogue: Option<&ValidatedCatalogueBundle>,
) -> Option<MacroscopicProcess> {
    if let Some(process) = classifies_combustion(declaration, reactants, claim) {
        return Some(process);
    }
    let [first, second] = declaration.reactants() else {
        return None;
    };
    if classifies_surface_oxidation(first, second, reactants, products, claim) {
        return Some(MacroscopicProcess::SurfaceOxidation);
    }
    if let Some(variant) = classifies_explosive_metal_water(reactants, products, catalogue) {
        return Some(MacroscopicProcess::ExplosiveMetalWater(variant));
    }
    if let Some(process) = classifies_gas_evolution(reactants, reactant_macroscopic_phases, claim) {
        return Some(process);
    }
    if classifies_aqueous_precipitation(reactants, products, claim) {
        return Some(MacroscopicProcess::AqueousPrecipitation);
    }
    if classifies_metal_displacement(
        reactants,
        products,
        reactant_macroscopic_phases,
        product_macroscopic_phases,
        claim,
    ) {
        return Some(MacroscopicProcess::MetalDisplacement);
    }

    let has_structural_acid = reactants.iter().any(|species| {
        species
            .bronsted_acid_profile()
            .is_some_and(|profile| !profile.proton_donor_sites().is_empty())
    });
    let has_ionic_base = reactants
        .iter()
        .any(|species| species.representation() == Some(RepresentationKind::Ionic));
    let liquid_water = claim
        .products
        .iter()
        .zip(products)
        .any(|(claim_product, product)| {
            claim_phase(claim_product.phase) == Phase::Liquid
                && product.representation() == Some(RepresentationKind::Molecular)
                && FormulaComposition::parse(&ascii_formula_key(&claim_product.formula))
                    .is_ok_and(|formula| has_counts(&formula, &[("H", 2), ("O", 1)]))
        });
    let dissolved_ionic_product =
        claim
            .products
            .iter()
            .zip(products)
            .any(|(claim_product, product)| {
                claim_phase(claim_product.phase) == Phase::Aqueous
                    && product.representation() == Some(RepresentationKind::Ionic)
            });
    if has_structural_acid && has_ionic_base && liquid_water && dissolved_ionic_product {
        return Some(MacroscopicProcess::SolventEvaporationCrystallization);
    }
    if classifies_solid_solid_synthesis(reactants, products, reactant_macroscopic_phases, claim) {
        return Some(MacroscopicProcess::SolidSolidSynthesis);
    }
    // The shared chem-domain core owns the hydrogen/oxygen carve-out and the
    // phase table, so this classifier cannot drift from the static one.
    classifies_phase_synthesis(
        first,
        second,
        reactants,
        products,
        reactant_macroscopic_phases,
        product_macroscopic_phases,
        claim,
    )
}

fn classifies_combustion(
    declaration: &ReactionDeclaration,
    reactants: &[OutcomeSpecies],
    claim: &ReactionClaim,
) -> Option<MacroscopicProcess> {
    let [first, second] = declaration.reactants() else {
        return None;
    };
    let (fuel, oxygen) = if is_dioxygen(first) {
        (second, first)
    } else if is_dioxygen(second) {
        (first, second)
    } else {
        return None;
    };
    let fuel_species = reactants
        .iter()
        .find(|species| species.id() == fuel.species())?;
    let oxygen_species = reactants
        .iter()
        .find(|species| species.id() == oxygen.species())?;
    if fuel_species.representation() != Some(RepresentationKind::Molecular)
        || oxygen_species.representation() != Some(RepresentationKind::Molecular)
        || !is_carbon_hydrogen_oxygen_fuel(fuel.formula())
    {
        return None;
    }
    let has_carbon_dioxide = claim.products.iter().any(|product| {
        claim_phase(product.phase) == Phase::Gas
            && FormulaComposition::parse(&ascii_formula_key(&product.formula))
                .is_ok_and(|formula| has_counts(&formula, &[("C", 1), ("O", 2)]))
    });
    let has_carbon_monoxide = claim.products.iter().any(|product| {
        claim_phase(product.phase) == Phase::Gas
            && FormulaComposition::parse(&ascii_formula_key(&product.formula))
                .is_ok_and(|formula| has_counts(&formula, &[("C", 1), ("O", 1)]))
    });
    let has_water_vapour = claim.products.iter().any(|product| {
        claim_phase(product.phase) == Phase::Gas
            && FormulaComposition::parse(&ascii_formula_key(&product.formula))
                .is_ok_and(|formula| has_counts(&formula, &[("H", 2), ("O", 1)]))
    });
    if has_carbon_monoxide {
        Some(MacroscopicProcess::IncompleteCombustion)
    } else {
        (claim.products.len() == 2 && has_carbon_dioxide && has_water_vapour)
            .then_some(MacroscopicProcess::CompleteCombustion)
    }
}

fn classifies_explosive_metal_water(
    reactants: &[OutcomeSpecies],
    products: &[OutcomeSpecies],
    catalogue: Option<&ValidatedCatalogueBundle>,
) -> Option<ExplosiveWaterContactVariantRecord> {
    let catalogue = catalogue?;
    let [first, second] = reactants else {
        return None;
    };
    let material = |species: &OutcomeSpecies| {
        let OutcomeSpecies::Resolved(species) = species else {
            return None;
        };
        let structure = species.structure.as_ref()?;
        catalogue.macroscopic_material(structure.id(), None)
    };
    let reactant_layout = |metal: &OutcomeSpecies, water: &OutcomeSpecies| {
        let metal_structure = match metal {
            OutcomeSpecies::Resolved(species) => species.structure.as_ref(),
            OutcomeSpecies::FormulaOnly { .. } => None,
        }?;
        let water_structure = match water {
            OutcomeSpecies::Resolved(species) => species.structure.as_ref(),
            OutcomeSpecies::FormulaOnly { .. } => None,
        }?;
        let Some(WaterContactBehaviourRecord::Explosive { variant }) =
            material(metal)?.water_contact
        else {
            return None;
        };
        (metal_structure.representation() == RepresentationKind::Metallic
            && material(metal)?.phase == Phase::Solid
            && water_structure.representation() == RepresentationKind::Molecular
            && inventory_has_counts(water_structure.formula(), &[("H", 2), ("O", 1)])
            && material(water)?.phase == Phase::Liquid)
            .then_some(variant)
    };
    let variant = reactant_layout(first, second).or_else(|| reactant_layout(second, first))?;
    let [first, second] = products else {
        return None;
    };
    let product_layout = |hydroxide: &OutcomeSpecies, hydrogen: &OutcomeSpecies| {
        let OutcomeSpecies::Resolved(hydrogen_species) = hydrogen else {
            return false;
        };
        let Some(hydrogen_structure) = hydrogen_species.structure.as_ref() else {
            return false;
        };
        hydroxide.representation() == Some(RepresentationKind::Ionic)
            && material(hydroxide).is_some_and(|record| record.phase == Phase::Aqueous)
            && hydrogen_structure.representation() == RepresentationKind::Molecular
            && inventory_has_counts(hydrogen_structure.formula(), &[("H", 2)])
            && material(hydrogen).is_some_and(|record| record.phase == Phase::Gas)
    };
    (product_layout(first, second) || product_layout(second, first)).then_some(variant)
}

fn classifies_solid_solid_synthesis(
    reactants: &[OutcomeSpecies],
    products: &[OutcomeSpecies],
    reactant_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    claim: &ReactionClaim,
) -> bool {
    let [first, second] = reactants else {
        return false;
    };
    let [product] = products else {
        return false;
    };
    let [claim_product] = claim.products.as_slice() else {
        return false;
    };
    effective_reactant_phase(
        first,
        reactant_macroscopic_phases,
        solid_synthesis_reactant_phase,
    ) == Some(Phase::Solid)
        && effective_reactant_phase(
            second,
            reactant_macroscopic_phases,
            solid_synthesis_reactant_phase,
        ) == Some(Phase::Solid)
        && (claim_phase(claim_product.phase) == Phase::Solid
            || (claim_phase(claim_product.phase) == Phase::Unknown
                && solid_synthesis_reactant_phase(product) == Some(Phase::Solid)))
        && product.representation().is_some()
        && !claim
            .products
            .iter()
            .any(|candidate| claim_phase(candidate.phase) == Phase::Gas)
}

fn classifies_phase_synthesis(
    first_term: &chem_domain::ReactionTerm,
    second_term: &chem_domain::ReactionTerm,
    reactants: &[OutcomeSpecies],
    products: &[OutcomeSpecies],
    reactant_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    product_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    claim: &ReactionClaim,
) -> Option<MacroscopicProcess> {
    let [first, second] = reactants else {
        return None;
    };
    let [product] = products else {
        return None;
    };
    let [claim_product] = claim.products.as_slice() else {
        return None;
    };
    let reactant_phase = |species: &OutcomeSpecies| {
        reactant_macroscopic_phases
            .get(species.id())
            .copied()
            .unwrap_or_else(|| species.phase())
    };
    let product_phase = product_macroscopic_phases
        .get(product.id())
        .copied()
        .unwrap_or_else(|| product.phase());
    let first_formula: Vec<(&str, u64)> = first_term
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str(), *count))
        .collect();
    let second_formula: Vec<(&str, u64)> = second_term
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str(), *count))
        .collect();
    let route = chem_domain::classify_phase_synthesis(
        (&first_formula, reactant_phase(first)),
        (&second_formula, reactant_phase(second)),
        {
            let claimed = claim_phase(claim_product.phase);
            if claimed == Phase::Unknown {
                product_phase
            } else {
                claimed
            }
        },
    )?;
    Some(match route {
        chem_domain::PhaseSynthesisRoute::SolidGas => MacroscopicProcess::SolidGasSynthesis,
        chem_domain::PhaseSynthesisRoute::GasGas => MacroscopicProcess::GasGasSynthesis,
    })
}

fn resolved_macroscopic_phase(
    species: &OutcomeSpecies,
    catalogue: Option<&ValidatedCatalogueBundle>,
) -> Option<Phase> {
    if species.phase() != Phase::Unknown {
        return Some(species.phase());
    }
    let OutcomeSpecies::Resolved(species) = species else {
        return None;
    };
    let structure = species.structure.as_ref()?;
    catalogue?
        .macroscopic_material(structure.id(), None)
        .map(|material| material.phase)
}

fn solid_synthesis_reactant_phase(species: &OutcomeSpecies) -> Option<Phase> {
    if species.phase() != Phase::Unknown {
        return Some(species.phase());
    }
    let OutcomeSpecies::Resolved(species) = species else {
        return None;
    };
    let structure = species.structure.as_ref()?;
    match structure.representation() {
        RepresentationKind::Metallic | RepresentationKind::Ionic => Some(Phase::Solid),
        RepresentationKind::Molecular => {
            let elements = structure.formula().elements();
            if elements.len() != 1 {
                return None;
            }
            let symbol = elements.keys().next()?.as_str();
            match symbol {
                "H" | "N" | "O" | "F" | "Cl" | "He" | "Ne" | "Ar" | "Kr" | "Xe" | "Rn" => {
                    Some(Phase::Gas)
                }
                "Br" => Some(Phase::Liquid),
                _ => Some(Phase::Solid),
            }
        }
        RepresentationKind::Ion => None,
    }
}

fn classifies_gas_evolution(
    reactants: &[OutcomeSpecies],
    reactant_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    claim: &ReactionClaim,
) -> Option<MacroscopicProcess> {
    let [first, second] = reactants else {
        return None;
    };
    if claim
        .products
        .iter()
        .filter(|product| claim_phase(product.phase) == Phase::Gas)
        .count()
        != 1
        || !claim
            .observations
            .iter()
            .any(|observation| observation.predicate == ClaimObservationPredicate::Evolves)
    {
        return None;
    }
    match [
        effective_reactant_phase(
            first,
            reactant_macroscopic_phases,
            gas_evolution_reactant_phase,
        )?,
        effective_reactant_phase(
            second,
            reactant_macroscopic_phases,
            gas_evolution_reactant_phase,
        )?,
    ] {
        [
            Phase::Aqueous | Phase::Liquid,
            Phase::Aqueous | Phase::Liquid,
        ] => Some(MacroscopicProcess::GasEvolutionLiquidLiquid),
        [Phase::Solid, Phase::Aqueous | Phase::Liquid]
        | [Phase::Aqueous | Phase::Liquid, Phase::Solid] => {
            Some(MacroscopicProcess::GasEvolutionSolidLiquid)
        }
        _ => None,
    }
}

fn gas_evolution_reactant_phase(species: &OutcomeSpecies) -> Option<Phase> {
    if species.phase() != Phase::Unknown {
        return Some(species.phase());
    }
    let OutcomeSpecies::Resolved(species) = species else {
        return None;
    };
    let structure = species.structure.as_ref()?;
    match structure.representation() {
        RepresentationKind::Metallic => Some(Phase::Solid),
        RepresentationKind::Ionic => {
            let salt = crate::solve::ionic_salt(structure)?;
            crate::solve::salt_solubility(&salt.cation, &salt.anion).map(|soluble| {
                if soluble {
                    Phase::Aqueous
                } else {
                    Phase::Solid
                }
            })
        }
        RepresentationKind::Molecular
            if !classify_bronsted_acid(structure)
                .proton_donor_sites()
                .is_empty() =>
        {
            Some(Phase::Aqueous)
        }
        RepresentationKind::Molecular | RepresentationKind::Ion => None,
    }
}

/// The process-specific interpretation (e.g. an ionic salt reads as its
/// dissolved aqueous phase during gas evolution) stays authoritative; the
/// resolved standard-state map only fills the gaps it leaves.
fn effective_reactant_phase(
    species: &OutcomeSpecies,
    phases: &BTreeMap<SpeciesId, Phase>,
    interpretation: impl FnOnce(&OutcomeSpecies) -> Option<Phase>,
) -> Option<Phase> {
    interpretation(species).or_else(|| {
        phases
            .get(species.id())
            .copied()
            .filter(|phase| *phase != Phase::Unknown)
    })
}

fn effective_product_phase(
    species: &OutcomeSpecies,
    claimed: ClaimPhase,
    phases: &BTreeMap<SpeciesId, Phase>,
) -> Phase {
    let claimed = claim_phase(claimed);
    if claimed == Phase::Unknown {
        phases.get(species.id()).copied().unwrap_or(Phase::Unknown)
    } else {
        claimed
    }
}

fn classifies_metal_displacement(
    reactants: &[OutcomeSpecies],
    products: &[OutcomeSpecies],
    reactant_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    product_macroscopic_phases: &BTreeMap<SpeciesId, Phase>,
    claim: &ReactionClaim,
) -> bool {
    let [first, second] = reactants else {
        return false;
    };
    let [first_product, second_product] = products else {
        return false;
    };
    let [first_claim_product, second_claim_product] = claim.products.as_slice() else {
        return false;
    };
    if claim
        .products
        .iter()
        .any(|product| claim_phase(product.phase) == Phase::Gas)
    {
        return false;
    }

    let reactant_phase = |species| {
        effective_reactant_phase(
            species,
            reactant_macroscopic_phases,
            gas_evolution_reactant_phase,
        )
    };
    let (original_metal, initial_solution) = match (
        first.representation(),
        reactant_phase(first),
        second.representation(),
        reactant_phase(second),
    ) {
        (
            Some(RepresentationKind::Metallic),
            Some(Phase::Solid),
            Some(RepresentationKind::Ionic),
            Some(Phase::Aqueous),
        ) => (first, second),
        (
            Some(RepresentationKind::Ionic),
            Some(Phase::Aqueous),
            Some(RepresentationKind::Metallic),
            Some(Phase::Solid),
        ) => (second, first),
        _ => return false,
    };
    let (final_solution, deposited_metal) = match (
        first_product.representation(),
        effective_product_phase(
            first_product,
            first_claim_product.phase,
            product_macroscopic_phases,
        ),
        second_product.representation(),
        effective_product_phase(
            second_product,
            second_claim_product.phase,
            product_macroscopic_phases,
        ),
    ) {
        (
            Some(RepresentationKind::Ionic),
            Phase::Aqueous,
            Some(RepresentationKind::Metallic),
            Phase::Solid,
        ) => (first_product, second_product),
        (
            Some(RepresentationKind::Metallic),
            Phase::Solid,
            Some(RepresentationKind::Ionic),
            Phase::Aqueous,
        ) => (second_product, first_product),
        _ => return false,
    };

    let elemental_symbol = |species: &OutcomeSpecies| {
        let OutcomeSpecies::Resolved(species) = species else {
            return None;
        };
        let elements = species.structure.as_ref()?.formula().elements();
        if elements.len() != 1 {
            return None;
        }
        elements.keys().next().cloned()
    };
    let contains_element = |species: &OutcomeSpecies, symbol: &ElementSymbol| {
        let OutcomeSpecies::Resolved(species) = species else {
            return false;
        };
        species
            .structure
            .as_ref()
            .is_some_and(|structure| structure.formula().elements().contains_key(symbol))
    };
    let Some(original_symbol) = elemental_symbol(original_metal) else {
        return false;
    };
    let Some(deposited_symbol) = elemental_symbol(deposited_metal) else {
        return false;
    };
    original_symbol != deposited_symbol
        && contains_element(final_solution, &original_symbol)
        && contains_element(initial_solution, &deposited_symbol)
}

fn classifies_aqueous_precipitation(
    reactants: &[OutcomeSpecies],
    products: &[OutcomeSpecies],
    claim: &ReactionClaim,
) -> bool {
    let [first, second] = reactants else {
        return false;
    };
    let soluble_ionic_reactant = |species: &OutcomeSpecies| {
        let OutcomeSpecies::Resolved(species) = species else {
            return false;
        };
        species
            .structure
            .as_ref()
            .and_then(crate::solve::ionic_salt)
            .and_then(|salt| crate::solve::salt_solubility(&salt.cation, &salt.anion))
            == Some(true)
    };
    if !soluble_ionic_reactant(first) || !soluble_ionic_reactant(second) {
        return false;
    }

    let solid_products = claim
        .products
        .iter()
        .zip(products)
        .filter(|(product_claim, product)| {
            claim_phase(product_claim.phase) == Phase::Solid
                && product.representation() == Some(RepresentationKind::Ionic)
        })
        .count();
    let has_aqueous_product =
        claim
            .products
            .iter()
            .zip(products)
            .any(|(product_claim, product)| {
                claim_phase(product_claim.phase) == Phase::Aqueous
                    && product.representation() == Some(RepresentationKind::Ionic)
            });
    solid_products == 1
        && has_aqueous_product
        && claim
            .observations
            .iter()
            .any(|observation| observation.predicate == ClaimObservationPredicate::Forms)
}

fn precipitate_colour(salt: &crate::solve::Salt) -> Option<MacroscopicColour> {
    let is_monatomic =
        |symbol: &str| salt.anion.len() == 1 && salt.anion.get(symbol).copied() == Some(1);
    let is_hydroxide = salt.anion.len() == 2
        && salt.anion.get("H").copied() == Some(1)
        && salt.anion.get("O").copied() == Some(1);

    if salt.cation == "Ag" && salt.cation_charge == 1 {
        return if is_monatomic("Cl") {
            Some(MacroscopicColour::White)
        } else if is_monatomic("Br") {
            Some(MacroscopicColour::Cream)
        } else if is_monatomic("I") {
            Some(MacroscopicColour::Yellow)
        } else {
            None
        };
    }
    if salt.cation == "Pb" && salt.cation_charge == 2 && is_monatomic("I") {
        return Some(MacroscopicColour::Yellow);
    }
    if is_hydroxide {
        return match (salt.cation.as_str(), salt.cation_charge) {
            ("Cu", 2) => Some(MacroscopicColour::Blue),
            ("Fe", 2) => Some(MacroscopicColour::PaleGreen),
            ("Fe", 3) => Some(MacroscopicColour::RedBrown),
            ("Al", 3) | ("Mg", 2) => Some(MacroscopicColour::White),
            _ => None,
        };
    }
    None
}

fn classifies_surface_oxidation(
    first: &chem_domain::ReactionTerm,
    second: &chem_domain::ReactionTerm,
    reactants: &[OutcomeSpecies],
    products: &[OutcomeSpecies],
    claim: &ReactionClaim,
) -> bool {
    let Some((surface_metal, surface_oxygen)) = (if is_dioxygen(first) {
        Some((second, first))
    } else if is_dioxygen(second) {
        Some((first, second))
    } else {
        None
    }) else {
        return false;
    };
    let surface_metal_species = reactants
        .iter()
        .find(|species| species.id() == surface_metal.species());
    let surface_oxygen_species = reactants
        .iter()
        .find(|species| species.id() == surface_oxygen.species());
    let solid_oxide_product =
        claim.products.len() == 1
            && claim.products.first().zip(products.first()).is_some_and(
                |(product_claim, product)| {
                    claim_phase(product_claim.phase) == Phase::Solid
                        && product.representation() == Some(RepresentationKind::Ionic)
                        && FormulaComposition::parse(&ascii_formula_key(&product_claim.formula))
                            .is_ok_and(|formula| {
                                formula
                                    .elements()
                                    .keys()
                                    .any(|element| element.as_str() == "O")
                            })
                },
            );
    is_dioxygen(surface_oxygen)
        && surface_metal_species.is_some_and(|species| {
            matches!(species.phase(), Phase::Solid | Phase::Unknown)
                && species.representation() == Some(RepresentationKind::Metallic)
        })
        && surface_oxygen_species.is_some_and(|species| {
            matches!(species.phase(), Phase::Gas | Phase::Unknown)
                && species.representation() == Some(RepresentationKind::Molecular)
        })
        && solid_oxide_product
}

fn is_dioxygen(term: &chem_domain::ReactionTerm) -> bool {
    has_counts(term.formula(), &[("O", 2)])
}

fn is_carbon_hydrogen_oxygen_fuel(formula: &FormulaComposition) -> bool {
    let elements = formula.elements();
    elements
        .keys()
        .all(|element| matches!(element.as_str(), "C" | "H" | "O"))
        && elements.keys().any(|element| element.as_str() == "C")
        && elements.keys().any(|element| element.as_str() == "H")
}

fn has_counts(formula: &FormulaComposition, expected: &[(&str, u64)]) -> bool {
    formula.elements().len() == expected.len()
        && expected.iter().all(|(symbol, count)| {
            formula
                .elements()
                .iter()
                .find(|(element, _)| element.as_str() == *symbol)
                .is_some_and(|(_, actual)| actual == count)
        })
}

fn inventory_has_counts(inventory: &ElementInventory, expected: &[(&str, u64)]) -> bool {
    inventory.elements().len() == expected.len()
        && expected.iter().all(|(symbol, count)| {
            inventory
                .elements()
                .iter()
                .find(|(element, _)| element.as_str() == *symbol)
                .is_some_and(|(_, actual)| actual == count)
        })
}

fn resolve_claim_product<'a>(
    product: &ClaimProduct,
    formula: &str,
    identities: &'a SpeciesRegistry,
) -> Option<&'a ResolvedSpecies> {
    let exact = SpeciesQuery {
        name: Some(product.name.clone()),
        formula: Some(formula.to_owned()),
        charge: None,
        phase: None,
        external_identifier: None,
    };
    if let SpeciesResolution::Resolved(species) = identities.resolve(&exact) {
        return Some(species);
    }

    // Model names are descriptive, not identity-bearing. A unique reviewed
    // formula or a formula-bound external hint may recover a local structure;
    // ambiguous formulae still fail closed to FormulaOnly.
    for hint in &product.identity_hints {
        let hinted = SpeciesQuery {
            name: None,
            formula: Some(formula.to_owned()),
            charge: None,
            phase: None,
            external_identifier: Some(external_identifier(hint)),
        };
        if let SpeciesResolution::Resolved(species) = identities.resolve(&hinted) {
            return Some(species);
        }
    }
    let formula_only = SpeciesQuery {
        name: None,
        formula: Some(formula.to_owned()),
        charge: None,
        phase: None,
        external_identifier: None,
    };
    match identities.resolve(&formula_only) {
        SpeciesResolution::Resolved(species) => Some(species),
        SpeciesResolution::Ambiguous(_) | SpeciesResolution::NotFound => None,
    }
}

fn external_identifier(hint: &ClaimIdentityHint) -> ExternalIdentifier {
    match hint.kind {
        ClaimIdentityHintKind::Inchi => ExternalIdentifier::Inchi(hint.value.clone()),
        ClaimIdentityHintKind::InchiKey => ExternalIdentifier::InchiKey(hint.value.clone()),
        ClaimIdentityHintKind::CanonicalSmiles => {
            ExternalIdentifier::CanonicalSmiles(hint.value.clone())
        }
        ClaimIdentityHintKind::IsomericSmiles => {
            ExternalIdentifier::IsomericSmiles(hint.value.clone())
        }
        ClaimIdentityHintKind::PubChemCid => ExternalIdentifier::PubChemCid(hint.value.clone()),
        ClaimIdentityHintKind::RegistryId => ExternalIdentifier::RegistryId(hint.value.clone()),
    }
}

/// Resolves and binds the two authored reactants to stable checked identities.
///
/// # Errors
///
/// Returns an error for a missing or chemically ambiguous identity, or when
/// the selected identity disagrees with the authored atom inventory.
pub fn resolve_request_species(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Result<Vec<OutcomeSpecies>, AgentError> {
    match resolve_request_identities(request, identities)? {
        RequestIdentityResolution::Resolved(species) => Ok(species),
        RequestIdentityResolution::Ambiguous(ambiguity) => Err(AgentError::new(
            AgentErrorKind::IdentityFailure,
            "request identity",
            format!(
                "reactant `{}` resolves to multiple identities",
                request.reactants[ambiguity.reactant_index].display
            ),
        )),
    }
}

/// Resolves the two request reactants without guessing between chemically
/// distinct alternatives.
///
/// # Errors
///
/// Returns an error for an absent selected identity or authored atom mismatch.
pub fn resolve_request_identities(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Result<RequestIdentityResolution, AgentError> {
    resolve_request_identities_inner(request, identities, None)
}

/// Resolves request identities while collapsing reviewed aliases proven
/// isomorphic by the validated catalogue. Chemically distinct alternatives
/// remain explicit ambiguity.
///
/// # Errors
///
/// Returns an error for an absent selected identity or authored atom mismatch.
pub fn resolve_request_identities_with_catalogue(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<RequestIdentityResolution, AgentError> {
    resolve_request_identities_inner(request, identities, Some(catalogue))
}

fn resolve_request_identities_inner(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
    catalogue: Option<&ValidatedCatalogueBundle>,
) -> Result<RequestIdentityResolution, AgentError> {
    validate_request_shape(request)?;
    let mut selections = Vec::with_capacity(request.reactants.len());
    for (reactant_index, input) in request.reactants.iter().enumerate() {
        let lookup = if let Some(species_id) = &input.species_id {
            IdentityLookup::Resolved(identities.get(species_id).ok_or_else(|| {
                AgentError::new(
                    AgentErrorKind::IdentityFailure,
                    "request identity",
                    format!("selected species `{species_id}` is not in the current snapshot"),
                )
            })?)
        } else {
            resolve_name_or_formula(&input.display, identities, catalogue)?
        };
        match lookup {
            IdentityLookup::Resolved(species) => {
                validate_atomic_numbers(&input.atomic_numbers, species)?;
                selections.push(OutcomeSpecies::Resolved(Box::new(species.clone())));
            }
            IdentityLookup::Ambiguous(ambiguity) => {
                let alternatives = ambiguity
                    .alternatives
                    .iter()
                    .filter_map(|id| identities.get(id))
                    .filter(|species| {
                        validate_atomic_numbers(&input.atomic_numbers, species).is_ok()
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if alternatives.len() == 1 {
                    selections.push(OutcomeSpecies::Resolved(Box::new(alternatives[0].clone())));
                } else if alternatives.len() >= 2 {
                    return Ok(RequestIdentityResolution::Ambiguous(
                        ReactantIdentityAmbiguity {
                            reactant_index,
                            query: ambiguity.query,
                            alternatives,
                        },
                    ));
                } else {
                    return Err(AgentError::new(
                        AgentErrorKind::InvalidRequest,
                        "request binding",
                        format!(
                            "no identity alternative for `{}` matches its composed atoms",
                            input.display
                        ),
                    ));
                }
            }
            IdentityLookup::NotFound => {
                if let Some(generated) = generated_reactant(input) {
                    selections.push(generated);
                } else {
                    selections.push(formula_only_reactant(input)?);
                }
            }
        }
    }
    Ok(RequestIdentityResolution::Resolved(selections))
}

fn validate_request_shape(request: &ReactionBuildRequest) -> Result<(), AgentError> {
    if !(1..=2).contains(&request.reactants.len()) {
        return Err(AgentError::new(
            AgentErrorKind::InvalidRequest,
            "request shape",
            "a dynamic request must contain one or two reactants",
        ));
    }
    if request.reactants.len() == 1
        && !request.selected_context.as_deref().is_some_and(|context| {
            matches!(
                context.trim().to_ascii_lowercase().as_str(),
                "heat" | "light" | "electricity"
            )
        })
    {
        return Err(AgentError::new(
            AgentErrorKind::InvalidRequest,
            "request shape",
            "a single-reactant request requires the context heat, light, or electricity",
        ));
    }
    Ok(())
}

fn validate_selected_context_binding(
    request: &ReactionBuildRequest,
    claim: &ReactionClaim,
) -> Result<(), AgentError> {
    if request.reactants.len() != 1 {
        return Ok(());
    }
    let selected = request
        .selected_context
        .as_deref()
        .expect("validated single-reactant requests always have context")
        .trim();
    if !claim.required_context.trim().eq_ignore_ascii_case(selected) {
        return Err(AgentError::new(
            AgentErrorKind::InvalidRequest,
            "request context",
            format!(
                "single-reactant claim context `{}` must preserve selected context `{selected}`",
                claim.required_context
            ),
        ));
    }
    Ok(())
}

enum IdentityLookup<'a> {
    Resolved(&'a ResolvedSpecies),
    Ambiguous(SpeciesAmbiguity),
    NotFound,
}

fn resolve_name_or_formula<'a>(
    value: &str,
    identities: &'a SpeciesRegistry,
    catalogue: Option<&ValidatedCatalogueBundle>,
) -> Result<IdentityLookup<'a>, AgentError> {
    let formula_key = ascii_formula_key(value);
    for query in [
        SpeciesQuery {
            name: Some(value.to_owned()),
            formula: None,
            charge: None,
            phase: None,
            external_identifier: None,
        },
        SpeciesQuery {
            name: None,
            formula: Some(formula_key.clone()),
            charge: None,
            phase: None,
            external_identifier: None,
        },
    ] {
        match identities.resolve(&query) {
            SpeciesResolution::Resolved(species) => return Ok(IdentityLookup::Resolved(species)),
            SpeciesResolution::Ambiguous(ambiguity) => {
                let alternatives = ambiguity
                    .alternatives
                    .iter()
                    .filter_map(|id| identities.get(id))
                    .collect::<Vec<_>>();
                if let Some(first) = alternatives.first()
                    && alternatives.iter().all(|candidate| {
                        candidate.formula == first.formula
                            && candidate.charge == first.charge
                            && candidate.phase == first.phase
                            && equivalent_structure(candidate, first, catalogue)
                    })
                {
                    return alternatives
                        .into_iter()
                        .min_by(|left, right| left.id.cmp(&right.id))
                        .map(IdentityLookup::Resolved)
                        .ok_or_else(|| {
                            AgentError::new(
                                AgentErrorKind::IdentityFailure,
                                "request identity",
                                format!("reactant `{value}` has no stable equivalent identity"),
                            )
                        });
                }
                let metallic = ambiguity
                    .alternatives
                    .iter()
                    .filter_map(|id| identities.get(id))
                    .filter(|species| {
                        species.structure.as_ref().is_some_and(|structure| {
                            structure.representation() == RepresentationKind::Metallic
                        })
                    })
                    .collect::<Vec<_>>();
                if let Some(first) = metallic.first()
                    && metallic.iter().all(|candidate| {
                        candidate.formula == first.formula && candidate.charge == first.charge
                    })
                {
                    return metallic
                        .into_iter()
                        .min_by(|left, right| left.id.cmp(&right.id))
                        .map(IdentityLookup::Resolved)
                        .ok_or_else(|| {
                            AgentError::new(
                                AgentErrorKind::IdentityFailure,
                                "request identity",
                                format!("reactant `{value}` has no stable metallic identity"),
                            )
                        });
                }
                return Ok(IdentityLookup::Ambiguous(ambiguity));
            }
            SpeciesResolution::NotFound => {}
        }
    }
    Ok(IdentityLookup::NotFound)
}

/// Builds an on-the-fly identity around a programmatically generated
/// structure. Returns None when the inventory has no unambiguous structure,
/// leaving the formula-only path to handle it.
fn generated_outcome_species(
    display_name: &str,
    formula_text: &str,
    phase: Phase,
    inventory: &ElementInventory,
) -> Option<OutcomeSpecies> {
    let digest = generated_species_identity_digest(display_name, formula_text).to_hex();
    let id = SpeciesId::from_str(&format!("generated.s{}", &digest[..24])).ok()?;
    let structure_id = StructureId::new(format!("generated.{}", &digest[..24])).ok()?;
    // Name-keyed canonical structures first: an inventory maps to one
    // generated structure, so a constitutional isomer of a canonical
    // molecule (ammonium cyanate vs urea) is only reachable by name — or
    // by subset SMILES, which the sketcher emits as the display.
    let structure =
        chem_domain::generate_named_structure(structure_id.clone(), display_name, inventory)
            .or_else(|| {
                let parsed =
                    chem_domain::structure_from_smiles(structure_id.clone(), display_name)?;
                (parsed.formula() == inventory).then_some(parsed)
            })
            .or_else(|| generate_structure(structure_id, inventory))?;
    let species =
        crate::identity::generated_species(&id, display_name, formula_text, phase, &structure)
            .ok()?;
    Some(OutcomeSpecies::Resolved(Box::new(species)))
}

fn generated_reactant(input: &crate::ReactantInput) -> Option<OutcomeSpecies> {
    if input.atomic_numbers.is_empty() {
        return None;
    }
    let mut counts = std::collections::BTreeMap::new();
    for atomic_number in &input.atomic_numbers {
        let symbol = ElementSymbol::new(symbol_of(*atomic_number)?).ok()?;
        *counts.entry(symbol).or_insert(0_u64) += 1;
    }
    let inventory = ElementInventory::new(counts).ok()?;
    // The display is only reference as a formula when it actually describes
    // the composed atoms: names ("ammonium cyanate") fail to parse, and a
    // SMILES display like "CCO" parses to the WRONG composition (C2O), so
    // both fall back to the inventory's own formula text.
    let key = ascii_formula_key(&input.display);
    let display_is_formula = FormulaComposition::parse(&key).is_ok_and(|parsed| {
        ElementInventory::new(
            parsed
                .elements()
                .iter()
                .map(|(symbol, count)| (symbol.clone(), *count)),
        )
        .is_ok_and(|parsed_inventory| parsed_inventory == inventory)
    });
    let formula_text = if display_is_formula {
        key
    } else {
        crate::identity::inventory_formula(&inventory)
    };
    generated_outcome_species(&input.display, &formula_text, Phase::Unknown, &inventory)
}

fn generated_product(
    product: &ClaimProduct,
    formula: &str,
    phase: Phase,
) -> Option<OutcomeSpecies> {
    let composition = FormulaComposition::parse(formula).ok()?;
    let inventory = ElementInventory::new(
        composition
            .elements()
            .iter()
            .map(|(symbol, count)| (symbol.clone(), *count)),
    )
    .ok()?;
    // A SMILES hint names the exact isomer; it must still agree with the
    // claimed formula before it may stand in for generation.
    for hint in &product.identity_hints {
        if !matches!(
            hint.kind,
            ClaimIdentityHintKind::CanonicalSmiles | ClaimIdentityHintKind::IsomericSmiles
        ) {
            continue;
        }
        let digest = generated_species_identity_digest(&product.name, formula).to_hex();
        let structure_id = StructureId::new(format!("generated.{}", &digest[..24])).ok()?;
        if let Some(structure) = chem_domain::structure_from_smiles(structure_id, &hint.value)
            && *structure.formula() == inventory
        {
            let id = SpeciesId::from_str(&format!("generated.s{}", &digest[..24])).ok()?;
            let species =
                crate::identity::generated_species(&id, &product.name, formula, phase, &structure)
                    .ok()?;
            return Some(OutcomeSpecies::Resolved(Box::new(species)));
        }
    }
    generated_outcome_species(&product.name, formula, phase, &inventory)
}

fn formula_only_reactant(input: &crate::ReactantInput) -> Result<OutcomeSpecies, AgentError> {
    let formula = ascii_formula_key(&input.display);
    let composition = FormulaComposition::parse(&formula).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidRequest, "request formula", error)
    })?;
    if input.atomic_numbers.is_empty() {
        return Err(AgentError::new(
            AgentErrorKind::InvalidRequest,
            "request binding",
            format!("reactant `{}` has no composed atoms", input.display),
        ));
    }
    let formula_atom_count =
        composition
            .elements()
            .values()
            .try_fold(0_usize, |total, count| {
                total
                    .checked_add(usize::try_from(*count).map_err(|_| {
                        AgentError::new(
                            AgentErrorKind::InvalidRequest,
                            "request binding",
                            "formula atom count overflow",
                        )
                    })?)
                    .ok_or_else(|| {
                        AgentError::new(
                            AgentErrorKind::InvalidRequest,
                            "request binding",
                            "formula atom count overflow",
                        )
                    })
            })?;
    if formula_atom_count != input.atomic_numbers.len() {
        return Err(AgentError::new(
            AgentErrorKind::InvalidRequest,
            "request binding",
            format!(
                "reactant `{}` formula contains {formula_atom_count} atoms but the composer supplied {}",
                input.display,
                input.atomic_numbers.len()
            ),
        ));
    }
    let mut atomic_numbers = input.atomic_numbers.clone();
    atomic_numbers.sort_unstable();
    let id_material = serde_json::to_vec(&("formula-only-reactant-v2", &formula, &atomic_numbers))
        .map_err(|error| {
            AgentError::from_source(AgentErrorKind::IdentityFailure, "request identity", error)
        })?;
    let digest = ContentDigest::sha256(&id_material).to_hex();
    let id = SpeciesId::from_str(&format!("dynamic.r{}", &digest[..24])).map_err(|error| {
        AgentError::from_source(AgentErrorKind::IdentityFailure, "request identity", error)
    })?;
    Ok(OutcomeSpecies::FormulaOnly {
        id,
        display_name: input.display.clone(),
        formula,
        phase: Phase::Unknown,
    })
}

fn equivalent_structure(
    left: &ResolvedSpecies,
    right: &ResolvedSpecies,
    catalogue: Option<&ValidatedCatalogueBundle>,
) -> bool {
    match (&left.structure, &right.structure) {
        (Some(left), Some(right)) => {
            left.representation() == right.representation()
                && (left.graph() == right.graph()
                    || catalogue.is_some_and(|catalogue| {
                        catalogue
                            .structures_isomorphic(left.id(), right.id())
                            .ok()
                            .flatten()
                            .unwrap_or(false)
                    }))
        }
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

fn ascii_formula_key(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '₀' => '0',
            '₁' => '1',
            '₂' => '2',
            '₃' => '3',
            '₄' => '4',
            '₅' => '5',
            '₆' => '6',
            '₇' => '7',
            '₈' => '8',
            '₉' => '9',
            other => other,
        })
        .collect()
}

fn generated_species_identity_digest(display_name: &str, formula: &str) -> ContentDigest {
    let mut material = Vec::new();
    for field in ["generated_species_identity", "2", display_name, formula] {
        material.extend_from_slice(field.len().to_string().as_bytes());
        material.push(b':');
        material.extend_from_slice(field.as_bytes());
    }
    ContentDigest::sha256(&material)
}

fn validate_atomic_numbers(authored: &[u8], resolved: &ResolvedSpecies) -> Result<(), AgentError> {
    let mut expected = BTreeMap::<u16, BigUint>::new();
    for (element, count) in resolved.formula.composition() {
        expected.insert(element.atomic_number(), count.clone());
    }
    let mut actual = BTreeMap::<u16, BigUint>::new();
    for atomic_number in authored {
        *actual.entry(u16::from(*atomic_number)).or_default() += BigUint::from(1_u8);
    }
    if actual != expected {
        return Err(AgentError::new(
            AgentErrorKind::InvalidRequest,
            "request binding",
            format!(
                "reactant `{}` identity disagrees with its composed atoms",
                resolved.display_name
            ),
        ));
    }
    Ok(())
}

fn resolved_term(species: &ResolvedSpecies) -> Result<UnbalancedReactionTerm, AgentError> {
    Ok(UnbalancedReactionTerm {
        species: species.id.clone(),
        display_name: species.display_name.clone(),
        formula_text: species.formula_text.clone(),
        formula: FormulaComposition::parse(&species.formula_text).map_err(|error| {
            AgentError::from_source(AgentErrorKind::CompilationFailure, "outcome formula", error)
        })?,
        charge: species.charge.clone(),
        phase: species.phase,
    })
}

fn outcome_term(species: &OutcomeSpecies) -> Result<UnbalancedReactionTerm, AgentError> {
    match species {
        OutcomeSpecies::Resolved(species) => resolved_term(species),
        OutcomeSpecies::FormulaOnly {
            id,
            display_name,
            formula,
            phase,
        } => Ok(UnbalancedReactionTerm {
            species: id.clone(),
            display_name: display_name.clone(),
            formula_text: formula.clone(),
            formula: FormulaComposition::parse(formula).map_err(|error| {
                AgentError::from_source(
                    AgentErrorKind::CompilationFailure,
                    "outcome formula",
                    error,
                )
            })?,
            charge: Charge::neutral(),
            phase: *phase,
        }),
    }
}

const fn claim_phase(phase: ClaimPhase) -> Phase {
    match phase {
        ClaimPhase::Aqueous => Phase::Aqueous,
        ClaimPhase::Solid => Phase::Solid,
        ClaimPhase::Liquid => Phase::Liquid,
        ClaimPhase::Gas => Phase::Gas,
        ClaimPhase::Unknown => Phase::Unknown,
    }
}

fn format_equation(declaration: &ReactionDeclaration) -> String {
    let side = |terms: &[chem_domain::ReactionTerm]| {
        terms
            .iter()
            .map(|term| {
                if term.coefficient() == 1 {
                    term.formula_text().to_owned()
                } else {
                    format!("{} {}", term.coefficient(), term.formula_text())
                }
            })
            .collect::<Vec<_>>()
            .join(" + ")
    };
    format!(
        "{} → {}",
        side(declaration.reactants()),
        side(declaration.products())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem_catalogue::{CatalogueEnvelope, ValidatedCatalogueBundle};
    use serde_json::json;

    use crate::{
        ClaimMode, ReactantInput, reviewed_species_registry, solve_reaction_claim,
        test_support::reference_catalogue as reference,
    };

    fn registry() -> SpeciesRegistry {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
                .expect("catalogue");
        let mut envelope: CatalogueEnvelope = serde_json::from_slice(&bytes).expect("envelope");
        envelope.digest = envelope.computed_digest().expect("digest");
        let catalogue = ValidatedCatalogueBundle::validate(envelope).expect("valid catalogue");
        reviewed_species_registry(&catalogue).expect("identities")
    }

    #[test]
    fn phase_synthesis_requires_exact_supported_typed_layouts() {
        let sulfur: &[(&str, u64)] = &[("S", 8)];
        let hydrogen: &[(&str, u64)] = &[("H", 2)];
        let oxygen: &[(&str, u64)] = &[("O", 2)];
        let classify =
            |first, second, product| chem_domain::classify_phase_synthesis(first, second, product);
        assert_eq!(
            classify((sulfur, Phase::Solid), (oxygen, Phase::Gas), Phase::Gas),
            Some(chem_domain::PhaseSynthesisRoute::SolidGas)
        );
        assert_eq!(
            classify((oxygen, Phase::Gas), (sulfur, Phase::Solid), Phase::Gas),
            Some(chem_domain::PhaseSynthesisRoute::SolidGas)
        );
        assert_eq!(
            classify((hydrogen, Phase::Gas), (sulfur, Phase::Gas), Phase::Gas),
            Some(chem_domain::PhaseSynthesisRoute::GasGas)
        );
        assert_eq!(
            classify((sulfur, Phase::Unknown), (oxygen, Phase::Gas), Phase::Gas),
            None
        );
        assert_eq!(
            classify((hydrogen, Phase::Gas), (sulfur, Phase::Gas), Phase::Liquid),
            None
        );
        // Hydrogen/oxygen burning stays out of the chamber in either order.
        assert_eq!(
            classify((hydrogen, Phase::Gas), (oxygen, Phase::Gas), Phase::Gas),
            None
        );
        assert_eq!(
            classify((oxygen, Phase::Gas), (hydrogen, Phase::Gas), Phase::Gas),
            None
        );
    }

    #[test]
    fn catalogue_standard_phases_reach_future_dynamic_classification() {
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "hydrogen".into(),
                    atomic_numbers: vec![1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "chlorine".into(),
                    atomic_numbers: vec![17, 17],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "disposition": "reaction",
                "products": [{
                    "name": "hydrogen chloride",
                    "formula": "HCl",
                    "phase": "gas",
                    "identity_hints": []
                }],
                "required_context": "phase-qualified synthesis",
                "observations": [{
                    "predicate": "forms",
                    "subject": "hydrogen chloride",
                    "value": null
                }],
                "sources": [],
                "ambiguity": null
            }))
            .expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome_with_catalogue(&request, claim, &identities, &catalogue)
                .expect("catalogue-backed claim compiles")
        else {
            panic!("expected static outcome");
        };

        assert_eq!(
            outcome.macroscopic_process(),
            Some(MacroscopicProcess::GasGasSynthesis)
        );
        assert!(
            outcome
                .reactants()
                .iter()
                .all(|reactant| outcome.macroscopic_phase(reactant) == Phase::Gas)
        );
    }

    #[test]
    fn researched_reactant_phases_route_uncatalogued_gas_synthesis() {
        let identities = registry();
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "NO".into(),
                    atomic_numbers: vec![7, 8],
                    species_id: None,
                },
                ReactantInput {
                    display: "O2".into(),
                    atomic_numbers: vec![8, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "disposition": "reaction",
                "reactant_phases": ["gas", "gas"],
                "products": [{
                    "name": "nitrogen dioxide",
                    "formula": "NO2",
                    "phase": "gas",
                    "identity_hints": []
                }],
                "required_context": "ordinary gas-phase combination",
                "observations": [{
                    "predicate": "forms",
                    "subject": "nitrogen dioxide",
                    "value": null
                }],
                "sources": [],
                "ambiguity": null
            }))
            .expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome(&request, claim, &identities).expect("claim compiles")
        else {
            panic!("expected static outcome");
        };

        assert_eq!(
            outcome.macroscopic_process(),
            Some(MacroscopicProcess::GasGasSynthesis)
        );
        assert!(
            outcome
                .reactants()
                .iter()
                .all(|reactant| outcome.macroscopic_phase(reactant) == Phase::Gas)
        );
    }

    #[test]
    fn researched_reactant_phase_count_must_match_request() {
        let identities = registry();
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "NO".into(),
                    atomic_numbers: vec![7, 8],
                    species_id: None,
                },
                ReactantInput {
                    display: "O2".into(),
                    atomic_numbers: vec![8, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "disposition": "reaction",
                "reactant_phases": ["gas"],
                "products": [{
                    "name": "nitrogen dioxide",
                    "formula": "NO2",
                    "phase": "gas",
                    "identity_hints": []
                }],
                "required_context": "ordinary gas-phase combination",
                "observations": [],
                "sources": [],
                "ambiguity": null
            }))
            .expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("wire claim remains bounded");

        let error = compile_claim_outcome(&request, claim, &identities)
            .expect_err("request binding must reject an incomplete phase list");
        assert_eq!(error.kind(), AgentErrorKind::InvalidProviderOutput);
        assert_eq!(error.context(), "reaction claim");
    }

    #[test]
    fn compact_claim_compiles_to_exact_private_static_outcome() {
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"representative educational outcome under the reviewed standard-outcome premise",
            "observations":[{"predicate":"evolves","subject":"hydrogen","value":null}],
            "sources":[],
            "ambiguity":null
        });
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let outcome = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "Li".into(),
                        atomic_numbers: vec![3],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ]
                .to_vec(),
                selected_context: None,
            },
            claim,
            &registry(),
        )
        .expect("compile claim");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome")
        };
        assert_eq!(outcome.equation(), "2 Li + 2 H2O → H2 + 2 LiOH");
        assert_eq!(outcome.claim_provenance(), OutcomeProvenance::ModelAsserted);
        assert_eq!(
            outcome.claim().provenance(),
            crate::ClaimProvenance::Provider
        );
        assert!(outcome.products().iter().all(OutcomeSpecies::has_structure));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
                .expect("catalogue");
        let mut envelope: CatalogueEnvelope = serde_json::from_slice(&bytes).expect("envelope");
        envelope.digest = envelope.computed_digest().expect("digest");
        let catalogue = ValidatedCatalogueBundle::validate(envelope).expect("valid catalogue");
        let source =
            std::fs::read_to_string(root.join("conformance/end-to-end/alkali-water-li-001.chems"))
                .expect("source");
        let evidence =
            std::fs::read(root.join("conformance/observations/alkali-water-li-001.evidence.json"))
                .expect("evidence");
        let parsed = chem_kernel::expand_provisional(
            "alkali-water-li-001.chems",
            &source,
            &catalogue,
            &evidence,
        )
        .expect("parsed declaration");
        assert_eq!(outcome.declaration(), parsed.claim().declaration());
    }

    #[test]
    fn unique_reviewed_product_formula_recovers_structure_when_model_name_drifts() {
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"aqueous hydroxide product","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen gas product","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"ordinary contact",
            "observations":[], "sources":[], "ambiguity":null
        });
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let compiled = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "Li".into(),
                        atomic_numbers: vec![3],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ]
                .to_vec(),
                selected_context: None,
            },
            claim,
            &registry(),
        )
        .expect("compiled");
        let CompiledClaimOutcome::Static(outcome) = compiled else {
            panic!("expected static outcome")
        };
        assert!(outcome.products().iter().all(OutcomeSpecies::has_structure));
        assert!(outcome.products_without_structure().is_empty());
    }

    #[test]
    fn neutralisation_process_and_aqueous_colour_are_structure_derived() {
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        let compile = |reactants: [(&str, Vec<u8>); 2]| {
            let request = ReactionBuildRequest {
                reactants: reactants
                    .into_iter()
                    .map(|(display, atomic_numbers)| ReactantInput {
                        display: display.to_owned(),
                        atomic_numbers,
                        species_id: None,
                    })
                    .collect(),
                selected_context: None,
            };
            let claim = solve_reaction_claim(&request, &identities).expect("neutralisation solves");
            let CompiledClaimOutcome::Static(outcome) =
                compile_claim_outcome(&request, claim, &identities).expect("outcome compiles")
            else {
                panic!("neutralisation is static")
            };
            outcome
        };

        let copper = compile([("CuO", vec![29, 8]), ("H2SO4", vec![1, 1, 16, 8, 8, 8, 8])]);
        let copper_sulfate = copper
            .products()
            .iter()
            .find(|product| product.display_name() == "copper(II) sulfate")
            .expect("copper sulfate product");
        assert_eq!(
            copper.macroscopic_process(),
            Some(MacroscopicProcess::SolventEvaporationCrystallization)
        );
        assert_eq!(
            copper.macroscopic_colour(copper_sulfate),
            Some(MacroscopicColour::PaleBlue)
        );

        let carbonate = compile([("HCl", vec![1, 17]), ("Na2CO3", vec![11, 11, 6, 8, 8, 8])]);
        assert_eq!(
            carbonate.macroscopic_process(),
            Some(MacroscopicProcess::GasEvolutionLiquidLiquid),
            "gas generation selects the authored gas clip before optional salt isolation"
        );
        assert!(
            carbonate
                .products()
                .iter()
                .all(|product| carbonate.macroscopic_colour(product).is_none()),
            "unknown or main-group colours keep the conservative fallback"
        );
    }

    #[test]
    fn catalogue_authorizes_every_heavy_alkali_water_layout_before_presentation() {
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        for (display, atomic_number, symbol, variant) in [
            (
                "RubidiumMetal",
                37,
                "Rb",
                ExplosiveWaterContactVariantRecord::Rubidium,
            ),
            (
                "CaesiumMetal",
                55,
                "Cs",
                ExplosiveWaterContactVariantRecord::Caesium,
            ),
            (
                "FranciumMetal",
                87,
                "Fr",
                ExplosiveWaterContactVariantRecord::Francium,
            ),
        ] {
            let request = ReactionBuildRequest {
                reactants: vec![
                    ReactantInput {
                        display: display.to_owned(),
                        atomic_numbers: vec![atomic_number],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "water".to_owned(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ],
                selected_context: None,
            };
            let claim = ProviderClaim::from_json(
                &serde_json::to_vec(&json!({
                    "schema_version": 1,
                    "disposition": "reaction",
                    "products": [
                        {"name":"aqueous hydroxide", "formula":format!("{symbol}OH"), "phase":"aqueous", "identity_hints":[]},
                        {"name":"hydrogen", "formula":"H2", "phase":"gas", "identity_hints":[]}
                    ],
                    "required_context":"representative educational outcome",
                    "observations": [],
                    "sources": [],
                    "ambiguity": null
                }))
                .expect("claim bytes"),
                ClaimMode::Fast,
            )
            .expect("claim");
            let CompiledClaimOutcome::Static(outcome) = compile_claim_outcome_with_catalogue(
                &request,
                claim.clone(),
                &identities,
                &catalogue,
            )
            .expect("catalogue-aware outcome") else {
                panic!("heavy alkali outcome must be static")
            };
            assert_eq!(
                outcome.macroscopic_process(),
                Some(MacroscopicProcess::ExplosiveMetalWater(variant)),
                "{display} requires the exact reviewed water-contact variant"
            );

            let CompiledClaimOutcome::Static(without_catalogue) =
                compile_claim_outcome(&request, claim, &identities).expect("unreviewed compile")
            else {
                panic!("unreviewed outcome must remain static")
            };
            assert!(
                !matches!(
                    without_catalogue.macroscopic_process(),
                    Some(MacroscopicProcess::ExplosiveMetalWater(_))
                ),
                "the high-energy category needs the catalogue material capability"
            );
        }
    }

    #[test]
    fn formula_only_reactant_compiles_and_enters_structure_escalation() {
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "disposition": "reaction",
                "products": [
                    {"name":"carbon dioxide","formula":"CO2","phase":"gas","identity_hints":[]},
                    {"name":"water","formula":"H2O","phase":"gas","identity_hints":[]}
                ],
                "required_context":"combustion in oxygen",
                "observations":[], "sources":[], "ambiguity":null
            }))
            .expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        let compiled = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "CH4".into(),
                        atomic_numbers: vec![6, 1, 1, 1, 1],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "O2".into(),
                        atomic_numbers: vec![8, 8],
                        species_id: None,
                    },
                ]
                .to_vec(),
                selected_context: None,
            },
            claim,
            &identities,
        )
        .expect("formula-only reactants compile");
        let CompiledClaimOutcome::Static(outcome) = compiled else {
            panic!("expected static outcome")
        };
        let terms = |terms: &[chem_domain::ReactionTerm]| {
            terms
                .iter()
                .map(|term| (term.formula_text().to_owned(), term.coefficient()))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            terms(outcome.declaration().reactants()),
            BTreeMap::from([("CH4".to_owned(), 1), ("O2".to_owned(), 2)])
        );
        assert_eq!(
            terms(outcome.declaration().products()),
            BTreeMap::from([("CO2".to_owned(), 1), ("H2O".to_owned(), 2)])
        );
        // Methane is not catalogued, but its structure generates on the fly,
        // so nothing needs escalation.
        assert!(matches!(
            &outcome.reactants()[0],
            OutcomeSpecies::Resolved(species) if species.structure.is_some()
        ));
        assert!(outcome.species_without_structure().is_empty());
        assert_eq!(
            outcome.macroscopic_process(),
            Some(MacroscopicProcess::CompleteCombustion),
            "validated C/H fuel + dioxygen -> gaseous CO2 + H2O should carry a typed process"
        );
        assert_eq!(outcome.combustion_fuel_carbon_count(), Some(1));
    }

    #[test]
    fn generated_reactant_identity_is_versioned_atom_order_canonical_and_count_bound() {
        let identities = registry();
        let request = |atomic_numbers| ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "CH4".into(),
                    atomic_numbers,
                    species_id: None,
                },
                ReactantInput {
                    display: "O2".into(),
                    atomic_numbers: vec![8, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let RequestIdentityResolution::Resolved(first) =
            resolve_request_identities(&request(vec![6, 1, 1, 1, 1]), &identities)
                .expect("formula-only identity")
        else {
            panic!("formula-only request must resolve")
        };
        let RequestIdentityResolution::Resolved(reordered) =
            resolve_request_identities(&request(vec![1, 6, 1, 1, 1]), &identities)
                .expect("reordered formula-only identity")
        else {
            panic!("reordered formula-only request must resolve")
        };
        assert_eq!(first[0].id(), reordered[0].id());
        assert_eq!(
            first[0].id().as_str(),
            "generated.s1b9906418c6c2f68f315698c"
        );

        let error = resolve_request_identities(&request(vec![6, 1, 1]), &identities)
            .expect_err("the formula and composed atom count must agree");
        assert_eq!(error.kind(), AgentErrorKind::InvalidRequest);
        assert_eq!(error.context(), "request binding");
    }

    #[test]
    fn reviewed_isomorphic_aliases_do_not_create_learner_ambiguity() {
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "CO2".into(),
                    atomic_numbers: vec![6, 8, 8],
                    species_id: None,
                },
                ReactantInput {
                    display: "NaCl".into(),
                    atomic_numbers: vec![11, 17],
                    species_id: None,
                },
            ]
            .to_vec(),
            selected_context: None,
        };
        let resolved = resolve_request_identities_with_catalogue(&request, &identities, &catalogue)
            .expect("identity resolution");
        assert!(matches!(resolved, RequestIdentityResolution::Resolved(_)));
    }

    #[test]
    fn single_reactant_light_context_balances_without_photon_species() {
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        let mut request = ReactionBuildRequest {
            reactants: vec![ReactantInput {
                display: "AgCl".into(),
                atomic_numbers: vec![47, 17],
                species_id: None,
            }],
            selected_context: Some("light".into()),
        };
        let RequestIdentityResolution::Resolved(resolved) =
            resolve_request_identities_with_catalogue(&request, &identities, &catalogue)
                .expect("identity resolution")
        else {
            panic!("AgCl aliases should collapse")
        };
        let OutcomeSpecies::Resolved(species) = &resolved[0] else {
            panic!("AgCl should be reviewed")
        };
        request.reactants[0].species_id = Some(species.id.clone());
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "disposition": "reaction",
                "products": [
                    {"name":"silver","formula":"Ag","phase":"solid","identity_hints":[]},
                    {"name":"chlorine","formula":"Cl2","phase":"gas","identity_hints":[]}
                ],
                "required_context":"light",
                "observations":[], "sources":[], "ambiguity":null
            }))
            .expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome(&request, claim, &identities).expect("compile")
        else {
            panic!("static outcome expected")
        };
        assert!(outcome.equation().starts_with("2 AgCl →"));
        assert!(!outcome.equation().to_ascii_lowercase().contains("photon"));
        assert_eq!(outcome.declaration().required_context(), "light");
    }

    #[test]
    fn catalogue_backed_sodium_hydroxide_electrolysis_uses_the_net_water_reaction() {
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        let mut request = ReactionBuildRequest {
            reactants: vec![ReactantInput {
                display: "NaOH".into(),
                atomic_numbers: vec![11, 8, 1],
                species_id: None,
            }],
            selected_context: Some("electricity".into()),
        };
        let RequestIdentityResolution::Resolved(resolved) =
            resolve_request_identities_with_catalogue(&request, &identities, &catalogue)
                .expect("identity resolution")
        else {
            panic!("NaOH aliases should collapse")
        };
        request.reactants[0].species_id = Some(resolved[0].id().clone());

        let claim = crate::solve_reaction_claim_with_catalogue(&request, &identities, &catalogue)
            .expect("aqueous NaOH electrolysis should solve locally");
        assert_eq!(
            claim
                .products
                .iter()
                .map(|product| product.formula.as_str())
                .collect::<Vec<_>>(),
            ["H2", "O2"]
        );
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome_with_catalogue(&request, claim, &identities, &catalogue)
                .expect("catalogue-backed NaOH electrolysis should balance")
        else {
            panic!("NaOH electrolysis should produce a static outcome")
        };

        assert_eq!(outcome.declaration().reactants().len(), 1);
        assert_eq!(outcome.declaration().reactants()[0].formula_text(), "H2O");
        assert_eq!(outcome.declaration().reactants()[0].coefficient(), 2);
        assert_eq!(
            outcome
                .declaration()
                .products()
                .iter()
                .map(|term| (term.formula_text(), term.coefficient()))
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::from([("H2", 2), ("O2", 1)])
        );
        assert!(
            outcome
                .declaration()
                .products()
                .iter()
                .all(|term| term.formula_text() != "NaOH"),
            "an unchanged electrolyte must not be listed as its own product"
        );
        assert!(outcome.species_without_structure().is_empty());
    }

    #[test]
    fn single_reactant_claim_cannot_replace_the_selected_energy_context() {
        let catalogue = reference();
        let identities = reviewed_species_registry(&catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: vec![ReactantInput {
                display: "AgCl".into(),
                atomic_numbers: vec![47, 17],
                species_id: None,
            }],
            selected_context: Some("light".into()),
        };
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "disposition": "reaction",
                "products": [
                    {"name":"silver","formula":"Ag","phase":"solid","identity_hints":[]},
                    {"name":"chlorine","formula":"Cl2","phase":"gas","identity_hints":[]}
                ],
                "required_context":"heat",
                "observations":[], "sources":[], "ambiguity":null
            }))
            .expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let error = compile_claim_outcome(&request, claim, &identities)
            .expect_err("the model cannot replace the learner's selected context");
        assert_eq!(error.kind(), AgentErrorKind::InvalidRequest);
        assert_eq!(error.context(), "request context");
        assert!(
            error
                .to_string()
                .contains("preserve selected context `light`")
        );
    }

    #[test]
    fn two_reactant_request_json_keeps_the_existing_shape() {
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "H2".into(),
                    atomic_numbers: vec![1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "O2".into(),
                    atomic_numbers: vec![8, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        assert_eq!(
            serde_json::to_value(request).expect("request JSON"),
            json!({
                "reactants": [
                    {"display":"H2","atomic_numbers":[1,1]},
                    {"display":"O2","atomic_numbers":[8,8]}
                ]
            })
        );
    }

    #[test]
    fn request_binding_rejects_stale_composed_atoms() {
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"ordinary contact","observations":[],"sources":[],"ambiguity":null
        });
        let parsed =
            ProviderClaim::from_json(&serde_json::to_vec(&claim).unwrap(), ClaimMode::Fast)
                .unwrap();
        let error = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "Li".into(),
                        atomic_numbers: vec![37],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ]
                .to_vec(),
                selected_context: None,
            },
            parsed,
            &registry(),
        )
        .expect_err("stale atoms must fail request binding");
        assert_eq!(error.kind(), AgentErrorKind::InvalidRequest);
        assert_eq!(error.context(), "request binding");
    }

    #[test]
    fn composer_subscript_formula_resolves_without_changing_display_text() {
        let identities = registry();
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "Li".into(),
                    atomic_numbers: vec![3],
                    species_id: None,
                },
                ReactantInput {
                    display: "H₂O".into(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: None,
                },
            ]
            .to_vec(),
            selected_context: None,
        };
        let RequestIdentityResolution::Resolved(resolved) =
            resolve_request_identities(&request, &identities).expect("identity resolution")
        else {
            panic!("request should resolve")
        };
        let OutcomeSpecies::Resolved(species) = &resolved[1] else {
            panic!("water should resolve")
        };
        assert_eq!(species.formula_text, "H2O");
        assert_eq!(request.reactants[1].display, "H₂O");
    }

    #[test]
    fn model_product_subscripts_normalize_before_identity_and_balance() {
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "H₂SO₄".into(),
                    atomic_numbers: vec![1, 1, 16, 8, 8, 8, 8],
                    species_id: None,
                },
                ReactantInput {
                    display: "NaOH".into(),
                    atomic_numbers: vec![11, 8, 1],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim_json = r#"{
                "schema_version":1,
                "disposition":"reaction",
                "products":[
                    {"name":"sodium sulfate","formula":"Na₂SO₄","phase":"aqueous","identity_hints":[]},
                    {"name":"water","formula":"H₂O","phase":"liquid","identity_hints":[]}
                ],
                "required_context":"representative complete neutralisation",
                "observations":[],
                "sources":[],
                "ambiguity":null
            }"#;
        let claim =
            ProviderClaim::from_json(claim_json.as_bytes(), ClaimMode::Fast).expect("claim");

        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome(&request, claim, &registry()).expect("balanced static outcome")
        else {
            panic!("reaction should compile")
        };
        assert!(outcome.products().iter().any(|product| {
            matches!(
                product,
                OutcomeSpecies::Resolved(species)
                    if species.formula_text == "Na2SO4" && species.structure.is_some()
            )
        }));
        assert_eq!(outcome.claim().products[0].formula, "Na₂SO₄");
    }
}
