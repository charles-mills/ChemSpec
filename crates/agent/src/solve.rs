//! Algorithmic reaction solving.
//!
//! Predicts products for reaction families that follow deterministically
//! from reactant structure: acid-base neutralization and binary synthesis.
//! The output is an ordinary [`ReactionClaim`]; downstream exact balancing
//! and structural validation gate it exactly like a model claim. Anything
//! outside these families returns None so the caller can fall back to the
//! model.

use std::collections::BTreeMap;

use chem_domain::{
    ElementInventory, ElementSymbol, RepresentationKind, SpeciesRegistry, StructureDefinition,
    StructureId, classify_bronsted_acid, generate_structure,
};

use crate::{
    ClaimDisposition, ClaimPhase, ClaimProduct, OutcomeSpecies, ReactionBuildRequest,
    ReactionClaim, RequestIdentityResolution,
    claim::REACTION_CLAIM_SCHEMA_VERSION, resolve_request_identities,
};

/// Donor elements that make a proton-donor site an acid site in practice;
/// carbon acids stay out so alkanes never neutralize.
const ACIDIC_DONORS: [&str; 6] = ["O", "F", "Cl", "Br", "I", "S"];

/// Attempts to solve the request without a model. Returns a fully formed
/// reaction claim, or None when no deterministic family applies.
#[must_use]
pub fn solve_reaction_claim(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Option<ReactionClaim> {
    if request.reactants.len() != 2 {
        return None;
    }
    let Ok(RequestIdentityResolution::Resolved(species)) =
        resolve_request_identities(request, identities)
    else {
        return None;
    };
    let structures = species
        .iter()
        .map(|entry| match entry {
            OutcomeSpecies::Resolved(resolved) => resolved.structure.as_ref(),
            OutcomeSpecies::FormulaOnly { .. } => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let products = solve_neutralization(structures[0], structures[1])
        .or_else(|| solve_neutralization(structures[1], structures[0]))
        .or_else(|| solve_synthesis(structures[0], structures[1]))?;
    Some(ReactionClaim {
        schema_version: REACTION_CLAIM_SCHEMA_VERSION,
        disposition: ClaimDisposition::Reaction,
        products,
        required_context: String::new(),
        observations: Vec::new(),
        sources: Vec::new(),
        ambiguity: None,
    })
}

/// Acid + metal hydroxide → salt + water.
fn solve_neutralization(
    acid: &StructureDefinition,
    base: &StructureDefinition,
) -> Option<Vec<ClaimProduct>> {
    if acid.representation() != RepresentationKind::Molecular {
        return None;
    }
    let donors = classify_bronsted_acid(acid)
        .proton_donor_sites()
        .iter()
        .filter(|site| ACIDIC_DONORS.contains(&site.donor_element().as_str()))
        .count();
    let donors = u64::try_from(donors).ok()?;
    if donors == 0 || is_water(acid.formula()) {
        return None;
    }
    let (cation, cation_charge) = hydroxide_base_cation(base)?;

    let mut anion = acid
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
        .collect::<BTreeMap<_, _>>();
    let hydrogens = anion.get_mut("H")?;
    *hydrogens = hydrogens.checked_sub(donors)?;
    if *hydrogens == 0 {
        anion.remove("H");
    }
    if anion.is_empty() {
        return None;
    }

    let shared = gcd(cation_charge, donors);
    let mut salt = BTreeMap::new();
    salt.insert(cation.clone(), donors / shared);
    for (symbol, count) in &anion {
        *salt.entry(symbol.clone()).or_insert(0) += count * (cation_charge / shared);
    }
    Some(vec![
        ClaimProduct {
            name: "Water".to_owned(),
            formula: "H2O".to_owned(),
            phase: ClaimPhase::Liquid,
            identity_hints: Vec::new(),
        },
        product_from_counts(&salt, Some(&cation)),
    ])
}

/// Element + element → the unique smallest generable binary compound.
fn solve_synthesis(
    left: &StructureDefinition,
    right: &StructureDefinition,
) -> Option<Vec<ClaimProduct>> {
    let (left_element, _) = single_element(left)?;
    let (right_element, _) = single_element(right)?;
    if left_element == right_element {
        return None;
    }
    let mut best: Option<(u64, BTreeMap<ElementSymbol, u64>, RepresentationKind)> = None;
    let mut tie = false;
    for (i, j) in [(1, 1), (1, 2), (1, 3), (2, 1), (3, 1), (2, 3), (3, 2)] {
        let inventory = ElementInventory::new([
            (left_element.clone(), i),
            (right_element.clone(), j),
        ])
        .ok()?;
        let id = StructureId::new("generated.synthesis").ok()?;
        let Some(candidate) = generate_structure(id, &inventory) else {
            continue;
        };
        let size = i + j;
        match &best {
            Some((existing, counts, _)) if *existing == size => {
                if counts != inventory.elements() {
                    tie = true;
                }
            }
            Some((existing, _, _)) if *existing < size => {}
            _ => {
                best = Some((
                    size,
                    inventory.elements().clone(),
                    candidate.representation(),
                ));
                tie = false;
            }
        }
    }
    let (_, counts, representation) = best?;
    if tie {
        return None;
    }
    let counts = counts
        .iter()
        .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
        .collect::<BTreeMap<_, _>>();
    let cation = (representation == RepresentationKind::Ionic)
        .then(|| cation_of(&counts, &left_element, &right_element))
        .flatten();
    Some(vec![product_from_counts(&counts, cation.as_ref())])
}

/// For an ionic binary product, the metal is whichever element the ionic
/// assembly promoted to a cation; the generator only promotes metals, and a
/// binary compound has exactly one of them: the one that is not a nonmetal
/// anion former. Pick by asking the generator which lone element fails to
/// stand alone as a molecule (metals always do).
fn cation_of(
    counts: &BTreeMap<String, u64>,
    left: &ElementSymbol,
    right: &ElementSymbol,
) -> Option<String> {
    let _ = counts;
    for symbol in [left, right] {
        let inventory = ElementInventory::new([(symbol.clone(), 2)]).ok()?;
        let id = StructureId::new("generated.metal-probe").ok()?;
        if generate_structure(id, &inventory).is_none() {
            return Some(symbol.as_str().to_owned());
        }
    }
    None
}

fn single_element(structure: &StructureDefinition) -> Option<(ElementSymbol, u64)> {
    let elements = structure.formula().elements();
    if elements.len() != 1 {
        return None;
    }
    elements
        .iter()
        .next()
        .map(|(symbol, count)| (symbol.clone(), *count))
}

fn is_water(inventory: &ElementInventory) -> bool {
    let elements = inventory.elements();
    elements.len() == 2
        && elements
            .iter()
            .all(|(symbol, count)| matches!((symbol.as_str(), count), ("H", 2) | ("O", 1)))
}

/// One ionic hydroxide base: every group is either a single-atom cation of
/// one element or an O-H hydroxide unit.
fn hydroxide_base_cation(base: &StructureDefinition) -> Option<(String, u64)> {
    if base.representation() != RepresentationKind::Ionic {
        return None;
    }
    let graph = base.graph();
    let mut cation: Option<(String, u64)> = None;
    let mut hydroxides = 0_u64;
    for group in graph.groups().values() {
        let members = group
            .atoms()
            .iter()
            .map(|id| &graph.atoms()[id])
            .collect::<Vec<_>>();
        let elements = members
            .iter()
            .map(|atom| atom.element().as_str())
            .collect::<Vec<_>>();
        match elements.as_slice() {
            [only] if members[0].electrons().formal_charge() > 0 => {
                let found = (
                    (*only).to_owned(),
                    u64::from(members[0].electrons().formal_charge().unsigned_abs()),
                );
                if cation.get_or_insert_with(|| found.clone()) != &found {
                    return None;
                }
            }
            _ => {
                let mut sorted = elements.clone();
                sorted.sort_unstable();
                if sorted != ["H", "O"] {
                    return None;
                }
                hydroxides += 1;
            }
        }
    }
    (hydroxides > 0).then_some(cation)?
}

/// Salt-style formula text: cation first, then non-O/H elements, then O,
/// then H. Molecular formulas without a cation follow the same tail order
/// with H promoted to the front (`H2O`, `HCl`, `H2S`).
fn product_from_counts(counts: &BTreeMap<String, u64>, cation: Option<&String>) -> ClaimProduct {
    let mut formula = String::new();
    let mut append = |symbol: &str, count: u64| {
        formula.push_str(symbol);
        if count > 1 {
            formula.push_str(&count.to_string());
        }
    };
    if let Some(cation) = cation
        && let Some(count) = counts.get(cation)
    {
        append(cation, *count);
    }
    if cation.is_none()
        && let Some(count) = counts.get("H")
    {
        append("H", *count);
    }
    for (symbol, count) in counts {
        if Some(symbol) == cation || symbol == "O" || symbol == "H" {
            continue;
        }
        append(symbol, *count);
    }
    if let Some(count) = counts.get("O") {
        append("O", *count);
    }
    if cation.is_some()
        && let Some(count) = counts.get("H")
    {
        append("H", *count);
    }
    ClaimProduct {
        name: formula.clone(),
        formula,
        phase: ClaimPhase::Unknown,
        identity_hints: Vec::new(),
    }
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let swap = left % right;
        left = right;
        right = swap;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompiledClaimOutcome, ReactantInput, compile_claim_outcome};

    fn request(reactants: &[(&str, &[u8])]) -> ReactionBuildRequest {
        ReactionBuildRequest {
            reactants: reactants
                .iter()
                .map(|(display, atoms)| ReactantInput {
                    display: (*display).to_owned(),
                    atomic_numbers: atoms.to_vec(),
                    species_id: None,
                })
                .collect(),
            selected_context: None,
        }
    }

    #[test]
    fn sulfuric_acid_and_sodium_hydroxide_solve_without_a_model() {
        let request = request(&[
            ("H₂SO₄", &[1, 1, 16, 8, 8, 8, 8]),
            ("NaOH", &[11, 8, 1]),
        ]);
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.disposition, ClaimDisposition::Reaction);
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "Na2SO4"]);
        let outcome =
            compile_claim_outcome(&request, claim, &registry).expect("balanced outcome");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(outcome.equation().contains("Na2SO4"), "{}", outcome.equation());
        assert!(
            outcome.species_without_structure().is_empty(),
            "every species should carry a generated structure: {:?}",
            outcome.species_without_structure()
        );
    }

    #[test]
    fn hydrochloric_acid_neutralization_solves() {
        let request = request(&[("HCl", &[1, 17]), ("NaOH", &[11, 8, 1])]);
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "NaCl"]);
    }

    #[test]
    fn hydrogen_and_chlorine_synthesize_hydrogen_chloride() {
        let request = request(&[("H₂", &[1, 1]), ("Cl₂", &[17, 17])]);
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].formula, "HCl");
    }

    #[test]
    fn methane_is_not_treated_as_an_acid() {
        let request = request(&[("CH₄", &[6, 1, 1, 1, 1]), ("NaOH", &[11, 8, 1])]);
        assert!(solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn ambiguous_synthesis_is_left_to_the_model() {
        // SO2 and SO3 are both structurally valid; the solver must not guess.
        let request = request(&[("S₈", &[16; 8]), ("O₂", &[8, 8])]);
        assert!(solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none());
    }
}
