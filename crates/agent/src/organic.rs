//! Functional-group detection and graph rewrites for the classroom organic
//! reaction families. Everything here works on an editable heavy-atom view
//! of a molecular graph and emits products as subset SMILES, which the
//! outcome compiler parses back into validated structures.

use chem_domain::{BondOrder, RepresentationKind, StructureDefinition};

/// Heavy atoms with folded hydrogen counts: the natural unit for
/// functional-group edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Editable {
    pub symbols: Vec<String>,
    pub hydrogens: Vec<u8>,
    /// (left, right, order 1..=3) over heavy-atom indices.
    pub bonds: Vec<(usize, usize, u8)>,
}

impl Editable {
    /// A molecular structure as an editable view. None for ions, salts,
    /// charged atoms, and delocalized (aromatic) systems — the rewrite
    /// families below stay out of that chemistry.
    pub fn from_structure(structure: &StructureDefinition) -> Option<Self> {
        if structure.representation() != RepresentationKind::Molecular {
            return None;
        }
        let graph = structure.graph();
        let mut heavy = Vec::new();
        let mut index_of = std::collections::BTreeMap::new();
        for (id, atom) in graph.atoms() {
            if atom.electrons().formal_charge() != 0 {
                return None;
            }
            if atom.element().as_str() != "H" {
                index_of.insert(id.clone(), heavy.len());
                heavy.push(atom.element().as_str().to_owned());
            }
        }
        let mut hydrogens = vec![0_u8; heavy.len()];
        let mut bonds = Vec::new();
        for bond in graph.covalent_bonds().values() {
            if bond.delocalization().is_some() {
                return None;
            }
            let order = match bond.order() {
                BondOrder::Single => 1,
                BondOrder::Double => 2,
                BondOrder::Triple => 3,
            };
            match (index_of.get(bond.left()), index_of.get(bond.right())) {
                (Some(left), Some(right)) => bonds.push((*left, *right, order)),
                (Some(index), None) | (None, Some(index)) => hydrogens[*index] += 1,
                (None, None) => return None,
            }
        }
        Some(Self {
            symbols: heavy,
            hydrogens,
            bonds,
        })
    }

