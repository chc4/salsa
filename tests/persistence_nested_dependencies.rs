#![cfg(all(feature = "persistence", feature = "inventory"))]

//! A snapshot records each persisted memo's dependencies down to the nearest persisted queries, and
//! a database loaded from it still sees every input each memo read.

use salsa::{Database, Setter};

#[salsa::input(persist, singleton)]
struct Text {
    value: String,
}

#[salsa::input(persist)]
struct Key {
    offset: usize,
}

#[salsa::tracked(returns(copy))]
fn inner(db: &dyn Database) -> usize {
    Text::get(db).value(db).len()
}

#[salsa::tracked(returns(copy))]
fn outer(db: &dyn Database) -> usize {
    inner(db) * 10
}

/// Persisted, with an entry for each key, each reading the same queries that are not persisted.
#[salsa::tracked(returns(copy), persist)]
fn total(db: &dyn Database, key: Key) -> usize {
    outer(db) + key.offset(db)
}

#[salsa::tracked(returns(copy), persist)]
fn counted(db: &dyn Database) -> usize {
    Text::get(db).value(db).chars().filter(|c| *c == 'a').count()
}

#[salsa::tracked(returns(copy))]
fn doubled(db: &dyn Database) -> usize {
    counted(db) * 2
}

/// Persisted, reading a persisted query through one that is not.
#[salsa::tracked(returns(copy), persist)]
fn through(db: &dyn Database) -> usize {
    doubled(db) + 1
}

#[salsa::db]
#[derive(Default, Clone)]
struct Db {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

/// The snapshot of `db`, as JSON.
fn snapshot(db: &mut Db) -> String {
    serde_json::to_string(&<dyn Database>::as_serialize(db)).unwrap()
}

fn load(snapshot: &str) -> Db {
    let mut db = Db::default();
    <dyn Database>::deserialize(&mut db, &mut serde_json::Deserializer::from_str(snapshot))
        .unwrap();
    db
}

#[test]
fn every_memo_sees_the_inputs_read_through_queries_another_memo_read_first() {
    let mut db = Db::default();
    Text::new(&db, "abc".to_string());
    let keys = [Key::new(&db, 1), Key::new(&db, 2)];
    for &key in &keys {
        total(&db, key);
    }
    let mut loaded = load(&snapshot(&mut db));
    Text::get(&loaded).set_value(&mut loaded).to("abcdef".to_string());
    assert_eq!(keys.map(|key| total(&loaded, key)), [61, 62]);
}

#[test]
fn a_persisted_query_read_through_another_is_recorded_in_place_of_its_inputs() {
    let mut db = Db::default();
    Text::new(&db, "banana".to_string());
    through(&db);
    let saved = snapshot(&mut db);
    let mut loaded = load(&saved);
    Text::get(&loaded).set_value(&mut loaded).to("aa".to_string());
    assert_eq!(through(&loaded), 5);

    // In the snapshot, `through` (7 for "banana") has one edge, to the ingredient of `counted` (3).
    let body: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let memo_of = |value: u64| {
        body["ingredients"]
            .as_object()
            .unwrap()
            .iter()
            .find_map(|(ingredient, memos)| {
                let memo = memos.as_object()?.values().find(|m| m["value"] == value)?;
                Some((ingredient.parse::<u64>().unwrap(), memo.clone()))
            })
            .unwrap()
    };
    let (counted_ingredient, _) = memo_of(3);
    let (_, through_memo) = memo_of(7);
    let edges = through_memo["revisions"]["origin"]["Derived"].as_array().unwrap().clone();
    let ingredients: Vec<u64> = edges.iter().map(|e| e[1].as_u64().unwrap()).collect();
    assert_eq!(ingredients, [counted_ingredient]);
}
