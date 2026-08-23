//! Trust tiers for emitted claims. Trust is derived from evidence -- whether a
//! verification run actually passed -- never from whether a human nodded along.
//!
//! The `approaches` table carries `spec_id` but no link to an individual
//! acceptance criterion, so trust can only be established at spec granularity.
//! The variant is therefore named `SpecVerified`, not `Verified`, so no call
//! site can accidentally imply that one specific decision was proved.

use crate::emit::model::VerificationRow;

/// How much evidence backs a claim in an emitted document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// A verification run for the owning spec passed. This does not prove any
    /// individual decision within that spec.
    SpecVerified,
    /// No passing verification run exists for the owning spec.
    Unverified,
}

/// Trust tier rendering.
impl Trust {
    /// The exact wording written into emitted documents for this tier. The
    /// spec-verified wording deliberately states what it does not prove.
    pub fn label(self) -> &'static str {
        match self {
            Trust::SpecVerified => {
                "spec verified -- a verification run for this spec passed; \
                 this individual decision was not separately proved"
            }
            Trust::Unverified => "not independently verified",
        }
    }
}

/// Derive a spec's trust tier from its verification rows. One passing run is
/// enough, because a passing run is evidence the spec's criteria were exercised.
pub fn derive_trust(verifications: &[VerificationRow]) -> Trust {
    if verifications.iter().any(|v| v.success) {
        Trust::SpecVerified
    } else {
        Trust::Unverified
    }
}

#[cfg(test)]
/// Tests for deriving a trust tier from verification rows.
mod tests {
    use super::*;
    use crate::emit::model::VerificationRow;

    /// Build a verification row with the given outcome.
    fn row(success: bool) -> VerificationRow {
        VerificationRow {
            command: "cargo test".into(),
            success,
            criteria_index: None,
        }
    }

    /// No verification rows at all means nothing has been proved.
    #[test]
    fn no_verifications_is_unverified() {
        assert_eq!(derive_trust(&[]), Trust::Unverified);
    }

    /// Verification rows that all failed do not confer trust.
    #[test]
    fn only_failing_verifications_is_unverified() {
        assert_eq!(derive_trust(&[row(false), row(false)]), Trust::Unverified);
    }

    /// At least one passing run establishes spec-level trust.
    #[test]
    fn any_passing_verification_is_spec_verified() {
        assert_eq!(derive_trust(&[row(false), row(true)]), Trust::SpecVerified);
    }

    /// The spec-verified label must state the limit of the claim, so a reader is
    /// never told an individual decision was proved when it was not. Both halves
    /// of the caveat are pinned: an edit that keeps "not separately proved" while
    /// dropping "individual decision" would still be a softening, and that is
    /// precisely the regression this test exists to catch.
    #[test]
    fn spec_verified_label_states_its_limit() {
        let label = Trust::SpecVerified.label();
        assert!(label.contains("individual decision"));
        assert!(label.contains("not separately proved"));
    }
}
