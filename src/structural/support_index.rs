//! Shared deterministic reverse-index transition for runtime owners mounted on structural elements.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

use super::state::StructuralElementId;

/// Structural inconsistency in a runtime owner's derived support reverse index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SupportIndexValidationFault<Id> {
    ZeroSupportElementId,
    EmptySupportBucket {
        element: StructuralElementId,
    },
    InvalidItemId {
        item: Id,
        element: StructuralElementId,
    },
    UnknownIndexedItem {
        item: Id,
        element: StructuralElementId,
    },
    SupportMismatch {
        item: Id,
        indexed: StructuralElementId,
        actual: Option<StructuralElementId>,
    },
}

fn validate_indexed_item<Id>(
    item: Id,
    element: StructuralElementId,
    invalid_item_id: &impl Fn(Id) -> bool,
    actual_support: &impl Fn(Id) -> Option<Option<StructuralElementId>>,
) -> Result<(), SupportIndexValidationFault<Id>>
where
    Id: Copy,
{
    if invalid_item_id(item) {
        return Err(SupportIndexValidationFault::InvalidItemId { item, element });
    }
    let Some(actual) = actual_support(item) else {
        return Err(SupportIndexValidationFault::UnknownIndexedItem { item, element });
    };
    if actual != Some(element) {
        return Err(SupportIndexValidationFault::SupportMismatch {
            item,
            indexed: element,
            actual,
        });
    }
    Ok(())
}

fn validate_support_bucket<Id>(
    element: StructuralElementId,
    items: &BTreeSet<Id>,
    invalid_item_id: &impl Fn(Id) -> bool,
    actual_support: &impl Fn(Id) -> Option<Option<StructuralElementId>>,
) -> Result<(), SupportIndexValidationFault<Id>>
where
    Id: Copy + Ord,
{
    if element.value() == 0 {
        return Err(SupportIndexValidationFault::ZeroSupportElementId);
    }
    if items.is_empty() {
        return Err(SupportIndexValidationFault::EmptySupportBucket { element });
    }
    for item in items.iter().copied() {
        validate_indexed_item(item, element, invalid_item_id, actual_support)?;
    }
    Ok(())
}

/// Validates one owner's derived structural-support reverse index against authoritative records.
pub(crate) fn validate_support_index<Id>(
    index: &BTreeMap<StructuralElementId, BTreeSet<Id>>,
    invalid_item_id: impl Fn(Id) -> bool,
    actual_support: impl Fn(Id) -> Option<Option<StructuralElementId>>,
) -> Result<(), SupportIndexValidationFault<Id>>
where
    Id: Copy + Ord,
{
    for (element, items) in index {
        validate_support_bucket(*element, items, &invalid_item_id, &actual_support)?;
    }
    Ok(())
}

pub(crate) fn assert_support_index_change_available<Id>(
    index: &BTreeMap<StructuralElementId, BTreeSet<Id>>,
    item: Id,
    before: Option<StructuralElementId>,
    after: Option<StructuralElementId>,
) where
    Id: Copy + Debug + Ord,
{
    if let Some(before) = before {
        assert!(
            index
                .get(&before)
                .is_some_and(|indexed| indexed.contains(&item)),
            "runtime invariant broken: structural support index {before:?} is missing {item:?}"
        );
    }
    if after != before
        && let Some(after) = after
    {
        assert!(
            !index
                .get(&after)
                .is_some_and(|indexed| indexed.contains(&item)),
            "runtime invariant broken: structural support index {after:?} already contains {item:?}"
        );
    }
}

/// Applies one already-validated structural-support reassignment to an owner's reverse index.
///
/// The owning subsystem remains responsible for its authoritative record and revision. This operation
/// owns only the repeated derived-index invariant: the prior membership must exist, a distinct target
/// membership must not already exist, empty buckets are removed, and the final membership is unique.
pub(crate) fn apply_support_index_change<Id>(
    index: &mut BTreeMap<StructuralElementId, BTreeSet<Id>>,
    item: Id,
    before: Option<StructuralElementId>,
    after: Option<StructuralElementId>,
) where
    Id: Copy + Debug + Ord,
{
    assert_support_index_change_available(index, item, before, after);

    if let Some(before) = before {
        let remove_bucket = {
            let indexed = index.get_mut(&before).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: structural support index lost {before:?} while moving {item:?}"
                )
            });
            assert!(
                indexed.remove(&item),
                "runtime invariant broken: structural support index {before:?} lost {item:?} while moving it"
            );
            indexed.is_empty()
        };
        if remove_bucket {
            index.remove(&before);
        }
    }

    if let Some(after) = after {
        assert!(
            index.entry(after).or_default().insert(item),
            "runtime invariant broken: structural support index {after:?} duplicated {item:?}"
        );
    }
}