    pub(crate) fn neighbours(&self, index: usize) -> impl Iterator<Item = (usize, u8)> + '_ {
        self.bonds.iter().filter_map(move |(left, right, order)| {
            if *left == index {
                Some((*right, *order))
            } else if *right == index {
                Some((*left, *order))
            } else {
                None
            }
        })
    }

    fn element_counts(&self) -> std::collections::BTreeMap<String, u64> {
        let mut counts = std::collections::BTreeMap::new();
        for (index, symbol) in self.symbols.iter().enumerate() {
            *counts.entry(symbol.clone()).or_insert(0) += 1;
            *counts.entry("H".to_owned()).or_insert(0) += u64::from(self.hydrogens[index]);
        }
        counts.retain(|_, count| *count > 0);
        counts
    }

    /// Hill-style formula text (C, H, then alphabetical).
    pub fn formula_text(&self) -> String {
        let counts = self.element_counts();
        let mut ordered = Vec::new();
        for lead in ["C", "H"] {
            if let Some(count) = counts.get(lead) {
                ordered.push((lead.to_owned(), *count));
            }
        }
        for (symbol, count) in &counts {
            if symbol != "C" && symbol != "H" {
                ordered.push((symbol.clone(), *count));
            }
        }
        ordered
            .into_iter()
            .map(|(symbol, count)| {
                if count == 1 {
                    symbol
                } else {
                    format!("{symbol}{count}")
                }
            })
            .collect()
    }

    /// Subset SMILES for an acyclic editable molecule. Rewrite products are
    /// all trees; cyclic input returns None.
    // ponytail: tree-only writer; route through chem-domain's ring-aware
    // writer if a family ever emits a cyclic product.
    pub fn to_smiles(&self) -> Option<String> {
        if self.symbols.is_empty() || self.bonds.len() + 1 != self.symbols.len() {
            return None;
        }
        let mut visited = vec![false; self.symbols.len()];
        let mut output = String::new();
        self.emit(0, None, &mut visited, &mut output)?;
        visited.iter().all(|seen| *seen).then_some(output)
    }

    fn emit(
        &self,
        index: usize,
        parent: Option<usize>,
        visited: &mut Vec<bool>,
        output: &mut String,
    ) -> Option<()> {
        if visited[index] {
            return None;
        }
        visited[index] = true;
        output.push_str(&self.atom_text(index));
        let children: Vec<(usize, u8)> = self
            .neighbours(index)
            .filter(|(neighbour, _)| Some(*neighbour) != parent)
            .collect();
        for (position, (child, order)) in children.iter().enumerate() {
            let last = position + 1 == children.len();
            if !last {
                output.push('(');
            }
            output.push_str(match order {
                1 => "",
                2 => "=",
                _ => "#",
            });
            self.emit(*child, Some(index), visited, output)?;
            if !last {
                output.push(')');
            }
        }
        Some(())
    }

    fn atom_text(&self, index: usize) -> String {
        let symbol = &self.symbols[index];
        let heavy_orders: u8 = self.neighbours(index).map(|(_, order)| order).sum();
        let implicit =
            chem_domain::subset_valence(symbol).map(|valence| valence.saturating_sub(heavy_orders));
        if implicit == Some(self.hydrogens[index]) {
            return symbol.clone();
        }
        let hydrogens_text = match self.hydrogens[index] {
            0 => String::new(),
            1 => "H".to_owned(),
            count => format!("H{count}"),
        };
        format!("[{symbol}{hydrogens_text}]")
    }

    /// A canonical key for acyclic molecules (AHU rooted at the tree
    /// centre), used to recognise products by name. None for cyclic input.
    pub fn canonical_key(&self) -> Option<String> {
        let count = self.symbols.len();
        if count == 0 || self.bonds.len() + 1 != count {
            return None;
        }
        // Tree centre by leaf peeling.
        let mut degree: Vec<usize> = (0..count)
            .map(|index| self.neighbours(index).count())
            .collect();
        let mut removed = vec![false; count];
        let mut remaining = count;
        while remaining > 2 {
            let leaves: Vec<usize> = (0..count)
                .filter(|index| !removed[*index] && degree[*index] <= 1)
                .collect();
            for leaf in leaves {
                removed[leaf] = true;
                remaining -= 1;
                for (neighbour, _) in self.neighbours(leaf) {
                    if !removed[neighbour] {
                        degree[neighbour] -= 1;
                    }
                }
            }
        }
        (0..count)
            .filter(|index| !removed[*index])
            .map(|centre| self.rooted_key(centre, None))
            .min()
    }

    fn rooted_key(&self, index: usize, parent: Option<usize>) -> String {
        let mut children: Vec<String> = self
            .neighbours(index)
            .filter(|(neighbour, _)| Some(*neighbour) != parent)
            .map(|(child, order)| format!("{order}{}", self.rooted_key(child, Some(index))))
            .collect();
        children.sort();
        format!(
            "({}{}{})",
            self.symbols[index],
            self.hydrogens[index],
            children.concat()
        )
    }
}

/// An editable view over explicit atoms (hydrogens included) and bonds,
/// as animation frames carry them. None when a hydrogen bridges two atoms
/// or floats unbonded.
pub(crate) fn editable_from_explicit(
    symbols: &[&str],
    bonds: &[(usize, usize, u8)],
) -> Option<Editable> {
    let mut heavy_index = vec![None; symbols.len()];
    let mut heavy = Vec::new();
    for (index, symbol) in symbols.iter().enumerate() {
        if *symbol != "H" {
            heavy_index[index] = Some(heavy.len());
            heavy.push((*symbol).to_owned());
        }
    }
    let mut hydrogens = vec![0_u8; heavy.len()];
    let mut heavy_bonds = Vec::new();
    for (left, right, order) in bonds {
        match (
            heavy_index.get(*left).copied().flatten(),
            heavy_index.get(*right).copied().flatten(),
        ) {
            (Some(left), Some(right)) => heavy_bonds.push((left, right, *order)),
            (Some(index), None) | (None, Some(index)) if *order == 1 => hydrogens[index] += 1,
            _ => return None,
        }
    }
    Some(Editable {
        symbols: heavy,
        hydrogens,
        bonds: heavy_bonds,
    })
}

