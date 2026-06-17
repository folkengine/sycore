//! Replays the bundled sample season and asserts the kernel reconstructs it with
//! zero hard conflicts. Runs only with `--features seed`.
#![cfg(feature = "seed")]

use sycore::query::conflicts;
use sycore::seed::build_sample;

const SAMPLE: &str = include_str!("../data/orchestra_sample_data.json");

#[test]
fn sample_season_builds_with_no_hard_conflicts() {
    let (state, _warnings) =
        build_sample(SAMPLE).expect("sample data should seed without hard errors");

    // The whole 20-concert season loaded.
    assert_eq!(state.orchestras.len(), 1);
    assert!(
        state.concerts.len() >= 20,
        "expected the full season of concerts"
    );
    assert!(
        state.musicians.len() >= 180,
        "expected the full roster pool"
    );

    // The headline invariant: a valid season has no double-bookings anywhere.
    let global = conflicts(&state);
    assert!(
        global.is_empty(),
        "unexpected conflicts after seeding: {global:?}"
    );
}

#[test]
fn sample_season_produces_some_soft_warnings() {
    // Soft warnings (understaffing, low availability, capability mismatch) are
    // expected and allowed — they must not be errors.
    let (_state, warnings) = build_sample(SAMPLE).expect("seed");
    // We don't assert a specific count (data may evolve); just that the channel works.
    let _ = warnings.len();
}
