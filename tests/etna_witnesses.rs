//! Witness tests for the ETNA workload.
//!
//! Each `witness_*` test calls one of the `property_*` functions in
//! `lru::etna` with frozen inputs. Tests pass on the base commit and fail
//! when the corresponding mutation is active.

use lru::etna::{
    property_clear_resize_drops_all, property_clone_unbounded_no_panic,
    property_drop_impl_drops_all, property_pop_drops_key, property_pop_iter_consistent,
    PropertyResult,
};

fn assert_pass(r: PropertyResult) {
    match r {
        PropertyResult::Pass => {}
        PropertyResult::Fail(m) => panic!("property failed: {m}"),
        PropertyResult::Discard => panic!("property unexpectedly discarded"),
    }
}

// ---- clone_unbounded_no_panic_3ec42b6_1 ----

#[test]
fn witness_clone_unbounded_no_panic_case_three_entries() {
    // Cloning an unbounded cache with several entries should not panic.
    let items = vec![(1u32, 10u32), (2, 20), (3, 30)];
    assert_pass(property_clone_unbounded_no_panic(items));
}

#[test]
fn witness_clone_unbounded_no_panic_case_one_entry() {
    let items = vec![(42u32, 99u32)];
    assert_pass(property_clone_unbounded_no_panic(items));
}

#[test]
fn witness_clone_unbounded_no_panic_case_empty() {
    let items: Vec<(u32, u32)> = vec![];
    assert_pass(property_clone_unbounded_no_panic(items));
}

// ---- pop_iter_consistent_5f4a46a_1 ----

#[test]
fn witness_pop_iter_consistent_case_pop_middle() {
    // 5 keys, pop the third (index 2) → iter must not visit it.
    let keys = vec![1u32, 2, 3, 4, 5];
    assert_pass(property_pop_iter_consistent((keys, 2)));
}

#[test]
fn witness_pop_iter_consistent_case_pop_first() {
    let keys = vec![10u32, 20, 30];
    assert_pass(property_pop_iter_consistent((keys, 0)));
}

#[test]
fn witness_pop_iter_consistent_case_pop_last() {
    let keys = vec![100u32, 200, 300, 400];
    assert_pass(property_pop_iter_consistent((keys, 3)));
}

// ---- drop_impl_drops_all_37dbda0_1 ----

#[test]
fn witness_drop_impl_drops_all_case_one() {
    assert_pass(property_drop_impl_drops_all(vec![1u32]));
}

#[test]
fn witness_drop_impl_drops_all_case_many() {
    assert_pass(property_drop_impl_drops_all(vec![1u32, 2, 3, 4, 5, 6, 7, 8]));
}

// ---- clear_resize_drops_all_4460158_1 ----

#[test]
fn witness_clear_resize_drops_all_case_clear() {
    assert_pass(property_clear_resize_drops_all((vec![1u32, 2, 3, 4], true)));
}

#[test]
fn witness_clear_resize_drops_all_case_resize() {
    assert_pass(property_clear_resize_drops_all((vec![1u32, 2, 3, 4, 5], false)));
}

#[test]
fn witness_clear_resize_drops_all_case_clear_one() {
    assert_pass(property_clear_resize_drops_all((vec![42u32], true)));
}

// ---- pop_drops_key_ea64c8f_1 ----

#[test]
fn witness_pop_drops_key_case_one() {
    assert_pass(property_pop_drops_key(vec![1u32]));
}

#[test]
fn witness_pop_drops_key_case_many() {
    assert_pass(property_pop_drops_key(vec![1u32, 2, 3, 4, 5]));
}
