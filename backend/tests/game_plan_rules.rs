use bgtutor::game_plan::{GamePlan, classify};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
struct ReferenceFixture {
    name: String,
    board: [i32; 26],
    white: GamePlan,
    black: GamePlan,
}

fn reference_fixtures() -> Vec<ReferenceFixture> {
    serde_json::from_str(include_str!("fixtures/game_plan_reference.json")).unwrap()
}

#[test]
fn matches_pinned_cpp_reference_for_all_classes_and_color_pairs() {
    let mut classes = HashSet::new();
    let mut pairs = HashSet::new();
    for fixture in reference_fixtures() {
        let result = classify(&fixture.board).unwrap();
        assert_eq!(result.white, fixture.white, "{} white", fixture.name);
        assert_eq!(result.black, fixture.black, "{} black", fixture.name);
        classes.insert(serde_json::to_string(&result.white).unwrap());
        classes.insert(serde_json::to_string(&result.black).unwrap());
        pairs.insert((
            serde_json::to_string(&result.white).unwrap(),
            serde_json::to_string(&result.black).unwrap(),
        ));
    }
    assert_eq!(classes.len(), 5);
    assert_eq!(pairs.len(), 17, "pure race plus all 16 contact class pairs");
}

#[test]
fn color_swap_and_point_reflection_swap_absolute_labels() {
    for fixture in reference_fixtures() {
        // BG Tutor's bars retain absolute signs under this transformation.
        let flipped = std::array::from_fn(|point| -fixture.board[25 - point]);
        let result = classify(&flipped).unwrap();
        assert_eq!(
            result.white, fixture.black,
            "{} flipped white",
            fixture.name
        );
        assert_eq!(
            result.black, fixture.white,
            "{} flipped black",
            fixture.name
        );
        assert_eq!(
            result.game_over,
            classify(&fixture.board).unwrap().game_over
        );
    }
}

#[test]
fn negative_black_bar_prevents_a_pure_race_and_supplies_attacking_target() {
    let fixture = reference_fixtures()
        .into_iter()
        .find(|fixture| fixture.name == "black_bar_sign")
        .unwrap();
    let result = classify(&fixture.board).unwrap();
    assert_eq!(result.white, GamePlan::Attacking);
    assert_ne!(result.black, GamePlan::PureRace);
    assert!(!result.game_over);
}

#[test]
fn invalid_values_fail_before_pip_arithmetic_or_sign_negation() {
    for value in [i32::MIN, i32::MAX, -16, 16] {
        for point in 0..26 {
            let mut board = [0; 26];
            board[point] = value;
            assert!(classify(&board).is_err(), "point {point}: {value}");
        }
    }
}
