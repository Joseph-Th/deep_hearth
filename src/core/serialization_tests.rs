//! Contract tests for strict persistent ordered-collection deserialization.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictMapFixture {
    #[serde(deserialize_with = "super::deserialize_btree_map_no_duplicates")]
    values: BTreeMap<u32, u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictMapOfSetsFixture {
    #[serde(deserialize_with = "super::deserialize_btree_map_of_sets_no_duplicates")]
    values: BTreeMap<u32, BTreeSet<u32>>,
}

#[test]
fn strict_map_accepts_unique_entries_and_rejects_duplicate_raw_json_keys() {
    let valid = serde_json::from_str::<StrictMapFixture>(r#"{"values":{"1":10,"2":20}}"#)
        .unwrap_or_else(|error| panic!("valid strict-map payload failed: {error}"));
    assert_eq!(valid.values, BTreeMap::from([(1, 10), (2, 20)]));

    let error = match serde_json::from_str::<StrictMapFixture>(r#"{"values":{"1":10,"1":20}}"#) {
        Ok(_) => panic!("duplicate persistent map keys were accepted"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("duplicate ordered-map key"),
        "unexpected duplicate-map error: {error}"
    );
}

#[test]
fn strict_map_of_sets_accepts_unique_entries_and_rejects_duplicates() {
    let valid =
        serde_json::from_str::<StrictMapOfSetsFixture>(r#"{"values":{"1":[10,20],"2":[30]}}"#)
            .unwrap_or_else(|error| panic!("valid strict-map-of-sets payload failed: {error}"));
    assert_eq!(
        valid.values,
        BTreeMap::from([(1, BTreeSet::from([10, 20])), (2, BTreeSet::from([30]))])
    );

    let duplicate_key =
        match serde_json::from_str::<StrictMapOfSetsFixture>(r#"{"values":{"1":[10],"1":[20]}}"#) {
            Ok(_) => panic!("duplicate persistent map-of-set keys were accepted"),
            Err(error) => error,
        };
    assert!(
        duplicate_key
            .to_string()
            .contains("duplicate ordered-map key"),
        "unexpected duplicate-map error: {duplicate_key}"
    );

    let duplicate_member =
        match serde_json::from_str::<StrictMapOfSetsFixture>(r#"{"values":{"1":[10,10]}}"#) {
            Ok(_) => panic!("duplicate persistent set members were accepted"),
            Err(error) => error,
        };
    assert!(
        duplicate_member
            .to_string()
            .contains("duplicate ordered-set element"),
        "unexpected duplicate-set error: {duplicate_member}"
    );
}
