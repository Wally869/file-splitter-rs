//! A module of `suite.rs`, not a test target: its split must stay a module.

use split_fixture::side;

fn doubled_side() -> u32 {
    let first = side();
    let second = first * 2;
    second
}

#[test]
fn doubled_side_is_two() {
    let first = doubled_side();
    let second = first + 0;
    assert_eq!(second, 2);
}

#[test]
fn side_is_positive() {
    let first = side();
    let second = first + 0;
    assert!(second > 0);
}
