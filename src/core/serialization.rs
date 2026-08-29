//! Format-neutral strict deserializers for ordered persistent collections.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;
use std::marker::PhantomData;

use serde::de::{Deserialize, DeserializeSeed, Deserializer, Error, MapAccess, SeqAccess, Visitor};

/// Deserializes an ordered map while rejecting repeated keys instead of silently overwriting them.
pub(crate) fn deserialize_btree_map_no_duplicates<'de, D, K, V>(
    deserializer: D,
) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct StrictMapVisitor<K, V>(PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for StrictMapVisitor<K, V>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
    {
        type Value = BTreeMap<K, V>;

        fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an ordered map without duplicate keys")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = access.next_entry()? {
                if values.insert(key, value).is_some() {
                    return Err(A::Error::custom("duplicate ordered-map key"));
                }
            }
            Ok(values)
        }
    }

    deserializer.deserialize_map(StrictMapVisitor(PhantomData))
}

struct StrictSetSeed<T>(PhantomData<T>);

impl<'de, T> DeserializeSeed<'de> for StrictSetSeed<T>
where
    T: Deserialize<'de> + Ord,
{
    type Value = BTreeSet<T>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StrictSetVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for StrictSetVisitor<T>
        where
            T: Deserialize<'de> + Ord,
        {
            type Value = BTreeSet<T>;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an ordered set without duplicate elements")
            }

            fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = BTreeSet::new();
                while let Some(value) = access.next_element()? {
                    if !values.insert(value) {
                        return Err(A::Error::custom("duplicate ordered-set element"));
                    }
                }
                Ok(values)
            }
        }

        deserializer.deserialize_seq(StrictSetVisitor(PhantomData))
    }
}

/// Deserializes an ordered map of ordered sets with duplicate rejection at both collection levels.
pub(crate) fn deserialize_btree_map_of_sets_no_duplicates<'de, D, K, V>(
    deserializer: D,
) -> Result<BTreeMap<K, BTreeSet<V>>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de> + Ord,
{
    struct StrictMapOfSetsVisitor<K, V>(PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for StrictMapOfSetsVisitor<K, V>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de> + Ord,
    {
        type Value = BTreeMap<K, BTreeSet<V>>;

        fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an ordered map of ordered sets without duplicate entries")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut values = BTreeMap::new();
            while let Some(key) = access.next_key()? {
                let set = access.next_value_seed(StrictSetSeed(PhantomData))?;
                if values.insert(key, set).is_some() {
                    return Err(A::Error::custom("duplicate ordered-map key"));
                }
            }
            Ok(values)
        }
    }

    deserializer.deserialize_map(StrictMapOfSetsVisitor(PhantomData))
}

#[cfg(test)]
#[path = "serialization_tests.rs"]
mod tests;
