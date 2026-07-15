//! Deterministic names derived from validated product structure.

use std::collections::{BTreeMap, BTreeSet};

use chem_kernel::{SimulationFrame, SimulationFrames};

use crate::elements::{self, Category};

pub fn product_names(frames: &SimulationFrames) -> String {
    let Some(frame) = frames.frames().last() else {
        return "reaction products".to_owned();
    };
    let names = frame
        .product_membership()
        .values()
        .map(|atoms| product_name(frame, atoms))
        .collect::<BTreeSet<_>>();
    if names.is_empty() {
        "reaction products".to_owned()
    } else {
        names.into_iter().collect::<Vec<_>>().join(" + ")
    }
}

fn product_name(frame: &SimulationFrame, product_atoms: &BTreeSet<chem_domain::AtomId>) -> String {
    let mut counts = BTreeMap::<String, usize>::new();
    for atom in product_atoms {
        if let Some(atom) = frame.atoms().get(atom) {
            *counts.entry(atom.element.as_str().to_owned()).or_default() += 1;
        }
    }
    if counts.len() == 1 {
        return element_name(counts.keys().next().expect("one element"));
    }

    let ionic = frame.ionic_associations().values().any(|association| {
        association
            .components
            .values()
            .flatten()
            .any(|atom| product_atoms.contains(atom))
    });
    if ionic {
        return ionic_name(frame, product_atoms, &counts);
    }
    molecular_name(&counts)
}

fn ionic_name(
    frame: &SimulationFrame,
    product_atoms: &BTreeSet<chem_domain::AtomId>,
    counts: &BTreeMap<String, usize>,
) -> String {
    let cation_symbol = product_atoms.iter().find_map(|id| {
        let atom = frame.atoms().get(id)?;
        (atom.electrons.formal_charge() > 0).then(|| atom.element.as_str().to_owned())
    });
    let Some(cation_symbol) = cation_symbol else {
        return molecular_name(counts);
    };
    let charges = product_atoms
        .iter()
        .filter_map(|id| frame.atoms().get(id))
        .filter(|atom| atom.element.as_str() == cation_symbol)
        .map(|atom| atom.electrons.formal_charge())
        .filter(|charge| *charge > 0)
        .collect::<BTreeSet<_>>();
    let mut cation = element_name(&cation_symbol);
    let stock_required = charges.len() > 1
        || elements::SUPPORTED
            .iter()
            .find(|element| element.symbol == cation_symbol)
            .is_some_and(|element| {
                matches!(
                    element.category,
                    Category::TransitionMetal | Category::Lanthanide | Category::Actinide
                )
            });
    if stock_required && !charges.is_empty() {
        let numerals = charges
            .iter()
            .map(|charge| roman(*charge))
            .collect::<Vec<_>>()
            .join(",");
        cation.push('(');
        cation.push_str(&numerals);
        cation.push(')');
    }

    let has_hydroxide = counts.contains_key("H")
        && counts.contains_key("O")
        && frame.covalent_edges().values().any(|edge| {
            product_atoms.contains(&edge.left)
                && product_atoms.contains(&edge.right)
                && endpoint_symbols(frame, &edge.left, &edge.right) == Some(("H", "O"))
        });
    let monatomic_anions = product_atoms
        .iter()
        .filter_map(|id| frame.atoms().get(id))
        .filter(|atom| atom.electrons.formal_charge() < 0)
        .map(|atom| atom.element.as_str())
        .collect::<BTreeSet<_>>();
    let anion = if has_hydroxide {
        "hydroxide".to_owned()
    } else if oxygen_pair_bonded(frame, product_atoms) {
        let oxygen_charge = product_atoms
            .iter()
            .filter_map(|id| frame.atoms().get(id))
            .filter(|atom| atom.element.as_str() == "O")
            .map(|atom| i32::from(atom.electrons.formal_charge()))
            .sum::<i32>();
        if oxygen_charge == -1 {
            "superoxide".to_owned()
        } else {
            "peroxide".to_owned()
        }
    } else if monatomic_anions.len() == 1 {
        monatomic_anion_name(
            monatomic_anions
                .iter()
                .next()
                .expect("one monatomic anion"),
        )
    } else {
        "ionic compound".to_owned()
    };
    format!("{cation} {anion}")
}