/// The display name of a molecule matching this editable graph: the named
/// table first (trivial spellings win), then a systematic IUPAC name for
/// the classroom subset. Wrong names are worse than none: table matches
/// are exact (canonical-key), and the systematic namer declines anything
/// outside its subset.
pub(crate) fn recognised_name(molecule: &Editable) -> Option<String> {
    let key = molecule.canonical_key();
    if let Some(key) = &key {
        for (spellings, smiles) in chem_domain::named_molecules() {
            let candidate = chem_domain::structure_from_smiles(
                chem_domain::StructureId::new("organic.name-lookup").ok()?,
                smiles,
            )?;
            if let Some(editable) = Editable::from_structure(&candidate)
                && editable.canonical_key().as_deref() == Some(key.as_str())
            {
                return spellings.first().map(|name| (*name).to_owned());
            }
        }
    }
    crate::iupac_name::systematic_name(molecule)
}

/// The single C=C double bond of a plain alkene: exactly one multiple bond
/// in the molecule, and it joins two carbons.
pub(crate) fn single_alkene(molecule: &Editable) -> Option<(usize, usize)> {
    let mut alkene = None;
    for (left, right, order) in &molecule.bonds {
        if *order == 1 {
            continue;
        }
        if *order != 2
            || molecule.symbols[*left] != "C"
            || molecule.symbols[*right] != "C"
            || alkene.is_some()
        {
            return None;
        }
        alkene = Some((*left, *right));
    }
    alkene
}

