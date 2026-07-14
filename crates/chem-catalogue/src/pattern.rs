use std::collections::{BTreeMap, BTreeSet};

use chem_domain::{
    Atom, AtomGroupId, AtomId, BondOrder, ContentDigest, CovalentBond, CovalentBondId,
    CovalentElectronOrigin, ElementSymbol, IonicAssociationId, MetallicDomainId, PremiseId,
    StructuralGraph, StructureId,
};

use super::{
    BondElectronOriginRecord, BondOrderRecord, CatalogueError, CatalogueErrorCode, GraphPatternId,
    GraphPatternRecord, GraphPatternRelationshipRecord, PatternElementRecord,
    PatternVariableRecord, StructuralTraitDefinitionRecord, StructuralTraitId,
    StructuralTraitSiteKindRecord, ValidatedCatalogueBundle, require_premise, validate_label,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternRoleInput {
    pub role: String,
    pub pattern: GraphPatternId,
    pub structure: StructureId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPatternMatchBinding {
    catalogue_digest: ContentDigest,
    roles: BTreeMap<String, RolePatternMatchBinding>,
}

impl GraphPatternMatchBinding {
    #[must_use]
    pub const fn roles(&self) -> &BTreeMap<String, RolePatternMatchBinding> {
        &self.roles
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolePatternMatchBinding {
    pattern: GraphPatternId,
    structure: StructureId,
    atoms: BTreeMap<String, AtomId>,
    covalent_bonds: BTreeMap<String, CovalentBondId>,
    groups: BTreeMap<String, AtomGroupId>,
    ionic_associations: BTreeMap<String, IonicAssociationId>,
    metallic_domains: BTreeMap<String, MetallicDomainId>,
}

impl RolePatternMatchBinding {
    #[must_use]
    pub const fn pattern(&self) -> &GraphPatternId {
        &self.pattern
    }

    #[must_use]
    pub const fn structure(&self) -> &StructureId {
        &self.structure
    }

    #[must_use]
    pub const fn atoms(&self) -> &BTreeMap<String, AtomId> {
        &self.atoms
    }

    #[must_use]
    pub const fn covalent_bonds(&self) -> &BTreeMap<String, CovalentBondId> {
        &self.covalent_bonds
    }

    #[must_use]
    pub const fn groups(&self) -> &BTreeMap<String, AtomGroupId> {
        &self.groups
    }

    #[must_use]
    pub const fn ionic_associations(&self) -> &BTreeMap<String, IonicAssociationId> {
        &self.ionic_associations
    }

    #[must_use]
    pub const fn metallic_domains(&self) -> &BTreeMap<String, MetallicDomainId> {
        &self.metallic_domains
    }
}

impl ValidatedCatalogueBundle {
    #[must_use]
    pub fn graph_pattern(&self, id: &GraphPatternId) -> Option<&GraphPatternRecord> {
        self.graph_patterns
            .get(id)
            .map(|index| &self.document.graph_patterns[*index])
    }

    /// Enumerates provisional graph-pattern matches in canonical binding order.
    ///
    /// The result has no capability to construct or validate a reaction.
    ///
    /// # Errors
    ///
    /// Returns a catalogue error when an input role, pattern, structure, or
    /// required element-parameter binding does not resolve.
    pub fn raw_pattern_matches(
        &self,
        inputs: &[PatternRoleInput],
        element_parameters: &BTreeMap<String, ElementSymbol>,
    ) -> Result<Vec<GraphPatternMatchBinding>, CatalogueError> {
        if inputs.is_empty() {
            return pattern_error("match request has no role-bound patterns");
        }
        let mut role_matches = BTreeMap::<String, Vec<RolePatternMatchBinding>>::new();
        for input in inputs {
            validate_label(&input.role, CatalogueErrorCode::InvalidGraphPattern)?;
            if role_matches.contains_key(&input.role) {
                return pattern_error(format!("duplicate match role `{}`", input.role));
            }
            let pattern = self.graph_pattern(&input.pattern).ok_or_else(|| {
                CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!("graph pattern `{}` does not resolve", input.pattern),
                )
            })?;
            let structure = self.structure(&input.structure).ok_or_else(|| {
                CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!("match structure `{}` does not resolve", input.structure),
                )
            })?;
            role_matches.insert(
                input.role.clone(),
                enumerate_role_matches(
                    pattern,
                    &input.structure,
                    structure.graph(),
                    self,
                    element_parameters,
                )?,
            );
        }

        let mut combined = vec![BTreeMap::new()];
        for (role, matches) in role_matches {
            let mut next = Vec::new();
            for prefix in &combined {
                for role_match in &matches {
                    let mut value = prefix.clone();
                    value.insert(role.clone(), role_match.clone());
                    next.push(value);
                }
            }
            combined = next;
        }
        Ok(combined
            .into_iter()
            .map(|roles| GraphPatternMatchBinding {
                catalogue_digest: self.digest,
                roles,
            })
            .collect())
    }

    /// Tests reactant-graph automorphism equivalence between two raw matches.
    ///
    /// # Errors
    ///
    /// Returns a catalogue error when the match roles, patterns, or structure
    /// identities differ or no longer resolve in this catalogue.
    pub fn pattern_matches_are_automorphism_related(
        &self,
        left: &GraphPatternMatchBinding,
        right: &GraphPatternMatchBinding,
    ) -> Result<bool, CatalogueError> {
        if left.catalogue_digest != self.digest || right.catalogue_digest != self.digest {
            return pattern_error("automorphism comparison contains a foreign catalogue binding");
        }
        if left.roles.keys().collect::<Vec<_>>() != right.roles.keys().collect::<Vec<_>>() {
            return pattern_error("automorphism comparison has different role sets");
        }
        for (role, left_binding) in &left.roles {
            let right_binding = &right.roles[role];
            if left_binding.pattern != right_binding.pattern
                || left_binding.structure != right_binding.structure
            {
                return pattern_error(format!(
                    "automorphism role `{role}` has different pattern or structure identities"
                ));
            }
            let graph = self
                .structure(&left_binding.structure)
                .ok_or_else(|| pattern_error_value("match structure no longer resolves"))?
                .graph();
            if !bindings_related_by_automorphism(graph, left_binding, right_binding) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub(super) fn validate_graph_patterns(
    records: &[GraphPatternRecord],
    elements: &BTreeMap<ElementSymbol, usize>,
    traits: &[StructuralTraitDefinitionRecord],
    trait_index: &BTreeMap<StructuralTraitId, usize>,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<GraphPatternId, usize>, CatalogueError> {
    let mut result = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        if result.insert(record.id.clone(), index).is_some() {
            return super::duplicate_id(&record.id);
        }
        if record.variables.is_empty() || record.premise_ids.is_empty() {
            return pattern_error(format!(
                "pattern `{}` requires variables and premises",
                record.id
            ));
        }
        for premise in &record.premise_ids {
            require_premise(premise, premises)?;
        }
        let mut binding_kinds = BTreeMap::new();
        for (name, variable) in &record.variables {
            validate_label(name, CatalogueErrorCode::InvalidGraphPattern)?;
            binding_kinds.insert(name.clone(), StructuralTraitSiteKindRecord::Atom);
            validate_atom_constraint(&record.id, variable, elements)?;
        }
        for relationship in &record.relationships {
            let name = relationship.binding_name();
            validate_label(name, CatalogueErrorCode::InvalidGraphPattern)?;
            if binding_kinds
                .insert(name.clone(), relationship.binding_kind())
                .is_some()
            {
                return pattern_error(format!("pattern `{}` repeats binding `{name}`", record.id));
            }
        }
        for relationship in &record.relationships {
            validate_relationship(&record.id, relationship, &record.variables, &binding_kinds)?;
        }
        let mut seen_traits = BTreeSet::new();
        for requirement in &record.traits {
            if !seen_traits.insert(requirement.trait_id.clone()) {
                return pattern_error(format!(
                    "pattern `{}` repeats trait `{}`",
                    record.id, requirement.trait_id
                ));
            }
            let definition = trait_index
                .get(&requirement.trait_id)
                .map(|index| &traits[*index])
                .ok_or_else(|| {
                    CatalogueError::new(
                        CatalogueErrorCode::UnknownReference,
                        format!("pattern trait `{}` does not resolve", requirement.trait_id),
                    )
                })?;
            if requirement.sites.keys().collect::<BTreeSet<_>>()
                != definition.sites.keys().collect::<BTreeSet<_>>()
            {
                return pattern_error(format!(
                    "pattern `{}` trait `{}` must bind every declared site exactly once",
                    record.id, requirement.trait_id
                ));
            }
            for (site, binding) in &requirement.sites {
                if binding_kinds.get(binding) != definition.sites.get(site) {
                    return pattern_error(format!(
                        "pattern `{}` trait site `{site}` has the wrong binding kind",
                        record.id
                    ));
                }
            }
        }
    }
    Ok(result)
}

fn validate_atom_constraint(
    pattern: &GraphPatternId,
    variable: &PatternVariableRecord,
    elements: &BTreeMap<ElementSymbol, usize>,
) -> Result<(), CatalogueError> {
    if let Some(element) = &variable.atom.element {
        match element {
            PatternElementRecord::Literal(symbol)
                if !elements.is_empty() && !elements.contains_key(symbol) =>
            {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!("pattern `{pattern}` element `{symbol}` does not resolve"),
                ));
            }
            PatternElementRecord::Parameter(reference) => {
                validate_label(
                    &reference.parameter,
                    CatalogueErrorCode::InvalidGraphPattern,
                )?;
            }
            PatternElementRecord::Literal(_) => {}
        }
    }
    Ok(())
}

fn validate_relationship(
    pattern: &GraphPatternId,
    relationship: &GraphPatternRelationshipRecord,
    variables: &BTreeMap<String, PatternVariableRecord>,
    bindings: &BTreeMap<String, StructuralTraitSiteKindRecord>,
) -> Result<(), CatalogueError> {
    let atom_ref = |name: &str| {
        if variables.contains_key(name) {
            Ok(())
        } else {
            pattern_error(format!(
                "pattern `{pattern}` references unknown atom variable `{name}`"
            ))
        }
    };
    match relationship {
        GraphPatternRelationshipRecord::Covalent {
            left,
            right,
            order,
            electron_origin,
            ..
        } => {
            atom_ref(left)?;
            atom_ref(right)?;
            if left == right {
                return pattern_error(format!("pattern `{pattern}` contains a self-edge"));
            }
            if let BondElectronOriginRecord::Dative { donor, acceptor } = electron_origin
                && (*order != BondOrderRecord::Single
                    || !((donor == left && acceptor == right)
                        || (donor == right && acceptor == left)))
            {
                return pattern_error(format!(
                    "pattern `{pattern}` dative edge must be directed over a single bond"
                ));
            }
        }
        GraphPatternRelationshipRecord::GroupMembership { atoms, .. } => {
            if atoms.is_empty() {
                return pattern_error(format!("pattern `{pattern}` has an empty group match"));
            }
            for atom in atoms {
                atom_ref(atom)?;
            }
        }
        GraphPatternRelationshipRecord::IonicAssociation { groups, .. } => {
            if groups.is_empty()
                || groups
                    .iter()
                    .any(|group| bindings.get(group) != Some(&StructuralTraitSiteKindRecord::Group))
            {
                return pattern_error(format!(
                    "pattern `{pattern}` ionic association has invalid group bindings"
                ));
            }
        }
        GraphPatternRelationshipRecord::MetallicDomain {
            sites,
            delocalized_electrons,
            ..
        } => {
            if sites.is_empty() || *delocalized_electrons == 0 {
                return pattern_error(format!("pattern `{pattern}` metallic domain is empty"));
            }
            for site in sites {
                atom_ref(site)?;
            }
        }
    }
    Ok(())
}

fn enumerate_role_matches(
    pattern: &GraphPatternRecord,
    structure_id: &StructureId,
    graph: &StructuralGraph,
    catalogue: &ValidatedCatalogueBundle,
    element_parameters: &BTreeMap<String, ElementSymbol>,
) -> Result<Vec<RolePatternMatchBinding>, CatalogueError> {
    for variable in pattern.variables.values() {
        if let Some(PatternElementRecord::Parameter(reference)) = &variable.atom.element
            && !element_parameters.contains_key(&reference.parameter)
        {
            return pattern_error(format!(
                "element parameter `{}` has no match binding",
                reference.parameter
            ));
        }
    }
    let variables = pattern.variables.iter().collect::<Vec<_>>();
    let mut atom_bindings = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut result = Vec::new();
    enumerate_atoms(
        0,
        &variables,
        graph,
        pattern,
        structure_id,
        catalogue,
        element_parameters,
        &mut atom_bindings,
        &mut used,
        &mut result,
    )?;
    result.sort_by_key(binding_key);
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn enumerate_atoms(
    index: usize,
    variables: &[(&String, &PatternVariableRecord)],
    graph: &StructuralGraph,
    pattern: &GraphPatternRecord,
    structure_id: &StructureId,
    catalogue: &ValidatedCatalogueBundle,
    element_parameters: &BTreeMap<String, ElementSymbol>,
    bindings: &mut BTreeMap<String, AtomId>,
    used: &mut BTreeSet<AtomId>,
    output: &mut Vec<RolePatternMatchBinding>,
) -> Result<(), CatalogueError> {
    if index == variables.len() {
        enumerate_relationships(pattern, structure_id, graph, catalogue, bindings, output);
        return Ok(());
    }
    let (name, variable) = variables[index];
    for (atom_id, atom) in graph.atoms() {
        if !used.contains(atom_id) && atom_matches(atom, variable, graph, element_parameters)? {
            used.insert(atom_id.clone());
            bindings.insert(name.clone(), atom_id.clone());
            enumerate_atoms(
                index + 1,
                variables,
                graph,
                pattern,
                structure_id,
                catalogue,
                element_parameters,
                bindings,
                used,
                output,
            )?;
            bindings.remove(name);
            used.remove(atom_id);
        }
    }
    Ok(())
}

fn atom_matches(
    atom: &Atom,
    variable: &PatternVariableRecord,
    graph: &StructuralGraph,
    element_parameters: &BTreeMap<String, ElementSymbol>,
) -> Result<bool, CatalogueError> {
    let constraint = &variable.atom;
    let expected_element = match &constraint.element {
        Some(PatternElementRecord::Literal(value)) => Some(value),
        Some(PatternElementRecord::Parameter(reference)) => Some(
            element_parameters
                .get(&reference.parameter)
                .ok_or_else(|| {
                    pattern_error_value(format!(
                        "element parameter `{}` has no match binding",
                        reference.parameter
                    ))
                })?,
        ),
        None => None,
    };
    let electrons = atom.electrons();
    Ok(expected_element.is_none_or(|value| value == atom.element())
        && constraint
            .formal_charge
            .is_none_or(|value| value == electrons.formal_charge())
        && constraint
            .non_bonding_electrons
            .is_none_or(|value| value == electrons.non_bonding_electrons())
        && constraint
            .unpaired_electrons
            .is_none_or(|value| value == electrons.unpaired_electrons())
        && constraint.bond_order_sum.is_none_or(|value| {
            u64::from(value)
                == graph
                    .covalent_bond_order_sum(atom.id())
                    .expect("matched atom belongs to graph")
        }))
}

#[allow(clippy::too_many_lines)]
fn enumerate_relationships(
    pattern: &GraphPatternRecord,
    structure_id: &StructureId,
    graph: &StructuralGraph,
    catalogue: &ValidatedCatalogueBundle,
    atoms: &BTreeMap<String, AtomId>,
    output: &mut Vec<RolePatternMatchBinding>,
) {
    let mut bonds = BTreeMap::new();
    for relationship in &pattern.relationships {
        let GraphPatternRelationshipRecord::Covalent {
            bond,
            left,
            right,
            order,
            electron_origin,
        } = relationship
        else {
            continue;
        };
        let Some(actual) = find_matching_bond(
            graph,
            left,
            right,
            &atoms[left],
            &atoms[right],
            *order,
            electron_origin,
        ) else {
            return;
        };
        bonds.insert(bond.clone(), actual.id().clone());
    }

    let group_records = pattern
        .relationships
        .iter()
        .filter_map(|relationship| match relationship {
            GraphPatternRelationshipRecord::GroupMembership { group, atoms } => {
                Some((group, atoms))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut groups = Vec::new();
    enumerate_groups(
        0,
        &group_records,
        graph,
        atoms,
        &mut BTreeMap::new(),
        &mut groups,
    );
    for groups in groups {
        let association_records = pattern
            .relationships
            .iter()
            .filter_map(|relationship| match relationship {
                GraphPatternRelationshipRecord::IonicAssociation {
                    association,
                    groups,
                } => Some((association, groups)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut associations = Vec::new();
        enumerate_associations(
            0,
            &association_records,
            graph,
            &groups,
            &mut BTreeMap::new(),
            &mut associations,
        );
        for ionic_associations in associations {
            let domain_records = pattern
                .relationships
                .iter()
                .filter_map(|relationship| match relationship {
                    GraphPatternRelationshipRecord::MetallicDomain {
                        domain,
                        sites,
                        delocalized_electrons,
                    } => Some((domain, sites, *delocalized_electrons)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut domains = Vec::new();
            enumerate_domains(
                0,
                &domain_records,
                graph,
                atoms,
                &mut BTreeMap::new(),
                &mut domains,
            );
            for metallic_domains in domains {
                let binding = RolePatternMatchBinding {
                    pattern: pattern.id.clone(),
                    structure: structure_id.clone(),
                    atoms: atoms.clone(),
                    covalent_bonds: bonds.clone(),
                    groups: groups.clone(),
                    ionic_associations: ionic_associations.clone(),
                    metallic_domains,
                };
                if traits_match(pattern, structure_id, catalogue, &binding) {
                    output.push(binding);
                }
            }
        }
    }
}

fn find_matching_bond<'a>(
    graph: &'a StructuralGraph,
    left_name: &str,
    right_name: &str,
    left: &AtomId,
    right: &AtomId,
    order: BondOrderRecord,
    origin: &BondElectronOriginRecord,
) -> Option<&'a CovalentBond> {
    graph.covalent_bonds().values().find(|bond| {
        ((bond.left() == left && bond.right() == right)
            || (bond.left() == right && bond.right() == left))
            && bond.order() == domain_bond_order(order)
            && match (origin, bond.electron_origin()) {
                (BondElectronOriginRecord::Shared, CovalentElectronOrigin::Shared) => true,
                (
                    BondElectronOriginRecord::Dative { donor, acceptor },
                    CovalentElectronOrigin::Dative {
                        donor: actual_donor,
                        acceptor: actual_acceptor,
                    },
                ) => {
                    actual_donor
                        == if donor == left_name {
                            left
                        } else if donor == right_name {
                            right
                        } else {
                            return false;
                        }
                        && actual_acceptor
                            == if acceptor == left_name {
                                left
                            } else if acceptor == right_name {
                                right
                            } else {
                                return false;
                            }
                }
                _ => false,
            }
    })
}

fn enumerate_groups(
    index: usize,
    records: &[(&String, &BTreeSet<String>)],
    graph: &StructuralGraph,
    atoms: &BTreeMap<String, AtomId>,
    bindings: &mut BTreeMap<String, AtomGroupId>,
    output: &mut Vec<BTreeMap<String, AtomGroupId>>,
) {
    if index == records.len() {
        output.push(bindings.clone());
        return;
    }
    let (name, members) = records[index];
    let required = members
        .iter()
        .map(|member| &atoms[member])
        .collect::<BTreeSet<_>>();
    for (id, group) in graph.groups() {
        if required.iter().all(|atom| group.atoms().contains(*atom)) {
            bindings.insert(name.clone(), id.clone());
            enumerate_groups(index + 1, records, graph, atoms, bindings, output);
            bindings.remove(name);
        }
    }
}

fn enumerate_associations(
    index: usize,
    records: &[(&String, &BTreeSet<String>)],
    graph: &StructuralGraph,
    groups: &BTreeMap<String, AtomGroupId>,
    bindings: &mut BTreeMap<String, IonicAssociationId>,
    output: &mut Vec<BTreeMap<String, IonicAssociationId>>,
) {
    if index == records.len() {
        output.push(bindings.clone());
        return;
    }
    let (name, components) = records[index];
    let required = components
        .iter()
        .map(|component| &groups[component])
        .collect::<BTreeSet<_>>();
    for (id, association) in graph.ionic_associations() {
        if required
            .iter()
            .all(|component| association.components().contains(*component))
        {
            bindings.insert(name.clone(), id.clone());
            enumerate_associations(index + 1, records, graph, groups, bindings, output);
            bindings.remove(name);
        }
    }
}

fn enumerate_domains(
    index: usize,
    records: &[(&String, &BTreeSet<String>, u32)],
    graph: &StructuralGraph,
    atoms: &BTreeMap<String, AtomId>,
    bindings: &mut BTreeMap<String, MetallicDomainId>,
    output: &mut Vec<BTreeMap<String, MetallicDomainId>>,
) {
    if index == records.len() {
        output.push(bindings.clone());
        return;
    }
    let (name, sites, electrons) = &records[index];
    let required = sites
        .iter()
        .map(|site| &atoms[site])
        .collect::<BTreeSet<_>>();
    for (id, domain) in graph.metallic_domains() {
        if domain.delocalized_electrons() == *electrons
            && required.iter().all(|site| domain.sites().contains(*site))
        {
            bindings.insert((*name).clone(), id.clone());
            enumerate_domains(index + 1, records, graph, atoms, bindings, output);
            bindings.remove(*name);
        }
    }
}

fn traits_match(
    pattern: &GraphPatternRecord,
    structure_id: &StructureId,
    catalogue: &ValidatedCatalogueBundle,
    binding: &RolePatternMatchBinding,
) -> bool {
    pattern.traits.iter().all(|required| {
        let Some(assertion) = catalogue.structure_trait_assertion(structure_id, &required.trait_id)
        else {
            return false;
        };
        required.sites.iter().all(|(trait_site, pattern_binding)| {
            let expected = binding
                .atoms
                .get(pattern_binding)
                .map(ToString::to_string)
                .or_else(|| {
                    binding
                        .covalent_bonds
                        .get(pattern_binding)
                        .map(ToString::to_string)
                })
                .or_else(|| binding.groups.get(pattern_binding).map(ToString::to_string))
                .or_else(|| {
                    binding
                        .ionic_associations
                        .get(pattern_binding)
                        .map(ToString::to_string)
                })
                .or_else(|| {
                    binding
                        .metallic_domains
                        .get(pattern_binding)
                        .map(ToString::to_string)
                });
            expected.as_ref() == assertion.sites.get(trait_site)
        })
    })
}

fn binding_key(binding: &RolePatternMatchBinding) -> Vec<String> {
    binding
        .atoms
        .values()
        .map(ToString::to_string)
        .chain(binding.covalent_bonds.values().map(ToString::to_string))
        .chain(binding.groups.values().map(ToString::to_string))
        .chain(binding.ionic_associations.values().map(ToString::to_string))
        .chain(binding.metallic_domains.values().map(ToString::to_string))
        .collect()
}

fn bindings_related_by_automorphism(
    graph: &StructuralGraph,
    left: &RolePatternMatchBinding,
    right: &RolePatternMatchBinding,
) -> bool {
    if left.atoms.keys().collect::<Vec<_>>() != right.atoms.keys().collect::<Vec<_>>() {
        return false;
    }
    let required = left
        .atoms
        .iter()
        .map(|(name, atom)| (atom.clone(), right.atoms[name].clone()))
        .collect::<BTreeMap<_, _>>();
    let sources = graph.atoms().keys().cloned().collect::<Vec<_>>();
    automorphism_search(
        0,
        &sources,
        graph,
        left,
        right,
        &required,
        &mut BTreeMap::new(),
        &mut BTreeSet::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn automorphism_search(
    index: usize,
    sources: &[AtomId],
    graph: &StructuralGraph,
    left: &RolePatternMatchBinding,
    right: &RolePatternMatchBinding,
    required: &BTreeMap<AtomId, AtomId>,
    mapping: &mut BTreeMap<AtomId, AtomId>,
    used: &mut BTreeSet<AtomId>,
) -> bool {
    if index == sources.len() {
        return automorphism_preserves_relationships(graph, mapping)
            && automorphism_preserves_bindings(graph, left, right, mapping);
    }
    let source = &sources[index];
    let candidates = if let Some(target) = required.get(source) {
        vec![target]
    } else {
        graph.atoms().keys().collect()
    };
    for target in candidates {
        if used.contains(target)
            || atom_signature(&graph.atoms()[source]) != atom_signature(&graph.atoms()[target])
            || !partial_bonds_preserved(graph, source, target, mapping)
        {
            continue;
        }
        mapping.insert(source.clone(), target.clone());
        used.insert(target.clone());
        if automorphism_search(
            index + 1,
            sources,
            graph,
            left,
            right,
            required,
            mapping,
            used,
        ) {
            return true;
        }
        mapping.remove(source);
        used.remove(target);
    }
    false
}

fn atom_signature(atom: &Atom) -> (&ElementSymbol, i16, u8, u8) {
    (
        atom.element(),
        atom.electrons().formal_charge(),
        atom.electrons().non_bonding_electrons(),
        atom.electrons().unpaired_electrons(),
    )
}

fn partial_bonds_preserved(
    graph: &StructuralGraph,
    source: &AtomId,
    target: &AtomId,
    mapping: &BTreeMap<AtomId, AtomId>,
) -> bool {
    mapping.iter().all(|(other_source, other_target)| {
        edge_signature(graph, source, other_source) == edge_signature(graph, target, other_target)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EdgeSignature {
    order: BondOrder,
    direction: i8,
}

fn edge_signature(
    graph: &StructuralGraph,
    first: &AtomId,
    second: &AtomId,
) -> Option<EdgeSignature> {
    graph.covalent_bonds().values().find_map(|bond| {
        if !((bond.left() == first && bond.right() == second)
            || (bond.left() == second && bond.right() == first))
        {
            return None;
        }
        let direction = match bond.electron_origin() {
            CovalentElectronOrigin::Shared => 0,
            CovalentElectronOrigin::Dative { donor, .. } if donor == first => 1,
            CovalentElectronOrigin::Dative { .. } => -1,
        };
        Some(EdgeSignature {
            order: bond.order(),
            direction,
        })
    })
}

fn automorphism_preserves_relationships(
    graph: &StructuralGraph,
    mapping: &BTreeMap<AtomId, AtomId>,
) -> bool {
    let mapped_groups = graph
        .groups()
        .values()
        .map(|group| {
            group
                .atoms()
                .iter()
                .map(|atom| mapping[atom].clone())
                .collect()
        })
        .collect::<Vec<BTreeSet<_>>>();
    let mut mapped_groups = mapped_groups;
    mapped_groups.sort();
    let mut groups = graph
        .groups()
        .values()
        .map(|group| group.atoms().clone())
        .collect::<Vec<_>>();
    groups.sort();
    if mapped_groups != groups {
        return false;
    }

    let association_signature = |mapped: bool| {
        let mut signatures = graph
            .ionic_associations()
            .values()
            .map(|association| {
                association
                    .components()
                    .iter()
                    .map(|group| {
                        graph.groups()[group]
                            .atoms()
                            .iter()
                            .map(|atom| {
                                if mapped {
                                    mapping[atom].clone()
                                } else {
                                    atom.clone()
                                }
                            })
                            .collect::<BTreeSet<_>>()
                    })
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        signatures.sort();
        signatures
    };
    if association_signature(true) != association_signature(false) {
        return false;
    }

    let domain_signature = |mapped: bool| {
        let mut signatures = graph
            .metallic_domains()
            .values()
            .map(|domain| {
                (
                    domain
                        .sites()
                        .iter()
                        .map(|atom| {
                            if mapped {
                                mapping[atom].clone()
                            } else {
                                atom.clone()
                            }
                        })
                        .collect::<BTreeSet<_>>(),
                    domain.delocalized_electrons(),
                )
            })
            .collect::<Vec<_>>();
        signatures.sort();
        signatures
    };
    domain_signature(true) == domain_signature(false)
}

fn automorphism_preserves_bindings(
    graph: &StructuralGraph,
    left: &RolePatternMatchBinding,
    right: &RolePatternMatchBinding,
    mapping: &BTreeMap<AtomId, AtomId>,
) -> bool {
    for (name, left_group) in &left.groups {
        let Some(right_group) = right.groups.get(name) else {
            return false;
        };
        let mapped = graph.groups()[left_group]
            .atoms()
            .iter()
            .map(|atom| mapping[atom].clone())
            .collect::<BTreeSet<_>>();
        if mapped != *graph.groups()[right_group].atoms() {
            return false;
        }
    }
    for (name, left_domain) in &left.metallic_domains {
        let Some(right_domain) = right.metallic_domains.get(name) else {
            return false;
        };
        let left_domain = &graph.metallic_domains()[left_domain];
        let right_domain = &graph.metallic_domains()[right_domain];
        let mapped = left_domain
            .sites()
            .iter()
            .map(|atom| mapping[atom].clone())
            .collect::<BTreeSet<_>>();
        if mapped != *right_domain.sites()
            || left_domain.delocalized_electrons() != right_domain.delocalized_electrons()
        {
            return false;
        }
    }
    for (name, left_bond) in &left.covalent_bonds {
        let Some(right_bond) = right.covalent_bonds.get(name) else {
            return false;
        };
        let left_bond = &graph.covalent_bonds()[left_bond];
        let right_bond = &graph.covalent_bonds()[right_bond];
        if edge_signature(graph, left_bond.left(), left_bond.right())
            != edge_signature(
                graph,
                &mapping[left_bond.left()],
                &mapping[left_bond.right()],
            )
            || !((right_bond.left() == &mapping[left_bond.left()]
                && right_bond.right() == &mapping[left_bond.right()])
                || (right_bond.left() == &mapping[left_bond.right()]
                    && right_bond.right() == &mapping[left_bond.left()]))
        {
            return false;
        }
    }
    for (name, left_association) in &left.ionic_associations {
        let Some(right_association) = right.ionic_associations.get(name) else {
            return false;
        };
        let component_sets = |association: &IonicAssociationId, mapped: bool| {
            graph.ionic_associations()[association]
                .components()
                .iter()
                .map(|group| {
                    graph.groups()[group]
                        .atoms()
                        .iter()
                        .map(|atom| {
                            if mapped {
                                mapping[atom].clone()
                            } else {
                                atom.clone()
                            }
                        })
                        .collect::<BTreeSet<_>>()
                })
                .collect::<BTreeSet<_>>()
        };
        if component_sets(left_association, true) != component_sets(right_association, false) {
            return false;
        }
    }
    true
}

const fn domain_bond_order(order: BondOrderRecord) -> BondOrder {
    match order {
        BondOrderRecord::Single => BondOrder::Single,
        BondOrderRecord::Double => BondOrder::Double,
        BondOrderRecord::Triple => BondOrder::Triple,
    }
}

fn pattern_error<T>(message: impl Into<String>) -> Result<T, CatalogueError> {
    Err(pattern_error_value(message))
}

fn pattern_error_value(message: impl Into<String>) -> CatalogueError {
    CatalogueError::new(CatalogueErrorCode::InvalidGraphPattern, message)
}
