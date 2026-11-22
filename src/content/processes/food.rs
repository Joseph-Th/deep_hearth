//! Primitive preservation process names exposed through the ordinary recipe book.

use crate::production::ProcessDefinition;

use super::{PROCESS_CURE_WILD_TUBERS, PROCESS_SORT_WILD_NUTS_FOR_WINTER};

pub(super) fn definitions() -> [ProcessDefinition; 2] {
    [
        ProcessDefinition::new(
            PROCESS_SORT_WILD_NUTS_FOR_WINTER,
            "sort wild nuts for winter",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_CURE_WILD_TUBERS,
            "trim and cure wild tubers",
            Vec::new(),
        ),
    ]
}