/// (carbon, oxygen) pairs for plain hydroxyl groups: an O-H oxygen singly
/// bonded to exactly one carbon that is not a carboxyl carbon.
pub(crate) fn hydroxyls(molecule: &Editable) -> Vec<(usize, usize)> {
    let carboxyl_carbons: Vec<usize> = carboxyls(molecule)
        .into_iter()
        .map(|group| group.carbon)
        .collect();
    (0..molecule.symbols.len())
        .filter_map(|oxygen| {
            if molecule.symbols[oxygen] != "O" || molecule.hydrogens[oxygen] != 1 {
                return None;
            }
            let heavy: Vec<(usize, u8)> = molecule.neighbours(oxygen).collect();
            match heavy.as_slice() {
                [(carbon, 1)]
                    if molecule.symbols[*carbon] == "C" && !carboxyl_carbons.contains(carbon) =>
                {
                    Some((*carbon, oxygen))
                }
                _ => None,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Carboxyl {
    pub carbon: usize,
    pub hydroxyl_oxygen: usize,
}

/// -COOH groups: a carbon double-bonded to one oxygen and single-bonded to
/// an O-H oxygen.
pub(crate) fn carboxyls(molecule: &Editable) -> Vec<Carboxyl> {
    (0..molecule.symbols.len())
        .filter_map(|carbon| {
            if molecule.symbols[carbon] != "C" {
                return None;
            }
            let mut double_oxygen = None;
            let mut hydroxyl_oxygen = None;
            for (neighbour, order) in molecule.neighbours(carbon) {
                if molecule.symbols[neighbour] != "O" {
                    continue;
                }
                match order {
                    2 => double_oxygen = Some(neighbour),
                    1 if molecule.hydrogens[neighbour] == 1
                        && molecule.neighbours(neighbour).count() == 1 =>
                    {
                        hydroxyl_oxygen = Some(neighbour);
                    }
                    _ => {}
                }
            }
            Some(Carboxyl {
                carbon,
                hydroxyl_oxygen: hydroxyl_oxygen?,
            })
            .filter(|_| double_oxygen.is_some())
        })
        .collect()
}

/// Saturates the molecule's single C=C bond, adding one of `attached` to
/// each carbon: H for hydrogenation, a halogen for halogenation.
pub(crate) fn saturate_alkene(molecule: &Editable, attached: &str) -> Option<Editable> {
    let (left, right) = single_alkene(molecule)?;
    let mut product = molecule.clone();
    for bond in &mut product.bonds {
        if (bond.0, bond.1) == (left, right) || (bond.1, bond.0) == (left, right) {
            bond.2 = 1;
        }
    }
    if attached == "H" {
        product.hydrogens[left] += 1;
        product.hydrogens[right] += 1;
    } else {
        for carbon in [left, right] {
            product.symbols.push(attached.to_owned());
            product.hydrogens.push(0);
            product.bonds.push((carbon, product.symbols.len() - 1, 1));
        }
    }
    Some(product)
}

/// Eliminates water from a simple alcohol: the hydroxyl leaves with one
/// hydrogen from an adjacent carbon, leaving a C=C bond. Restricted to
/// single-hydroxyl C/H/O molecules so only classroom alcohols fire.
pub(crate) fn dehydrate(molecule: &Editable) -> Option<Editable> {
    if molecule
        .symbols
        .iter()
        .any(|symbol| symbol != "C" && symbol != "O")
        || molecule
            .symbols
            .iter()
            .filter(|symbol| *symbol == "O")
            .count()
            != 1
        || !carboxyls(molecule).is_empty()
    {
        return None;
    }
    let hydroxyl_groups = hydroxyls(molecule);
    let [(carbon, oxygen)] = hydroxyl_groups.as_slice() else {
        return None;
    };
    let (carbon, oxygen) = (*carbon, *oxygen);
    // Every eliminable neighbour must give the same alkene (ethanol,
    // propan-2-ol); butan-2-ol's two distinct products stay ambiguous and
    // fall to the model — a guessed regiochemistry is worse than none.
    let partners: Vec<usize> = molecule
        .neighbours(carbon)
        .filter(|(neighbour, order)| {
            *order == 1
                && *neighbour != oxygen
                && molecule.symbols[*neighbour] == "C"
                && molecule.hydrogens[*neighbour] > 0
        })
        .map(|(neighbour, _)| neighbour)
        .collect();
    let mut unique: Option<(String, Editable)> = None;
    for partner in partners {
        let mut product = remove_atom(molecule, oxygen);
        let remap = |index: usize| if index > oxygen { index - 1 } else { index };
        let (chain_carbon, partner) = (remap(carbon), remap(partner));
        product.hydrogens[partner] -= 1;
        for bond in &mut product.bonds {
            if (bond.0, bond.1) == (chain_carbon, partner)
                || (bond.1, bond.0) == (chain_carbon, partner)
            {
                bond.2 = 2;
            }
        }
        let key = product.canonical_key()?;
        match &unique {
            None => unique = Some((key, product)),
            Some((existing, _)) if *existing == key => {}
            Some(_) => return None,
        }
    }
    unique.map(|(_, product)| product)
}

/// Condenses a carboxylic acid with an alcohol into the ester: the acid
/// loses its hydroxyl, the alcohol oxygen loses a hydrogen and bridges to
/// the acid carbon.
pub(crate) fn esterify(acid: &Editable, alcohol: &Editable) -> Option<Editable> {
    let organic_only = |molecule: &Editable| {
        molecule
            .symbols
            .iter()
            .all(|symbol| symbol == "C" || symbol == "O")
    };
    if !organic_only(acid) || !organic_only(alcohol) {
        return None;
    }
    let acid_groups = carboxyls(acid);
    let [carboxyl] = acid_groups.as_slice() else {
        return None;
    };
    if !carboxyls(alcohol).is_empty() {
        return None;
    }
    let alcohol_groups = hydroxyls(alcohol);
    let [(_, alcohol_oxygen)] = alcohol_groups.as_slice() else {
        return None;
    };
    let alcohol_oxygen = *alcohol_oxygen;
    if alcohol
        .symbols
        .iter()
        .filter(|symbol| *symbol == "O")
        .count()
        != 1
    {
        return None;
    }
    let mut product = remove_atom(acid, carboxyl.hydroxyl_oxygen);
    let remap = |index: usize| {
        if index > carboxyl.hydroxyl_oxygen {
            index - 1
        } else {
            index
        }
    };
    let acid_carbon = remap(carboxyl.carbon);
    let offset = product.symbols.len();
    product.symbols.extend(alcohol.symbols.iter().cloned());
    product.hydrogens.extend(alcohol.hydrogens.iter().copied());
    product.bonds.extend(
        alcohol
            .bonds
            .iter()
            .map(|(left, right, order)| (left + offset, right + offset, *order)),
    );
    product.hydrogens[offset + alcohol_oxygen] -= 1;
    product
        .bonds
        .push((acid_carbon, offset + alcohol_oxygen, 1));
    Some(product)
}

/// The single monosubstitution product of an alkane, when every choice of
/// carbon gives the same molecule (methane, ethane, neopentane): one
/// hydrogen swaps for the halogen. Positional ambiguity (propane and up)
/// returns None — a guessed isomer is worse than none.
pub(crate) fn unique_monosubstitution(molecule: &Editable, halogen: &str) -> Option<Editable> {
    if molecule.symbols.iter().any(|symbol| symbol != "C")
        || molecule.bonds.iter().any(|(_, _, order)| *order != 1)
    {
        return None;
    }
    let mut unique: Option<(String, Editable)> = None;
    for carbon in 0..molecule.symbols.len() {
        if molecule.hydrogens[carbon] == 0 {
            continue;
        }
        let mut product = molecule.clone();
        product.hydrogens[carbon] -= 1;
        product.symbols.push(halogen.to_owned());
        product.hydrogens.push(0);
        product.bonds.push((carbon, product.symbols.len() - 1, 1));
        let key = product.canonical_key()?;
        match &unique {
            None => unique = Some((key, product)),
            Some((existing, _)) if *existing == key => {}
            Some(_) => return None,
        }
    }
    unique.map(|(_, product)| product)
}

/// Markovnikov hydrohalogenation across the single C=C bond: the hydrogen
/// joins the carbon already richer in hydrogens, the halogen the more
/// substituted one. Equal hydrogen counts fall back to requiring a unique
/// product — a guessed regiochemistry is worse than none.
pub(crate) fn hydrohalogenate(molecule: &Editable, halogen: &str) -> Option<Editable> {
    let (left, right) = single_alkene(molecule)?;
    let build = |hydrogen_to: usize, halogen_to: usize| {
        let mut product = molecule.clone();
        for bond in &mut product.bonds {
            if (bond.0, bond.1) == (left, right) || (bond.1, bond.0) == (left, right) {
                bond.2 = 1;
            }
        }
        product.hydrogens[hydrogen_to] += 1;
        product.symbols.push(halogen.to_owned());
        product.hydrogens.push(0);
        product
            .bonds
            .push((halogen_to, product.symbols.len() - 1, 1));
        product
    };
    match molecule.hydrogens[left].cmp(&molecule.hydrogens[right]) {
        std::cmp::Ordering::Greater => Some(build(left, right)),
        std::cmp::Ordering::Less => Some(build(right, left)),
        std::cmp::Ordering::Equal => {
            let first = build(left, right);
            let second = build(right, left);
            (first.canonical_key()? == second.canonical_key()?).then_some(first)
        }
    }
}

/// The ester linkage of a molecule with exactly one: the bridging oxygen,
/// its acyl (carbonyl-bearing) carbon, and its alkyl carbon.
pub(crate) struct EsterLink {
    pub bridge_oxygen: usize,
    pub acyl_carbon: usize,
}

pub(crate) fn ester_link(molecule: &Editable) -> Option<EsterLink> {
    let mut found = None;
    for oxygen in 0..molecule.symbols.len() {
        if molecule.symbols[oxygen] != "O" || molecule.hydrogens[oxygen] != 0 {
            continue;
        }
        let ends: Vec<usize> = molecule
            .neighbours(oxygen)
            .filter(|(atom, order)| *order == 1 && molecule.symbols[*atom] == "C")
            .map(|(atom, _)| atom)
            .collect();
        let [first, second] = ends.as_slice() else {
            continue;
        };
        if molecule.neighbours(oxygen).count() != 2 {
            continue;
        }
        let carbonyl = |carbon: usize| {
            molecule
                .neighbours(carbon)
                .any(|(atom, order)| order == 2 && molecule.symbols[atom] == "O")
        };
        let acyl_carbon = match (carbonyl(*first), carbonyl(*second)) {
            (true, false) => *first,
            (false, true) => *second,
            _ => continue,
        };
        if found.is_some() {
            return None;
        }
        found = Some(EsterLink {
            bridge_oxygen: oxygen,
            acyl_carbon,
        });
    }
    found
}

/// Hydrolyses the molecule's single ester linkage: the acyl side regains a
/// hydroxyl (from the water) as a carboxylic acid, the bridging oxygen
/// leaves with the alkyl side as the alcohol.
pub(crate) fn hydrolyse_ester(molecule: &Editable) -> Option<(Editable, Editable)> {
    let link = ester_link(molecule)?;
    // Split the graph at the acyl-oxygen bond.
    let mut split = molecule.clone();
    split.bonds.retain(|(l, r, _)| {
        !((*l == link.acyl_carbon && *r == link.bridge_oxygen)
            || (*r == link.acyl_carbon && *l == link.bridge_oxygen))
    });
    split.hydrogens[link.bridge_oxygen] += 1;
    let (mut acid, alcohol_side) = split_components(&split, link.acyl_carbon)?;
    // The acid side gains the water's hydroxyl on its acyl carbon.
    let acyl_local = acid.1;
    acid.0.symbols.push("O".to_owned());
    acid.0.hydrogens.push(1);
    acid.0.bonds.push((acyl_local, acid.0.symbols.len() - 1, 1));
    Some((acid.0, alcohol_side))
}

/// Splits a two-component editable graph: returns (the component holding
/// `marker` with the marker's local index, the other component). None
/// unless there are exactly two components.
fn split_components(molecule: &Editable, marker: usize) -> Option<((Editable, usize), Editable)> {
    let count = molecule.symbols.len();
    let mut component = vec![usize::MAX; count];
    let mut next_label = 0;
    for start in 0..count {
        if component[start] != usize::MAX {
            continue;
        }
        let mut stack = vec![start];
        component[start] = next_label;
        while let Some(current) = stack.pop() {
            for (neighbour, _) in molecule.neighbours(current) {
                if component[neighbour] == usize::MAX {
                    component[neighbour] = next_label;
                    stack.push(neighbour);
                }
            }
        }
        next_label += 1;
    }
    if next_label != 2 {
        return None;
    }
    let marker_label = component[marker];
    let extract = |label: usize| {
        let indices: Vec<usize> = (0..count).filter(|i| component[*i] == label).collect();
        let local = |global: usize| indices.iter().position(|i| *i == global);
        Editable {
            symbols: indices
                .iter()
                .map(|i| molecule.symbols[*i].clone())
                .collect(),
            hydrogens: indices.iter().map(|i| molecule.hydrogens[*i]).collect(),
            bonds: molecule
                .bonds
                .iter()
                .filter_map(|(l, r, order)| Some((local(*l)?, local(*r)?, *order)))
                .collect(),
        }
    };
    let marker_component = extract(marker_label);
    let marker_local = (0..count)
        .filter(|i| component[*i] == marker_label)
        .position(|i| i == marker)?;
    let other = extract(1 - marker_label);
    Some(((marker_component, marker_local), other))
}

/// Catalytic partial oxidation of an alcohol: a primary alcohol loses two
/// hydrogens to become the aldehyde, a secondary one the ketone. Tertiary
/// alcohols (no hydrogen on the carbinol carbon) and polyols stay out.
pub(crate) fn oxidise_alcohol(molecule: &Editable) -> Option<Editable> {
    if molecule
        .symbols
        .iter()
        .filter(|symbol| *symbol == "O")
        .count()
        != 1
        || !carboxyls(molecule).is_empty()
    {
        return None;
    }
    let binding = hydroxyls(molecule);
    let [(carbon, oxygen)] = binding.as_slice() else {
        return None;
    };
    if molecule.hydrogens[*carbon] == 0 {
        return None;
    }
    let mut product = molecule.clone();
    product.hydrogens[*carbon] -= 1;
    product.hydrogens[*oxygen] -= 1;
    for bond in &mut product.bonds {
        if (bond.0, bond.1) == (*carbon, *oxygen) || (bond.1, bond.0) == (*carbon, *oxygen) {
            bond.2 = 2;
        }
    }
    Some(product)
}

/// The molecule with one atom (and its bonds) removed; indices above it
/// shift down by one. Hydrogens folded on the removed atom leave with it.
fn remove_atom(molecule: &Editable, target: usize) -> Editable {
    let remap = |index: usize| if index > target { index - 1 } else { index };
    Editable {
        symbols: molecule
            .symbols
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != target)
            .map(|(_, symbol)| symbol.clone())
            .collect(),
        hydrogens: molecule
            .hydrogens
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != target)
            .map(|(_, count)| *count)
            .collect(),
        bonds: molecule
            .bonds
            .iter()
            .filter(|(left, right, _)| *left != target && *right != target)
            .map(|(left, right, order)| (remap(*left), remap(*right), *order))
            .collect(),
    }
}
