//! Operation-specific production resolution; sibling definitions describe requirements while execution commits resolved outcomes.

use crate::core::time::TickSpan;
use crate::material::MaterialLotSpec;

use super::definitions::ProcessId;
#[cfg(test)]
use super::definitions::validate_resolved_outputs;

/// Immutable outcome of physical process resolution for one specific operation.
///
/// This type deliberately has no public constructor. Systems such as metallurgy, thermal
/// processing, equipment capability, and labor will eventually resolve authored process
/// requirements into this operation-specific duration and exact output-lot snapshot. The
/// production transaction accepts only a resolved value and never invents physical outcomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessResolution {
    process: ProcessId,
    duration: TickSpan,
    outputs: Vec<MaterialLotSpec>,
}

impl ProcessResolution {
    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub fn outputs(&self) -> &[MaterialLotSpec] {
        &self.outputs
    }
}

pub(crate) fn sum_lot_spec_mass(
    entries: &[MaterialLotSpec],
) -> Option<crate::core::quantity::Mass> {
    super::definitions::sum_lot_spec_mass(entries)
}

#[cfg(test)]
pub(crate) fn make_test_process_resolution(
    process: ProcessId,
    duration_ticks: u64,
    mut outputs: Vec<MaterialLotSpec>,
) -> ProcessResolution {
    assert!(
        process.value() != 0,
        "test process resolution id must be nonzero"
    );
    assert!(
        duration_ticks > 0,
        "test process resolution duration must be nonzero"
    );
    outputs.sort();
    validate_resolved_outputs(process, &outputs);
    assert!(
        sum_lot_spec_mass(&outputs).is_some(),
        "test process resolution output mass overflows"
    );
    ProcessResolution {
        process,
        duration: TickSpan::new(duration_ticks),
        outputs,
    }
}
