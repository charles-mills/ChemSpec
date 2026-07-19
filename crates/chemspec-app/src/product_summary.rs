//! Post-simulation product presentation compiled from validated final frames.
//!
//! This module performs deterministic layout and display formatting only. It
//! never parses source, selects a reaction, or invents a chemical property.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use chem_domain::AtomId;
use chem_kernel::{SimulationFrame, SimulationFrames};
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};

use crate::fonts;
use crate::settings::ChemicalLabels;
use crate::theme::{LAB_DARK, chemistry_color, color};

#[derive(Debug, Clone)]
pub struct SummaryData {
    pub products: Vec<ProductModel>,
}

#[derive(Debug, Clone)]
pub struct ProductModel {
    pub name: String,
    pub formula: String,
    pub coefficient: usize,
    pub classification: &'static str,
    pub composition: String,
    pub atom_count: usize,
    pub bond_count: usize,
    pub net_charge: i32,
    pub molar_mass: String,
    atoms: Vec<VisualAtom>,
    bonds: Vec<VisualBond>,
    ionic: bool,
}

#[derive(Debug, Clone)]
struct VisualAtom {
    symbol: String,
    formal_charge: i16,
    non_bonding_electrons: u8,
}

#[derive(Debug, Clone)]
struct VisualBond {
    left: usize,
    right: usize,
    order: u8,
}

impl SummaryData {
    #[must_use]
    pub fn from_frames(frames: &SimulationFrames) -> Option<Self> {
        let frame = frames.frames().last()?;
        let mut grouped = BTreeMap::<String, ProductModel>::new();
        for atoms in frame.product_membership().values() {
            let model = ProductModel::from_membership(frame, atoms);
            let signature = model.signature();
            grouped
                .entry(signature)
                .and_modify(|existing| existing.coefficient += 1)
                .or_insert(model);
        }
        (!grouped.is_empty()).then(|| Self {
            products: grouped.into_values().collect(),
        })
    }
}

impl ProductModel {
    fn from_membership(frame: &SimulationFrame, membership: &BTreeSet<AtomId>) -> Self {
        let mut counts = BTreeMap::<String, usize>::new();
        let mut atom_indices = BTreeMap::<String, usize>::new();
        let mut atoms = Vec::new();
        let mut net_charge = 0_i32;
        for atom_id in membership {
            let Some(atom) = frame.atoms().get(atom_id) else {
                continue;
            };
            let index = atoms.len();
            atom_indices.insert(atom_id.as_str().to_owned(), index);
            *counts.entry(atom.element.as_str().to_owned()).or_default() += 1;
            net_charge += i32::from(atom.electrons.formal_charge());
            atoms.push(VisualAtom {
                symbol: atom.element.as_str().to_owned(),
                formal_charge: atom.electrons.formal_charge(),
                non_bonding_electrons: atom.electrons.non_bonding_electrons(),
            });
        }
        let bonds = frame
            .covalent_edges()
            .values()
            .filter_map(|bond| {
                let left = atom_indices.get(bond.left.as_str()).copied()?;
                let right = atom_indices.get(bond.right.as_str()).copied()?;
                Some(VisualBond {
                    left,
                    right,
                    order: bond.order.order(),
                })
            })
            .collect::<Vec<_>>();
        let ionic = frame.ionic_associations().values().any(|association| {
            association
                .components
                .values()
                .any(|component| component.iter().any(|atom| membership.contains(atom)))
        });
        let metallic = frame
            .metallic_domains()
            .values()
            .any(|domain| domain.sites.iter().any(|atom| membership.contains(atom)));
        let classification = if ionic {
            "Ionic assembly"
        } else if metallic {
            "Metallic structure"
        } else if !bonds.is_empty() {
            "Covalent molecule"
        } else if atoms.len() == 1 {
            "Atomic product"
        } else {
            "Molecular assembly"
        };
        let formula = product_formula(frame, membership, &counts, ionic);
        Self {
            name: crate::nomenclature::product_name(frame, membership),
            formula,
            coefficient: 1,
            classification,
            composition: composition(&counts),
            atom_count: atoms.len(),
            bond_count: bonds.len(),
            net_charge,
            molar_mass: molar_mass(&counts),
            atoms,
            bonds,
            ionic,
        }
    }

