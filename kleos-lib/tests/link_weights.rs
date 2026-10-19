//! A memory reached through a contradiction must not outrank a neighbour
//! reached through a neutral link of the same similarity: graph expansion
//! scores a neighbour as similarity * link_type_weight.

use kleos_lib::memory::scoring::link_type_weight;

const NEUTRAL: &str = "similarity";

#[test]
fn a_contradicting_link_gets_no_boost_over_a_neutral_link() {
    assert!(
        link_type_weight("contradicts") <= link_type_weight(NEUTRAL),
        "contradicts {} > neutral {}",
        link_type_weight("contradicts"),
        link_type_weight(NEUTRAL)
    );
}

#[test]
fn the_other_typed_links_keep_their_boost() {
    for link_type in ["extends", "updates", "corrects", "caused_by", "causes"] {
        assert!(
            link_type_weight(link_type) > link_type_weight(NEUTRAL),
            "{link_type} lost its boost"
        );
    }
}
