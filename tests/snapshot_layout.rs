#![cfg(all(feature = "persistence", feature = "inventory"))]

//! The ingredients a snapshot numbers, as `snapshot_layout` lists them.

use salsa::{Database, SnapshotIngredient};

#[salsa::input(persist, singleton)]
struct Text {
    value: String,
}

#[salsa::tracked(returns(copy), persist)]
fn length(db: &dyn Database) -> usize {
    Text::get(db).value(db).len()
}

#[salsa::tracked(returns(copy))]
fn is_even(db: &dyn Database) -> bool {
    length(db) % 2 == 0
}

#[test]
fn the_layout_lists_every_ingredient_in_index_order() {
    let db = salsa::DatabaseImpl::new();
    let ingredient = |jar, name, persisted| SnapshotIngredient {
        jar,
        name,
        persisted,
    };
    // Structs come before tracked functions, and jars of each kind by type name.
    assert_eq!(
        <dyn Database>::snapshot_layout(&db),
        [
            ingredient("salsa::input::JarImpl<snapshot_layout::Text>", "Text", true),
            ingredient("salsa::input::JarImpl<snapshot_layout::Text>", "value", true),
            ingredient("snapshot_layout::is_even", "is_even", false),
            ingredient(
                "snapshot_layout::is_even",
                "is_even::interned_arguments",
                false
            ),
            ingredient("snapshot_layout::length", "length", true),
            ingredient("snapshot_layout::length", "length::interned_arguments", true),
        ]
    );
}
