//! Systematic organic names for the classroom subset, parsed into subset
//! SMILES: straight-chain roots C1-C8 with alkyl and halo substituents,
//! one site of unsaturation (-ene/-yne), hydroxyls (-ol/-diol/-triol),
//! two-word alkyl alkanoate esters ("propyl ethanoate"), or a
//! carboxylic acid (-oic acid). Everything outside the subset returns
//! None — a wrong molecule is worse than none. Chemical validity is not
//! judged here; the SMILES parser and valence rules fail closed on
//! impossible substitution downstream.

/// Chain roots by carbon count (index + 1 carbons).
const ROOTS: [&str; 8] = [
    "meth", "eth", "prop", "but", "pent", "hex", "hept", "oct",
];

const MULTIPLIERS: [(&str, usize); 3] = [("di", 2), ("tri", 3), ("tetra", 4)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Substituent {
    Methyl,
    Ethyl,
    Propyl,
    Halo(&'static str),
}

const SUBSTITUENTS: [(&str, Substituent); 7] = [
    ("methyl", Substituent::Methyl),
    ("ethyl", Substituent::Ethyl),
    ("propyl", Substituent::Propyl),
    ("fluoro", Substituent::Halo("F")),
    ("chloro", Substituent::Halo("Cl")),
    ("bromo", Substituent::Halo("Br")),
    ("iodo", Substituent::Halo("I")),
];

#[derive(Debug, Default)]
struct ChainSpec {
    length: usize,
    /// (position, bond order) of the one allowed unsaturation.
    unsaturation: Option<(usize, u8)>,
    hydroxyls: Vec<usize>,
    acid: bool,
    substituents: Vec<(usize, Substituent)>,
    /// Whether the suffix carried its own locant ("but-2-ene"); a bare
    /// leading locant ("2-butene") may only fill an implicit one.
    suffix_locant_explicit: bool,
    /// cis-/trans- prefix: Some(true) = cis. Only meaningful with a
    /// 1,2-disubstituted double bond inside the chain.
    stereo_cis: Option<bool>,
    /// Two-word ester sentinel: (alkyl carbons, acyl carbons incl. the
    /// carbonyl). When set, every other field is ignored.
    ester: Option<(usize, usize)>,
}

/// Subset SMILES for a systematic name ("2-methylbutane", "but-2-ene",
/// "(R)-butan-2-ol", "ethanoic acid", "1,2-dibromoethane"), or None
/// outside the subset.
#[must_use]
pub fn smiles_for_name(name: &str) -> Option<String> {
    let normalized = name
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let (normalized, descriptor) = if let Some(rest) = normalized.strip_prefix("(r)-") {
        (rest.to_owned(), Some(crate::cip::StereoDescriptor::R))
    } else if let Some(rest) = normalized.strip_prefix("(s)-") {
        (rest.to_owned(), Some(crate::cip::StereoDescriptor::S))
    } else {
        (normalized, None)
    };
    let spec = parse(&normalized)?;
    match descriptor {
        None => Some(spec.to_smiles()),
        // Build both handedness variants and keep the one whose CIP
        // descriptor matches; a name that is not a real stereocentre
        // ((R)-propan-2-ol) matches neither and fails closed.
        Some(wanted) => ["@", "@@"].iter().find_map(|glyph| {
            let candidate = spec.to_chiral_smiles(glyph)?;
            let structure = crate::smiles::structure_from_smiles(
                crate::identity::StructureId::new("iupac.chirality").ok()?,
                &candidate,
            )?;
            let centre = structure
                .graph()
                .atoms()
                .values()
                .find(|atom| atom.chirality().is_some())?
                .id()
                .clone();
            (crate::cip::stereocentre_descriptor(&structure, &centre) == Some(wanted))
                .then_some(candidate)
        }),
    }
}

fn parse(name: &str) -> Option<ChainSpec> {
    let (name, stereo_cis) = if let Some(rest) = name.strip_prefix("cis-") {
        (rest, Some(true))
    } else if let Some(rest) = name.strip_prefix("trans-") {
        (rest, Some(false))
    } else {
        (name, None)
    };
    let (body, acid) = match name.strip_suffix("oic acid") {
        Some(body) => (body.to_owned(), true),
        None => (name.to_owned(), false),
    };
    if let Some((alkyl, acyl)) = body.split_once(' ') {
        return ester_spec(alkyl, acyl);
    }
    let mut spec = ChainSpec {
        acid,
        stereo_cis,
        ..ChainSpec::default()
    };
    // Suffix and root, tried longest-suffix-first on the tail segment.
    let tail = body;
    let rest = if acid {
        strip_root_suffix(&tail, "an", &mut spec)?
    } else if let Some(rest) = try_polyol(&tail, &mut spec) {
        rest
    } else if let Some(rest) = try_suffix(&tail, "ol", &mut spec) {
        rest
    } else if let Some(rest) = try_unsaturated(&tail, "ene", 2, &mut spec) {
        rest
    } else if let Some(rest) = try_unsaturated(&tail, "yne", 3, &mut spec) {
        rest
    } else {
        let rest = tail.strip_suffix("ane")?;
        strip_root(rest, &mut spec)?
    };
    // Front-locant suffix form ("2-butene", "2-propanol"): a bare leading
    // locant belongs to a suffix that has not already claimed one.
    let trimmed = rest.trim_matches('-');
    if !trimmed.is_empty()
        && trimmed.bytes().all(|byte| byte.is_ascii_digit())
        && !spec.suffix_locant_explicit
    {
        let locant = trimmed.parse().ok()?;
        if let Some((_, order)) = spec.unsaturation {
            spec.unsaturation = Some((locant, order));
        } else if spec.hydroxyls.len() == 1 {
            spec.hydroxyls = vec![locant];
        } else {
            return None;
        }
    } else {
        parse_substituents(&rest, &mut spec)?;
    }
    validate(&spec).then_some(spec)
}

/// Two-word esters: `<alkyl>yl <root>anoate` ("propyl ethanoate"). The
/// returned spec is a sentinel: `to_smiles` special-cases it.
fn ester_spec(alkyl: &str, acyl: &str) -> Option<ChainSpec> {
    let alkyl_root = alkyl.strip_suffix("yl")?;
    let mut alkyl_spec = ChainSpec::default();
    strip_root(alkyl_root, &mut alkyl_spec)?
        .is_empty()
        .then_some(())?;
    let acyl_root = acyl.strip_suffix("anoate")?;
    let mut acyl_spec = ChainSpec::default();
    strip_root(acyl_root, &mut acyl_spec)?
        .is_empty()
        .then_some(())?;
    Some(ChainSpec {
        ester: Some((alkyl_spec.length, acyl_spec.length)),
        ..ChainSpec::default()
    })
}

/// `...an[-N-]ol` and `N-...anol`: hydroxyl with an optional locant.
fn try_suffix(tail: &str, suffix: &str, spec: &mut ChainSpec) -> Option<String> {
    let rest = tail.strip_suffix(suffix)?;
    let (rest, locant) = strip_trailing_locant(rest);
    let rest = strip_root_suffix(&rest, "an", spec)?;
    spec.suffix_locant_explicit = locant.is_some();
    spec.hydroxyls = vec![locant.unwrap_or(1)];
    Some(rest)
}

/// `...ane-N,N[,N]-diol|triol`: multiple hydroxyls with a locant list.
fn try_polyol(tail: &str, spec: &mut ChainSpec) -> Option<String> {
    let (rest, count) = if let Some(rest) = tail.strip_suffix("diol") {
        (rest, 2)
    } else if let Some(rest) = tail.strip_suffix("triol") {
        (rest, 3)
    } else {
        return None;
    };
    let rest = rest.strip_suffix('-')?;
    let split = rest
        .bytes()
        .rev()
        .take_while(|byte| byte.is_ascii_digit() || *byte == b',')
        .count();
    if split == 0 {
        return None;
    }
    let (head, locants) = rest.split_at(rest.len() - split);
    let locants: Vec<usize> = locants
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if locants.len() != count {
        return None;
    }
    let head = head.strip_suffix('-')?;
    let rest = strip_root_suffix(head, "ane", spec)?;
    spec.suffix_locant_explicit = true;
    spec.hydroxyls = locants;
    Some(rest)
}

/// `...[-N-]ene` / `N-...ene`: one double or triple bond.
fn try_unsaturated(tail: &str, suffix: &str, order: u8, spec: &mut ChainSpec) -> Option<String> {
    let rest = tail.strip_suffix(suffix)?;
    let (rest, locant) = strip_trailing_locant(rest);
    let rest = strip_root(&rest, spec)?;
    spec.suffix_locant_explicit = locant.is_some();
    spec.unsaturation = Some((locant.unwrap_or(1), order));
    Some(rest)
}

/// Strips `-N-` or a bare trailing locant digit block from the end.
fn strip_trailing_locant(text: &str) -> (String, Option<usize>) {
    let trimmed = text.strip_suffix('-').unwrap_or(text);
    let digits = trimmed
        .bytes()
        .rev()
        .take_while(u8::is_ascii_digit)
        .count();
    if digits == 0 {
        return (trimmed.to_owned(), None);
    }
    let split = trimmed.len() - digits;
    let locant = trimmed[split..].parse().ok();
    let mut head = trimmed[..split].to_owned();
    if head.ends_with('-') {
        head.pop();
    }
    (head, locant)
}

/// Strips `<root><glue>` from the end and records the chain length.
fn strip_root_suffix(text: &str, glue: &str, spec: &mut ChainSpec) -> Option<String> {
    let rest = text.strip_suffix(glue)?;
    strip_root(rest, spec)
}

fn strip_root(text: &str, spec: &mut ChainSpec) -> Option<String> {
    // Longest root first so "prop" is not read as a truncated "pent" etc.
    let mut roots: Vec<(usize, &str)> = ROOTS
        .iter()
        .enumerate()
        .map(|(index, root)| (index + 1, *root))
        .collect();
    roots.sort_by_key(|(_, root)| std::cmp::Reverse(root.len()));
    for (length, root) in roots {
        if let Some(rest) = text.strip_suffix(root) {
            spec.length = length;
            return Some(rest.to_owned());
        }
    }
    None
}

/// Parses the leading substituent prefixes: repeated
/// `[N[,N...]-][di|tri|tetra]<group>[-]`.
fn parse_substituents(prefixes: &str, spec: &mut ChainSpec) -> Option<()> {
    let mut rest = prefixes.trim_matches('-');
    while !rest.is_empty() {
        // Locants.
        let mut locants = Vec::new();
        let digits_and_commas = rest
            .bytes()
            .take_while(|byte| byte.is_ascii_digit() || *byte == b',')
            .count();
        if digits_and_commas > 0 {
            for piece in rest[..digits_and_commas].split(',') {
                locants.push(piece.parse::<usize>().ok()?);
            }
            rest = rest[digits_and_commas..].strip_prefix('-')?;
        }
        // Multiplier.
        let mut count = 1;
        for (multiplier, value) in MULTIPLIERS {
            if let Some(after) = rest.strip_prefix(multiplier) {
                // "di" must not eat the start of an unknown group name; only
                // accept when a known group follows.
                if SUBSTITUENTS
                    .iter()
                    .any(|(group, _)| after.starts_with(group))
                {
                    count = value;
                    rest = after;
                    break;
                }
            }
        }
        // Group.
        let (group_name, group) = SUBSTITUENTS
            .iter()
            .find(|(group, _)| rest.starts_with(group))?;
        rest = rest[group_name.len()..].trim_start_matches('-');
        if locants.is_empty() {
            // Locants may only be omitted when the position is forced.
            if count == 1 && spec.length <= 2 {
                locants.push(1);
            } else {
                return None;
            }
        }
        if locants.len() != count {
            return None;
        }
        for locant in locants {
            spec.substituents.push((locant, *group));
        }
    }
    Some(())
}

fn validate(spec: &ChainSpec) -> bool {
    if spec.ester.is_some() {
        return true;
    }
    if spec.length == 0 {
        return false;
    }
    let in_chain = |position: usize| (1..=spec.length).contains(&position);
    let stereo_valid = match (spec.stereo_cis, spec.unsaturation) {
        (None, _) => true,
        // cis/trans needs a double bond with chain carbons on both sides
        // and no substituents at either stereo carbon: anything more
        // needs E/Z priorities, which stay out of the subset.
        (Some(_), Some((position, 2))) => {
            position >= 2
                && position + 2 <= spec.length
                && spec
                    .substituents
                    .iter()
                    .all(|(locant, _)| *locant != position && *locant != position + 1)
                && spec.hydroxyls.is_empty()
                && !spec.acid
        }
        (Some(_), _) => false,
    };
    stereo_valid
        && spec
            .unsaturation
            .is_none_or(|(position, _)| in_chain(position) && position < spec.length)
        && spec.hydroxyls.iter().all(|position| in_chain(*position))
        && spec
            .substituents
            .iter()
            .all(|(position, _)| in_chain(*position))
}

impl ChainSpec {
    /// The chain position that could be a stereocentre: an interior
    /// carbon bearing the hydroxyl or a lone halo substituent.
    fn stereocentre_position(&self) -> Option<usize> {
        let single_hydroxyl = match self.hydroxyls.as_slice() {
            [] => None,
            [only] => Some(*only),
            _ => return None,
        };
        let position = single_hydroxyl.or_else(|| {
            let halos: Vec<usize> = self
                .substituents
                .iter()
                .filter(|(_, group)| matches!(group, Substituent::Halo(_)))
                .map(|(locant, _)| *locant)
                .collect();
            match halos.as_slice() {
                [single] => Some(*single),
                _ => None,
            }
        })?;
        (position > 1 && position < self.length).then_some(position)
    }

    /// The spec's SMILES with a chirality glyph on its stereocentre, or
    /// None when no position qualifies.
    fn to_chiral_smiles(&self, glyph: &str) -> Option<String> {
        let position = self.stereocentre_position()?;
        let plain = self.to_smiles();
        // The stereocentre is the position-th chain carbon: count bare
        // chain carbons in the emitted string (branch carbons are always
        // inside parentheses, so chain carbons are those at depth zero).
        let mut depth = 0_usize;
        let mut seen = 0_usize;
        for (index, character) in plain.char_indices() {
            match character {
                '(' => depth += 1,
                ')' => depth -= 1,
                'C' if depth == 0 => {
                    seen += 1;
                    if seen == position {
                        let mut output = plain[..index].to_owned();
                        output.push_str("[C");
                        output.push_str(glyph);
                        output.push_str("H]");
                        output.push_str(&plain[index + 1..]);
                        return Some(output);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn to_smiles(&self) -> String {
        if let Some((alkyl, acyl)) = self.ester {
            // R-O-C(=O)-R': alkyl chain, bridging oxygen, carbonyl, rest
            // of the acyl chain.
            let mut output = "C".repeat(alkyl);
            output.push_str("OC(=O)");
            output.push_str(&"C".repeat(acyl - 1));
            return output;
        }
        let mut output = String::new();
        for position in 1..=self.length {
            output.push('C');
            if position == 1 && self.acid {
                output.push_str("(=O)(O)");
            }
            for _ in self.hydroxyls.iter().filter(|locant| **locant == position) {
                output.push_str("(O)");
            }
            for (locant, group) in &self.substituents {
                if *locant != position {
                    continue;
                }
                match group {
                    Substituent::Methyl => output.push_str("(C)"),
                    Substituent::Ethyl => output.push_str("(CC)"),
                    Substituent::Propyl => output.push_str("(CCC)"),
                    Substituent::Halo(symbol) => {
                        output.push('(');
                        output.push_str(symbol);
                        output.push(')');
                    }
                }
            }
            if position < self.length {
                match self.unsaturation {
                    Some((start, 2)) if start == position => output.push('='),
                    Some((start, 3)) if start == position => output.push('#'),
                    _ => {}
                }
                // Direction marks around a cis/trans double bond at k:
                // C.../C=C/C is trans, C.../C=C\C cis.
                if let (Some(cis), Some((start, 2))) = (self.stereo_cis, self.unsaturation) {
                    if position + 1 == start {
                        output.push('/');
                    } else if position == start + 1 {
                        output.push(if cis { '\\' } else { '/' });
                    }
                }
            }
        }
        output
    }
}