    fn signature(&self) -> String {
        let mut bonds = self
            .bonds
            .iter()
            .map(|bond| {
                let mut endpoints = [
                    self.atoms[bond.left].symbol.as_str(),
                    self.atoms[bond.right].symbol.as_str(),
                ];
                endpoints.sort_unstable();
                format!("{}-{}:{}", endpoints[0], endpoints[1], bond.order)
            })
            .collect::<Vec<_>>();
        bonds.sort();
        format!(
            "{}|{}|{}|{}|{}",
            self.formula,
            self.classification,
            self.net_charge,
            self.atom_count,
            bonds.join(",")
        )
    }

    #[must_use]
    pub fn primary_label(&self, labels: ChemicalLabels) -> String {
        let mut label = match labels {
            ChemicalLabels::Formulae => self.formula.clone(),
            ChemicalLabels::Names => title_case(&self.name),
        };
        if self.coefficient > 1 {
            let _ = write!(label, "  ×{}", self.coefficient);
        }
        label
    }

    #[must_use]
    pub fn property_rows(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Formula", self.formula.clone()),
            ("Structure", self.classification.to_owned()),
            ("Composition", self.composition.clone()),
            ("Validated atoms", self.atom_count.to_string()),
            ("Covalent bonds", self.bond_count.to_string()),
            ("Net formal charge", format_charge(self.net_charge)),
            ("Reference molar mass", self.molar_mass.clone()),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct Product3dScene {
    data: SummaryData,
    elapsed_ms: u64,
    labels: ChemicalLabels,
}

impl Product3dScene {
    #[must_use]
    pub const fn new(data: SummaryData, elapsed_ms: u64, labels: ChemicalLabels) -> Self {
        Self {
            data,
            elapsed_ms,
            labels,
        }
    }
}

impl<Message> canvas::Program<Message> for Product3dScene {
    type State = ();

    #[allow(clippy::cast_precision_loss)]
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        draw_field(&mut frame, bounds.size(), 0.34);
        let slots = product_slots(bounds.size(), self.data.products.len());
        let rotation = self.elapsed_ms as f32 / 18_000.0 * std::f32::consts::TAU;
        for (index, (product, center)) in self.data.products.iter().zip(slots).enumerate() {
            let phase = rotation + index as f32 * 0.43;
            draw_product_3d(
                &mut frame,
                product,
                center,
                phase,
                bounds.size(),
                self.labels,
            );
        }
        vec![frame.into_geometry()]
    }
}

fn draw_field(frame: &mut canvas::Frame, size: Size, strength: f32) {
    let center = Point::new(size.width * 0.5, size.height * 0.5);
    let radius = size.width.max(size.height) * 0.42;
    frame.fill(
        &Path::circle(center, radius),
        color::ACCENT_FAINT.scale_alpha(strength),
    );
    let grid = color::LINE.scale_alpha(0.18);
    let mut x = 0.0;
    while x <= size.width {
        frame.stroke(
            &Path::line(Point::new(x, 0.0), Point::new(x, size.height)),
            Stroke::default().with_color(grid).with_width(0.5),
        );
        x += 52.0;
    }
    let mut y = 0.0;
    while y <= size.height {
        frame.stroke(
            &Path::line(Point::new(0.0, y), Point::new(size.width, y)),
            Stroke::default().with_color(grid).with_width(0.5),
        );
        y += 52.0;
    }
}

#[allow(clippy::cast_precision_loss)]
fn product_slots(size: Size, count: usize) -> Vec<Point> {
    let count = count.max(1);
    if count == 1 {
        return vec![Point::new(size.width * 0.5, size.height * 0.5)];
    }
    let columns = count.min(3);
    let rows = count.div_ceil(columns);
    let horizontal_span = if columns == 1 { 0.0 } else { 0.32 };
    let vertical_span = if rows == 1 { 0.0 } else { 0.30 };
    (0..count)
        .map(|index| {
            let column = index % columns;
            let row = index / columns;
            Point::new(
                size.width
                    * (0.5 - horizontal_span * 0.5
                        + horizontal_span * column as f32 / (columns - 1).max(1) as f32),
                size.height
                    * (0.5 - vertical_span * 0.5
                        + vertical_span * row as f32 / (rows - 1).max(1) as f32),
            )
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn model_coordinates(product: &ProductModel) -> Vec<[f32; 3]> {
    let count = product.atoms.len();
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![[0.0, 0.0, 0.0]];
    }
    let mut adjacency = vec![Vec::<usize>::new(); count];
    for bond in &product.bonds {
        adjacency[bond.left].push(bond.right);
        adjacency[bond.right].push(bond.left);
    }
    let mut components = Vec::<Vec<usize>>::new();
    let mut visited = vec![false; count];
    for start in 0..count {
        if visited[start] {
            continue;
        }
        let mut component = Vec::new();
        let mut pending = vec![start];
        visited[start] = true;
        while let Some(atom) = pending.pop() {
            component.push(atom);
            for &neighbour in &adjacency[atom] {
                if !visited[neighbour] {
                    visited[neighbour] = true;
                    pending.push(neighbour);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    let mut positions = vec![[0.0, 0.0, 0.0]; count];
    let component_count = components.len();
    for (component_index, component) in components.iter().enumerate() {
        let component_offset = if component_count == 1 {
            0.0
        } else {
            (component_index as f32 - (component_count - 1) as f32 * 0.5) * 1.35
        };
        if component.len() == 1 {
            positions[component[0]] = [component_offset, 0.0, 0.0];
            continue;
        }
        if component.len() == 2 {
            positions[component[0]] = [component_offset - 0.52, 0.0, 0.0];
            positions[component[1]] = [component_offset + 0.52, 0.0, 0.0];
            continue;
        }
        let center_index = component
            .iter()
            .copied()
            .max_by_key(|index| (adjacency[*index].len(), std::cmp::Reverse(*index)))
            .unwrap_or(component[0]);
        positions[center_index] = [component_offset, 0.0, 0.0];
        let satellites = component
            .iter()
            .copied()
            .filter(|index| *index != center_index)
            .collect::<Vec<_>>();
        let bent = adjacency[center_index].len() == 2
            && product.atoms[center_index].non_bonding_electrons >= 2
            && satellites.len() == 2;
        let trigonal_pyramidal = adjacency[center_index].len() == 3
            && product.atoms[center_index].non_bonding_electrons >= 2
            && satellites.len() == 3;
        let tetrahedral = adjacency[center_index].len() == 4 && satellites.len() == 4;
        for (satellite, atom_index) in satellites.into_iter().enumerate() {
            let vector = if tetrahedral {
                const TETRAHEDRAL: [[f32; 3]; 4] = [
                    [0.577_350_26, 0.577_350_26, 0.577_350_26],
                    [-0.577_350_26, -0.577_350_26, 0.577_350_26],
                    [-0.577_350_26, 0.577_350_26, -0.577_350_26],
                    [0.577_350_26, -0.577_350_26, -0.577_350_26],
                ];
                TETRAHEDRAL[satellite]
            } else if trigonal_pyramidal {
                let angle = std::f32::consts::TAU * satellite as f32 / 3.0;
                [angle.cos() * 0.928, angle.sin() * 0.928, -0.373]
            } else if bent {
                let half_angle = 104.5_f32.to_radians() * 0.5;
                let angle = if satellite == 0 {
                    std::f32::consts::FRAC_PI_2 - half_angle
                } else {
                    std::f32::consts::FRAC_PI_2 + half_angle
                };
                [angle.cos(), angle.sin(), 0.0]
            } else if component.len() == 3 && adjacency[center_index].len() == 2 {
                let angle = std::f32::consts::PI * satellite as f32;
                [angle.cos(), angle.sin(), 0.0]
            } else {
                let angle = std::f32::consts::TAU * satellite as f32 / (component.len() - 1) as f32;
                let z = ((satellite as f32 * 2.399_963_1).sin() * 0.42).clamp(-0.42, 0.42);
                [angle.cos(), angle.sin() * 0.72, z]
            };
            positions[atom_index] = [component_offset + vector[0], vector[1], vector[2]];
        }
    }
    positions
}

fn draw_product_3d(
    frame: &mut canvas::Frame,
    product: &ProductModel,
    center: Point,
    rotation: f32,
    bounds: Size,
    labels: ChemicalLabels,
) {
    let scale = (bounds.width.min(bounds.height) / 410.0).clamp(0.55, 1.2);
    let coordinates = model_coordinates(product);
    let projected = coordinates
        .iter()
        .map(|point| {
            let x = point[0] * rotation.cos() + point[2] * rotation.sin();
            let z = -point[0] * rotation.sin() + point[2] * rotation.cos();
            let y = point[1] * 0.92 - z * 0.16;
            let perspective = 3.2 / (3.2 - z * 0.72);
            (
                center
                    + Vector::new(
                        x * 72.0 * scale * perspective,
                        y * 72.0 * scale * perspective,
                    ),
                z,
                perspective,
            )
        })
        .collect::<Vec<_>>();
    if product.ionic {
        let association_radius = 104.0 * scale;
        frame.stroke(
            &Path::circle(center, association_radius),
            Stroke::default()
                .with_color(chemistry_color::IONIC.scale_alpha(0.34))
                .with_width(1.7),
        );
        frame.stroke(
            &Path::circle(center, association_radius + 7.0 * scale),
            Stroke::default()
                .with_color(chemistry_color::IONIC.scale_alpha(0.12))
                .with_width(1.0),
        );
    }
    for bond in &product.bonds {
        let left = projected[bond.left];
        let right = projected[bond.right];
        let depth = ((left.1 + right.1) * 0.5 + 1.0) * 0.5;
        draw_bond_2d(
            frame,
            left.0,
            right.0,
            bond.order,
            0.52 + depth * 0.38,
            scale * (left.2 + right.2) * 0.5,
        );
    }
    let mut order = (0..product.atoms.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| projected[*left].1.total_cmp(&projected[*right].1));
    for index in order {
        let (point, depth, perspective) = projected[index];
        let alpha = 0.72 + ((depth + 1.0) * 0.5).clamp(0.0, 1.0) * 0.28;
        draw_atom(
            frame,
            &product.atoms[index],
            point,
            21.0 * scale * perspective,
            alpha,
            perspective,
        );
    }
    draw_primary_label(frame, product, center, 102.0 * scale, 0.94, labels);
}

fn draw_bond_2d(
    frame: &mut canvas::Frame,
    left: Point,
    right: Point,
    order: u8,
    alpha: f32,
    scale: f32,
) {
    let direction = right - left;
    let length = (direction.x * direction.x + direction.y * direction.y)
        .sqrt()
        .max(1.0);
    let normal = Vector::new(-direction.y / length, direction.x / length);
    let count = order.clamp(1, 3);
    for lane in 0..count {
        let offset = (f32::from(lane) - (f32::from(count) - 1.0) * 0.5) * 4.0 * scale;
        frame.stroke(
            &Path::line(left + normal * offset, right + normal * offset),
            Stroke::default()
                .with_color(chemistry_color::COVALENT.scale_alpha(alpha * 0.78))
                .with_width(2.2 * scale),
        );
    }
}

fn draw_atom(
    frame: &mut canvas::Frame,
    atom: &VisualAtom,
    center: Point,
    radius: f32,
    alpha: f32,
    depth: f32,
) {
    frame.fill(
        &Path::circle(center + Vector::new(0.0, radius * 0.28), radius * 1.22),
        color::SHADOW.scale_alpha(alpha * 0.32),
    );
    frame.fill(
        &Path::circle(center, radius),
        element_color(&atom.symbol).scale_alpha(alpha),
    );
    frame.fill(
        &Path::circle(
            center + Vector::new(-radius * 0.28, -radius * 0.30),
            radius * 0.26,
        ),
        Color::WHITE.scale_alpha(alpha * 0.28),
    );
    frame.stroke(
        &Path::circle(center, radius),
        Stroke::default()
            .with_color(Color::WHITE.scale_alpha(alpha * 0.24))
            .with_width(depth.clamp(0.8, 1.8)),
    );
    frame.fill_text(canvas::Text {
        content: atom.symbol.clone(),
        position: center,
        color: color::CANVAS.scale_alpha(alpha),
        size: iced::Pixels((radius * 0.74).max(10.0)),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        font: fonts::SEMIBOLD,
        ..canvas::Text::default()
    });
    if atom.formal_charge != 0 {
        frame.fill_text(canvas::Text {
            content: format_charge(i32::from(atom.formal_charge)),
            position: center + Vector::new(radius * 0.78, -radius * 0.82),
            color: color::TEXT,
            size: iced::Pixels((radius * 0.42).max(8.0)),
            font: fonts::SEMIBOLD,
            ..canvas::Text::default()
        });
    }
    let electron_count = atom.non_bonding_electrons.min(8);
    for electron in 0..electron_count {
        let angle = std::f32::consts::TAU * f32::from(electron) / f32::from(electron_count.max(1));
        frame.fill(
            &Path::circle(
                center + Vector::new(angle.cos() * radius * 1.28, angle.sin() * radius * 1.28),
                (radius * 0.075).clamp(1.4, 2.4),
            ),
            chemistry_color::ELECTRON.scale_alpha(alpha * 0.86),
        );
    }
}

fn draw_primary_label(
    frame: &mut canvas::Frame,
    product: &ProductModel,
    center: Point,
    vertical_offset: f32,
    alpha: f32,
    labels: ChemicalLabels,
) {
    frame.fill_text(canvas::Text {
        content: product.primary_label(labels),
        position: center + Vector::new(0.0, vertical_offset),
        color: color::TEXT.scale_alpha(alpha),
        size: iced::Pixels(15.0),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        font: fonts::SEMIBOLD,
        ..canvas::Text::default()
    });
}

fn formula(counts: &BTreeMap<String, usize>) -> String {
    format_ordered_formula(counts, &ordered_elements(counts))
}

fn product_formula(
    frame: &SimulationFrame,
    membership: &BTreeSet<AtomId>,
    counts: &BTreeMap<String, usize>,
    ionic: bool,
) -> String {
    if !ionic {
        return formula(counts);
    }
    let Some(association) = frame.ionic_associations().values().find(|association| {
        association
            .components
            .values()
            .flatten()
            .all(|atom| membership.contains(atom))
    }) else {
        return formula(counts);
    };
    let mut components = association
        .components
        .iter()
        .map(|(group, atoms)| {
            let mut component_counts = BTreeMap::<String, usize>::new();
            for atom in atoms {
                if let Some(atom) = frame.atoms().get(atom) {
                    *component_counts
                        .entry(atom.element.as_str().to_owned())
                        .or_default() += 1;
                }
            }
            (
                association
                    .component_charges
                    .get(group)
                    .copied()
                    .unwrap_or(0),
                component_counts,
            )
        })
        .collect::<Vec<_>>();
    components.sort_by_key(|component| std::cmp::Reverse(component.0));
    components
        .into_iter()
        .map(|(_, component)| conventional_component_formula(&component))
        .collect()
}

fn conventional_component_formula(counts: &BTreeMap<String, usize>) -> String {
    let preferred = if counts.contains_key("C") && counts.contains_key("H") {
        ["H", "C", "N", "O"]
    } else if counts.contains_key("C") {
        ["C", "H", "N", "O"]
    } else if counts.contains_key("N") && counts.contains_key("O") {
        ["N", "H", "C", "O"]
    } else if counts.contains_key("O") && counts.contains_key("H") {
        ["O", "H", "C", "N"]
    } else {
        return formula(counts);
    };
    let mut order = preferred
        .into_iter()
        .filter(|symbol| counts.contains_key(*symbol))
        .collect::<Vec<_>>();
    let remaining = counts
        .keys()
        .map(String::as_str)
        .filter(|symbol| !order.contains(symbol))
        .collect::<Vec<_>>();
    order.extend(remaining);
    format_ordered_formula(counts, &order)
}

fn format_ordered_formula(counts: &BTreeMap<String, usize>, order: &[&str]) -> String {
    order
        .iter()
        .copied()
        .map(|symbol| {
            let count = counts.get(symbol).copied().unwrap_or(1);
            if count == 1 {
                symbol.to_owned()
            } else {
                format!("{symbol}{}", subscript(count))
            }
        })
        .collect()
}

fn composition(counts: &BTreeMap<String, usize>) -> String {
    ordered_elements(counts)
        .into_iter()
        .map(|symbol| format!("{} {symbol}", counts.get(symbol).copied().unwrap_or(0)))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

fn ordered_elements(counts: &BTreeMap<String, usize>) -> Vec<&str> {
    let mut symbols = counts.keys().map(String::as_str).collect::<Vec<_>>();
    if counts.contains_key("C") {
        symbols.sort_by_key(|symbol| match *symbol {
            "C" => (0, ""),
            "H" => (1, ""),
            other => (2, other),
        });
    }
    symbols
}

fn subscript(value: usize) -> String {
    value
        .to_string()
        .chars()
        .map(|digit| match digit {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            _ => digit,
        })
        .collect()
}

fn molar_mass(counts: &BTreeMap<String, usize>) -> String {
    let mut total = 0_u64;
    let mut approximate = false;
    for (symbol, count) in counts {
        let Some(element) = crate::elements::SUPPORTED
            .iter()
            .find(|element| element.symbol == symbol)
        else {
            return "Not available".to_owned();
        };
        approximate |= element.atomic_mass.starts_with('[');
        let Some(mass) = decimal_millionths(element.atomic_mass.trim_matches(['[', ']'])) else {
            return "Not available".to_owned();
        };
        total =
            total.saturating_add(mass.saturating_mul(u64::try_from(*count).unwrap_or(u64::MAX)));
    }
    let rounded_thousandths = total.saturating_add(500) / 1_000;
    let mut value = format!(
        "{}.{:03}",
        rounded_thousandths / 1_000,
        rounded_thousandths % 1_000
    );
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    format!("{}{value} g mol⁻¹", if approximate { "≈ " } else { "" })
}

fn decimal_millionths(value: &str) -> Option<u64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole = whole.parse::<u64>().ok()?;
    if fraction.len() > 6 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = format!("{fraction:0<6}").parse::<u64>().ok()?;
    Some(whole.saturating_mul(1_000_000).saturating_add(fraction))
}

fn format_charge(charge: i32) -> String {
    match charge {
        0 => "0 (neutral)".to_owned(),
        1 => "+1".to_owned(),
        -1 => "−1".to_owned(),
        value if value > 0 => format!("+{value}"),
        value => value.to_string().replace('-', "−"),
    }
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

fn element_color(symbol: &str) -> Color {
    match symbol {
        "H" => LAB_DARK.chemistry.hydrogen,
        "Li" => LAB_DARK.chemistry.lithium,
        "Ag" => LAB_DARK.chemistry.silver,
        "C" => LAB_DARK.chemistry.carbon,
        "N" => LAB_DARK.chemistry.nitrogen,
        "O" => LAB_DARK.chemistry.oxygen,
        "Na" => LAB_DARK.chemistry.sodium,
        "Cl" => LAB_DARK.chemistry.chlorine,
        _ => LAB_DARK.chemistry.element_default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_uses_hill_order_and_unicode_subscripts() {
        let counts = BTreeMap::from([
            ("O".to_owned(), 1),
            ("H".to_owned(), 4),
            ("C".to_owned(), 2),
        ]);
        assert_eq!(formula(&counts), "C₂H₄O");
    }

    #[test]
    fn molar_mass_is_composed_from_exact_decimal_element_metadata() {
        let water = BTreeMap::from([("H".to_owned(), 2), ("O".to_owned(), 1)]);
        assert_eq!(molar_mass(&water), "18.015 g mol⁻¹");
    }

    #[test]
    fn model_coordinates_are_deterministic_and_three_dimensional() {
        let product = ProductModel {
            name: "test".to_owned(),
            formula: "X₄".to_owned(),
            coefficient: 1,
            classification: "Covalent molecule",
            composition: "4 X".to_owned(),
            atom_count: 4,
            bond_count: 3,
            net_charge: 0,
            molar_mass: "Not available".to_owned(),
            atoms: (0..4)
                .map(|_| VisualAtom {
                    symbol: "X".to_owned(),
                    formal_charge: 0,
                    non_bonding_electrons: 0,
                })
                .collect(),
            bonds: vec![
                VisualBond {
                    left: 0,
                    right: 1,
                    order: 1,
                },
                VisualBond {
                    left: 0,
                    right: 2,
                    order: 1,
                },
                VisualBond {
                    left: 0,
                    right: 3,
                    order: 1,
                },
            ],
            ionic: false,
        };
        assert_eq!(model_coordinates(&product), model_coordinates(&product));
        assert!(
            model_coordinates(&product)
                .iter()
                .any(|position| position[2].abs() > 0.01)
        );
    }

    #[test]
    fn product_geometry_preserves_validated_bonds_and_separates_ionic_components() {
        let product = ProductModel {
            name: "lithium hydroxide".to_owned(),
            formula: "LiOH".to_owned(),
            coefficient: 1,
            classification: "Ionic assembly",
            composition: "1 H · 1 Li · 1 O".to_owned(),
            atom_count: 3,
            bond_count: 1,
            net_charge: 0,
            molar_mass: "23.948 g mol⁻¹".to_owned(),
            atoms: vec![
                VisualAtom {
                    symbol: "Li".to_owned(),
                    formal_charge: 1,
                    non_bonding_electrons: 0,
                },
                VisualAtom {
                    symbol: "O".to_owned(),
                    formal_charge: -1,
                    non_bonding_electrons: 6,
                },
                VisualAtom {
                    symbol: "H".to_owned(),
                    formal_charge: 0,
                    non_bonding_electrons: 0,
                },
            ],
            bonds: vec![VisualBond {
                left: 1,
                right: 2,
                order: 1,
            }],
            ionic: true,
        };
        let positions = model_coordinates(&product);
        let distance_squared = |left: [f32; 3], right: [f32; 3]| {
            left.into_iter()
                .zip(right)
                .map(|(left, right)| (left - right).powi(2))
                .sum::<f32>()
        };
        assert!(distance_squared(positions[0], positions[1]) > 0.1);
        assert!(distance_squared(positions[0], positions[2]) > 0.1);
        assert!(distance_squared(positions[1], positions[2]) > 0.1);
        assert_eq!(product.bonds.len(), 1);
        assert_eq!((product.bonds[0].left, product.bonds[0].right), (1, 2));
    }

    #[test]
    fn lone_pair_geometry_produces_a_bent_two_bond_molecule() {
        let product = ProductModel {
            name: "water".to_owned(),
            formula: "H₂O".to_owned(),
            coefficient: 1,
            classification: "Covalent molecule",
            composition: "2 H · 1 O".to_owned(),
            atom_count: 3,
            bond_count: 2,
            net_charge: 0,
            molar_mass: "18.015 g mol⁻¹".to_owned(),
            atoms: vec![
                VisualAtom {
                    symbol: "O".to_owned(),
                    formal_charge: 0,
                    non_bonding_electrons: 4,
                },
                VisualAtom {
                    symbol: "H".to_owned(),
                    formal_charge: 0,
                    non_bonding_electrons: 0,
                },
                VisualAtom {
                    symbol: "H".to_owned(),
                    formal_charge: 0,
                    non_bonding_electrons: 0,
                },
            ],
            bonds: vec![
                VisualBond {
                    left: 0,
                    right: 1,
                    order: 1,
                },
                VisualBond {
                    left: 0,
                    right: 2,
                    order: 1,
                },
            ],
            ionic: false,
        };
        let positions = model_coordinates(&product);
        let left = positions[1];
        let right = positions[2];
        let cosine = left
            .iter()
            .zip(right)
            .map(|(left, right)| left * right)
            .sum::<f32>();
        let angle = cosine.clamp(-1.0, 1.0).acos().to_degrees();
        assert!((angle - 104.5).abs() < 0.01);
    }

    #[test]
    fn summary_products_are_compiled_from_the_validated_final_frame() {
        let run = crate::chemistry::run(crate::chemistry::ReactionRequest::DEFAULT)
            .expect("default .chems request validates");
        let summary = SummaryData::from_frames(run.frames()).expect("products are assigned");
        let formulae = summary
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<BTreeSet<_>>();

        assert!(formulae.contains("H₂"));
        assert!(formulae.contains("LiOH"));
        assert!(
            summary
                .products
                .iter()
                .all(|product| product.atom_count > 0)
        );
    }
}
