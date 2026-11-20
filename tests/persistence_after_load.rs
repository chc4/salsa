#![cfg(all(feature = "persistence", feature = "inventory"))]

//! A database loaded from a snapshot verifies a query's dependencies before anything has called
//! them.

use salsa::{Database, Setter};

#[salsa::input(persist, singleton)]
struct Text {
    value: String,
}

#[salsa::tracked(returns(copy), persist)]
fn length(db: &dyn Database) -> usize {
    Text::get(db).value(db).len()
}

#[salsa::tracked(returns(copy), persist)]
fn is_even(db: &dyn Database) -> bool {
    length(db) % 2 == 0
}

#[salsa::db]
trait Words: Database {
    fn separator(&self) -> char;
}

#[salsa::db]
trait Sentences: Words {}

#[salsa::tracked(returns(copy), persist)]
fn words(db: &dyn Words) -> usize {
    Text::get(db).value(db).split(db.separator()).count()
}

/// Calls `words` through the `Sentences` view, so that no query the test calls takes `Words`.
#[salsa::tracked(returns(copy), persist)]
fn several_words(db: &dyn Sentences) -> bool {
    words(db) > 1
}

#[salsa::db]
#[derive(Default, Clone)]
struct Db {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

#[salsa::db]
impl Words for Db {
    fn separator(&self) -> char {
        ' '
    }
}

#[salsa::db]
impl Sentences for Db {}

/// Runs `run` on a database whose text is `value`, then saves the database and loads it into a
/// fresh one.
fn reloaded(value: &str, run: impl Fn(&Db)) -> Db {
    let mut db = Db::default();
    Text::new(&db, value.to_string());
    run(&db);
    let serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();
    let mut loaded = Db::default();
    <dyn Database>::deserialize(&mut loaded, &mut serde_json::Deserializer::from_str(&serialized))
        .unwrap();
    loaded
}

#[test]
fn a_dependency_is_verified_after_its_input_changes() {
    let mut db = reloaded("abcd", |db| {
        is_even(db);
    });
    Text::get(&db).set_value(&mut db).to("abc".to_string());
    assert!(!is_even(&db));
}

#[test]
fn a_dependency_taking_another_view_is_verified_after_its_input_changes() {
    let mut db = reloaded("one", |db| {
        several_words(db);
    });
    Text::get(&db).set_value(&mut db).to("one two".to_string());
    assert!(several_words(&db));
}