fn monatomic_anion_name(symbol: &str) -> String {
    let name = element_name(symbol);
    if name == "oxygen" {
        return "oxide".to_owned();
    }
    if let Some(stem) = name.strip_suffix("ine") {
        return format!("{stem}ide");
    }
    if let Some(stem) = name.strip_suffix("ogen") {
        return format!("{stem}ide");
    }
    if let Some(stem) = name.strip_suffix("orus") {
        return format!("{stem}ide");
    }
    if let Some(stem) = name.strip_suffix("ur") {
        return format!("{stem}ide");
    }
    format!("{name}ide")
}

fn oxygen_pair_bonded(
    frame: &SimulationFrame,
    product_atoms: &BTreeSet<chem_domain::AtomId>,
) -> bool {
    frame.covalent_edges().values().any(|edge| {
        product_atoms.contains(&edge.left)
            && product_atoms.contains(&edge.right)
            && endpoint_symbols(frame, &edge.left, &edge.right) == Some(("O", "O"))
    })
}

fn endpoint_symbols<'a>(
    frame: &'a SimulationFrame,
    left: &chem_domain::AtomId,
    right: &chem_domain::AtomId,
) -> Option<(&'a str, &'a str)> {
    let left = frame.atoms().get(left)?.element.as_str();
    let right = frame.atoms().get(right)?.element.as_str();
    if left <= right {
        Some((left, right))
    } else {
        Some((right, left))
    }
}

fn molecular_name(counts: &BTreeMap<String, usize>) -> String {
    let Some((oxygen_symbol, oxygen_count)) = counts.get_key_value("O") else {
        return counts
            .iter()
            .map(|(symbol, count)| format!("{}{}", prefix(*count, false), element_name(symbol)))
            .collect::<Vec<_>>()
            .join(" ");
    };
    let mut non_oxygen = counts.iter().filter(|(symbol, _)| *symbol != oxygen_symbol);
    let Some((first_symbol, first_count)) = non_oxygen.next() else {
        return "oxygen".to_owned();
    };
    let first = format!(
        "{}{}",
        prefix(*first_count, true),
        element_name(first_symbol)
    );
    let oxygen = oxide_prefix(*oxygen_count);
    format!("{first} {oxygen}")
}

fn element_name(symbol: &str) -> String {
    elements::SUPPORTED
        .iter()
        .find(|element| element.symbol == symbol)
        .map_or_else(
            || symbol.to_lowercase(),
            |element| element.name.to_lowercase(),
        )
}

fn prefix(count: usize, omit_mono: bool) -> &'static str {
    match count {
        1 if omit_mono => "",
        1 => "mono",
        2 => "di",
        3 => "tri",
        4 => "tetra",
        5 => "penta",
        6 => "hexa",
        7 => "hepta",
        8 => "octa",
        9 => "nona",
        10 => "deca",
        _ => "",
    }
}

fn oxide_prefix(count: usize) -> String {
    match count {
        1 => "monoxide".to_owned(),
        4 => "tetroxide".to_owned(),
        5 => "pentoxide".to_owned(),
        10 => "decoxide".to_owned(),
        value => format!("{}oxide", prefix(value, false)),
    }
}

fn roman(value: i16) -> String {
    const NUMERALS: &[(i16, &str)] = &[(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut remaining = value;
    let mut result = String::new();
    for (amount, numeral) in NUMERALS {
        while remaining >= *amount {
            result.push_str(numeral);
            remaining -= *amount;
        }
    }
    result
}
