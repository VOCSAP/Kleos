use serde::{Deserialize, Serialize};

/// Frozen fidelity question for the upstream judge.
pub const FIDELITY_PROMPT: &str = include_str!("../prompts/upstream_judge_fidelity.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    Fidele,
    Infidele,
    Inverifiable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Distinct,
    Doublon,
    Contredit,
    Prolonge,
    MetAJour,
    Partiel,
}

impl Relation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Distinct => "distinct",
            Self::Doublon => "doublon",
            Self::Contredit => "contredit",
            Self::Prolonge => "prolonge",
            Self::MetAJour => "met_a_jour",
            Self::Partiel => "partiel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Verdict {
    pub fidelity: Fidelity,
    pub relation: Relation,
    pub neighbour_id: Option<i64>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    Unfaithful,
    Unverifiable,
    Duplicate,
    ConflictWithoutNeighbour,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Publish { relation_tag: String },
    Drop { reason: DropReason },
    HoldConflict { neighbour_id: i64 },
}

/// Fidelity outranks relation. A contradiction without a neighbour id cannot
/// be re-evaluated later, so it is dropped rather than held.
pub fn effect(v: &Verdict) -> Effect {
    match v.fidelity {
        Fidelity::Infidele => {
            return Effect::Drop {
                reason: DropReason::Unfaithful,
            }
        }
        Fidelity::Inverifiable => {
            return Effect::Drop {
                reason: DropReason::Unverifiable,
            }
        }
        Fidelity::Fidele => {}
    }
    match (v.relation, v.neighbour_id) {
        (Relation::Doublon, _) => Effect::Drop {
            reason: DropReason::Duplicate,
        },
        (Relation::Contredit, Some(neighbour_id)) => Effect::HoldConflict { neighbour_id },
        (Relation::Contredit, None) => Effect::Drop {
            reason: DropReason::ConflictWithoutNeighbour,
        },
        (relation, _) => Effect::Publish {
            relation_tag: format!("relation:{}", relation.as_str()),
        },
    }
}
