use chem_catalogue::ReferenceCatalogue;

use crate::{
    AgentError, EscalatedMechanismOutcome, FamilyMatchOutcome, MechanismEscalationOutcome,
    MechanismProvider, ReviewedAnimationOutcome, ValidatedStaticOutcome,
    compile_reviewed_animation, derive_mechanism, match_reviewed_family,
};

/// Presentation capability established after a static outcome is already
/// available. Each animated variant carries validator-produced frames; the
/// static variant deliberately exposes no playback surface.
#[derive(Debug, Clone)]
pub enum DynamicPresentationOutcome {
    ReviewedFamily(Box<ReviewedAnimationOutcome>),
    Escalated(Box<EscalatedMechanismOutcome>),
    Static {
        outcome: Box<ValidatedStaticOutcome>,
        diagnostic: String,
        retryable: bool,
        attempts: usize,
    },
}

impl DynamicPresentationOutcome {
    #[must_use]
    pub const fn static_outcome(&self) -> &ValidatedStaticOutcome {
        match self {
            Self::ReviewedFamily(outcome) => outcome.static_outcome(),
            Self::Escalated(outcome) => outcome.static_outcome(),
            Self::Static { outcome, .. } => outcome,
        }
    }
}

/// Applies the local reviewed-family capability before considering model
/// escalation. Provider output can never select or override a family match.
///
/// # Errors
///
/// Returns an error only when a locally matched reviewed family cannot be
/// expanded or validated. Escalation failures settle into the static variant.
pub fn enrich_static_outcome<P: MechanismProvider>(
    outcome: ValidatedStaticOutcome,
    catalogue: &ReferenceCatalogue,
    provider: &mut P,
) -> Result<DynamicPresentationOutcome, AgentError> {
    match match_reviewed_family(&outcome, catalogue)? {
        FamilyMatchOutcome::Matched(family) => Ok(DynamicPresentationOutcome::ReviewedFamily(
            Box::new(compile_reviewed_animation(outcome, *family, catalogue)?),
        )),
        FamilyMatchOutcome::Ambiguous(rule_ids) => Ok(DynamicPresentationOutcome::Static {
            outcome: Box::new(outcome),
            diagnostic: format!(
                "multiple reviewed families remain applicable: {}",
                rule_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            retryable: false,
            attempts: 0,
        }),
        FamilyMatchOutcome::NoMatch => match derive_mechanism(outcome, catalogue, provider) {
            MechanismEscalationOutcome::Animated(outcome) => {
                Ok(DynamicPresentationOutcome::Escalated(outcome))
            }
            MechanismEscalationOutcome::Failed(error) => Err(error),
            MechanismEscalationOutcome::Unavailable {
                static_outcome,
                attempts,
                diagnostic,
                retryable,
            } => Ok(DynamicPresentationOutcome::Static {
                outcome: static_outcome,
                attempts,
                diagnostic,
                retryable,
            }),
        },
    }
}
