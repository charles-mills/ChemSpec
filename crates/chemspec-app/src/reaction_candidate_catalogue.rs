//! Curated Stage 4 reaction-request candidates.
//!
//! Matching one of these patterns only enables the builder's trigger. It is
//! not a validation result and cannot create products or a simulation frame.

use std::collections::BTreeMap;

use crate::composition_catalogue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Participant {
    Atom(u8),
    Composition(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactionCandidate {
    pub id: &'static str,
    pub name: &'static str,
    pub equation_preview: &'static str,
    pub visual_reactants: &'static [Participant],
    pub visual_products: &'static [Participant],
    pub stages: &'static [StoryboardStage],
    participants: &'static [Participant],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryboardStage {
    pub title: &'static str,
    pub explanation: &'static str,
}

const HYDROGEN_OXYGEN_STAGES: &[StoryboardStage] = &[
    StoryboardStage {
        title: "Reactants",
        explanation: "Two hydrogen molecules and one oxygen molecule preserve the balanced ratio.",
    },
    StoryboardStage {
        title: "Approach",
        explanation: "The representative particles move together before their electron sharing changes.",
    },
    StoryboardStage {
        title: "Rearrangement",
        explanation: "The original shared pairs fade as new oxygen–hydrogen pairs form.",
    },
    StoryboardStage {
        title: "Products",
        explanation: "Two grouped water models emerge, matching 2H₂ + O₂ → 2H₂O.",
    },
];

const LITHIUM_WATER_STAGES: &[StoryboardStage] = &[
    StoryboardStage {
        title: "Reactants",
        explanation: "Lithium and water are shown in the balanced 2:2 representative ratio.",
    },
    StoryboardStage {
        title: "Approach",
        explanation: "The reactant models move into the interaction region.",
    },
    StoryboardStage {
        title: "Rearrangement",
        explanation: "Atoms regroup while the hydrogen pair becomes a separate product.",
    },
    StoryboardStage {
        title: "Products",
        explanation: "Lithium hydroxide and hydrogen both remain visible as distinct products.",
    },
];

const CARBON_OXYGEN_STAGES: &[StoryboardStage] = &[
    StoryboardStage {
        title: "Reactants",
        explanation: "One carbon atom and one oxygen molecule enter the representative view.",
    },
    StoryboardStage {
        title: "Approach",
        explanation: "The reactants converge before the shared electron arrangement changes.",
    },
    StoryboardStage {
        title: "Rearrangement",
        explanation: "Two shared-pair regions form between carbon and oxygen atoms.",
    },
    StoryboardStage {
        title: "Products",
        explanation: "The final grouped atomic model represents carbon dioxide.",
    },
];

pub const SUPPORTED: &[ReactionCandidate] = &[
    ReactionCandidate {
        id: "hydrogen-oxygen",
        name: "Hydrogen and oxygen",
        equation_preview: "2H₂ + O₂  →  2H₂O",
        visual_reactants: &[
            Participant::Composition("H₂"),
            Participant::Composition("H₂"),
            Participant::Composition("O₂"),
        ],
        visual_products: &[
            Participant::Composition("H₂O"),
            Participant::Composition("H₂O"),
        ],
        stages: HYDROGEN_OXYGEN_STAGES,
        participants: &[
            Participant::Composition("H₂"),
            Participant::Composition("O₂"),
        ],
    },
    ReactionCandidate {
        id: "lithium-water",
        name: "Lithium and water",
        equation_preview: "2Li + 2H₂O  →  2LiOH + H₂",
        visual_reactants: &[
            Participant::Atom(3),
            Participant::Atom(3),
            Participant::Composition("H₂O"),
            Participant::Composition("H₂O"),
        ],
        visual_products: &[
            Participant::Composition("LiOH"),
            Participant::Composition("LiOH"),
            Participant::Composition("H₂"),
        ],
        stages: LITHIUM_WATER_STAGES,
        participants: &[Participant::Atom(3), Participant::Composition("H₂O")],
    },
    ReactionCandidate {
        id: "carbon-oxygen",
        name: "Carbon and oxygen",
        equation_preview: "C + O₂  →  CO₂",
        visual_reactants: &[Participant::Atom(6), Participant::Composition("O₂")],
        visual_products: &[Participant::Composition("CO₂")],
        stages: CARBON_OXYGEN_STAGES,
        participants: &[Participant::Atom(6), Participant::Composition("O₂")],
    },
];

pub fn recognize(participants: impl IntoIterator<Item = Participant>) -> Option<ReactionCandidate> {
    let actual = counts(participants);
    SUPPORTED
        .iter()
        .copied()
        .find(|candidate| counts(candidate.participants.iter().copied()) == actual)
}

/// Matches the two Stage 1 drafts without promoting them beyond user intent.
/// A recognised composition contributes its preview formula; a single loose
/// atom remains an atom. Every other draft shape is unsupported.
pub fn recognize_drafts(first: &[u8], second: &[u8]) -> Option<ReactionCandidate> {
    let participant = |atoms: &[u8]| {
        composition_catalogue::recognize(atoms.iter().copied())
            .map(|preview| Participant::Composition(preview.formula))
            .or_else(|| (atoms.len() == 1).then(|| Participant::Atom(atoms[0])))
    };

    recognize([participant(first)?, participant(second)?])
}

fn counts(participants: impl IntoIterator<Item = Participant>) -> BTreeMap<Participant, usize> {
    participants
        .into_iter()
        .fold(BTreeMap::new(), |mut counts, participant| {
            *counts.entry(participant).or_default() += 1;
            counts
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_is_order_independent_and_exact() {
        let hydrogen_oxygen = recognize([
            Participant::Composition("O₂"),
            Participant::Composition("H₂"),
        ])
        .expect("supported candidate");
        assert_eq!(hydrogen_oxygen.id, "hydrogen-oxygen");
        assert!(recognize([Participant::Composition("H₂")]).is_none());
        assert!(
            recognize([
                Participant::Composition("H₂"),
                Participant::Composition("O₂"),
                Participant::Atom(1),
            ])
            .is_none()
        );
    }

    #[test]
    fn multi_product_candidate_is_structured_data() {
        let lithium_water = recognize([Participant::Atom(3), Participant::Composition("H₂O")])
            .expect("supported candidate");
        assert_eq!(lithium_water.id, "lithium-water");
        assert!(lithium_water.equation_preview.contains("LiOH + H₂"));
    }

    #[test]
    fn stage_one_drafts_match_only_supported_reaction_candidates() {
        assert_eq!(
            recognize_drafts(&[6], &[8, 8]).map(|candidate| candidate.id),
            Some("carbon-oxygen")
        );
        assert!(recognize_drafts(&[6], &[8]).is_none());
        assert!(recognize_drafts(&[6, 6], &[8, 8]).is_none());
    }
}
