//! Read-only world matter accounting across stored lots and durable in-process production ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::AggregateMass;
use crate::core::state::AppState;

/// World-scale matter projection split by its current authoritative owner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatterAccounting {
    stored: AggregateMass,
    in_process: AggregateMass,
    total: AggregateMass,
}

impl MatterAccounting {
    /// Matter currently owned by inventory lots.
    #[must_use]
    pub const fn stored(self) -> AggregateMass {
        self.stored
    }

    /// Matter currently owned by durable production-job output snapshots.
    #[must_use]
    pub const fn in_process(self) -> AggregateMass {
        self.in_process
    }

    /// Total matter represented by the implemented authoritative matter owners.
    #[must_use]
    pub const fn total(self) -> AggregateMass {
        self.total
    }
}

/// Overflow while projecting world-scale matter ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatterAccountingError {
    StoredMassOverflow,
    InProcessMassOverflow,
    TotalMassOverflow,
}

impl Display for MatterAccountingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StoredMassOverflow => {
                formatter.write_str("stored world matter exceeds aggregate mass range")
            }
            Self::InProcessMassOverflow => {
                formatter.write_str("in-process world matter exceeds aggregate mass range")
            }
            Self::TotalMassOverflow => {
                formatter.write_str("total world matter exceeds aggregate mass range")
            }
        }
    }
}

impl Error for MatterAccountingError {}

/// Recomputes matter ownership from authoritative records without trusting stockpile caches.
///
/// Production inputs are removed from inventory at process start. The running job's resolved output
/// snapshot becomes the durable owner of that same matter until completion. Reserved inbound
/// capacity is therefore not additional matter and is deliberately excluded from this projection.
pub fn calculate_matter_accounting(
    state: &AppState,
) -> Result<MatterAccounting, MatterAccountingError> {
    let mut stored = AggregateMass::ZERO;
    for lot in state.inventory().lots() {
        stored = stored
            .checked_add(AggregateMass::from_mass(lot.mass()))
            .ok_or(MatterAccountingError::StoredMassOverflow)?;
    }

    let mut in_process = AggregateMass::ZERO;
    for job in state.production().jobs() {
        for output in job.outputs() {
            in_process = in_process
                .checked_add(AggregateMass::from_mass(output.mass()))
                .ok_or(MatterAccountingError::InProcessMassOverflow)?;
        }
    }

    let total = stored
        .checked_add(in_process)
        .ok_or(MatterAccountingError::TotalMassOverflow)?;
    Ok(MatterAccounting {
        stored,
        in_process,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, FORM_LUMP, MATERIAL_CHARCOAL, MATERIAL_WOOD, make_test_registries_with_process,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::core::time::WorldSeed;
    use crate::inventory::{add_stockpile, deposit_bulk_for_test};
    use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};
    use crate::production::{
        ProcessDefinition, ProcessId, make_test_process_resolution, validate_start_process,
    };
    use crate::simulation::advance_tick;

    const PROCESS: ProcessId = ProcessId::new(910_001);

    #[test]
    fn process_start_and_completion_preserve_world_matter_ownership_total() {
        let process = ProcessDefinition::new(
            PROCESS,
            "matter accounting fixture",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        );
        let registries = make_test_registries_with_process(process);
        let resolution = make_test_process_resolution(
            PROCESS,
            1,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(500_000),
            )],
        );
        let mut state = AppState::new(WorldSeed::new(0x0ACC_0017));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(id) => id,
            Err(error) => panic!("source fixture failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(id) => id,
            Err(error) => panic!("destination fixture failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
        ) {
            panic!("matter fixture deposit failed: {error}");
        }
        let before = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting,
            Err(error) => panic!("initial accounting failed: {error}"),
        };

        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("process validation failed: {error}"),
            };
        if let Err(error) = token.commit(&mut state) {
            panic!("process commit failed: {error}");
        }
        let running = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting,
            Err(error) => panic!("running accounting failed: {error}"),
        };

        assert_eq!(before.total(), running.total());
        assert_eq!(running.stored(), AggregateMass::ZERO);
        assert_eq!(running.in_process(), AggregateMass::from_milligrams(10));

        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion tick failed: {error}");
        }
        let completed = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting,
            Err(error) => panic!("completed accounting failed: {error}"),
        };

        assert_eq!(before.total(), completed.total());
        assert_eq!(completed.in_process(), AggregateMass::ZERO);
        assert_eq!(completed.stored(), AggregateMass::from_milligrams(10));
    }
}
