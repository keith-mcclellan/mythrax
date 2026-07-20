use mythrax_core::retrieval::boosts::{BoostSignals, BoostWeights, apply_boosts};

#[test]
fn boosts_clamp_to_zero_two_range() {
    let w = BoostWeights::default();
    let d = apply_boosts(
        0.10,
        &BoostSignals {
            person_name: true,
            exact_quote: true,
            temporal_proximity: 0.0,
            keyword_overlap: 0.0,
            ..Default::default()
        },
        &w,
    );
    assert!(d >= -2.0 && d <= 2.0);
}

#[test]
fn person_name_reduces_distance_about_40pct() {
    let w = BoostWeights::default();
    let base = 1.0;
    let boosted = apply_boosts(
        base,
        &BoostSignals {
            person_name: true,
            exact_quote: false,
            temporal_proximity: 0.0,
            keyword_overlap: 0.0,
            ..Default::default()
        },
        &w,
    );
    assert!(boosted < base);
    assert!((boosted - 0.60).abs() < 1e-3); // -40% (per REFERENCE BEHAVIORS)
}

#[test]
fn quoted_phrase_reduces_distance_about_60pct() {
    let w = BoostWeights::default();
    let base = 1.0;
    let boosted = apply_boosts(
        base,
        &BoostSignals {
            person_name: false,
            exact_quote: true,
            temporal_proximity: 0.0,
            keyword_overlap: 0.0,
            ..Default::default()
        },
        &w,
    );
    assert!(boosted < base);
    assert!((boosted - 0.40).abs() < 1e-3); // -60% (per REFERENCE BEHAVIORS)
}

#[test]
fn no_signals_is_identity() {
    assert_eq!(
        apply_boosts(0.73, &BoostSignals::default(), &BoostWeights::default()),
        0.73
    );
}
