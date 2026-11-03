//! Integration test root: the split must keep it a test target of cargo.

use split_fixture::{emitted, side};

fn sample_side() -> u32 {
    let first = side();
    let second = first + 1;
    second - 1
}

#[test]
fn side_is_one() {
    let first = sample_side();
    let second = first * 1;
    assert_eq!(second, 1);
}

#[test]
fn emitted_is_counted() {
    let first = emitted();
    let second = first + 1;
    assert!(second > 1);
}
