//! Registry-derived gameplay planning for immutable powered-craft work requirements.

use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::project_powered_craft_equipment_work;
use deep_hearth::energy::{EnergyStoreId, calculate_mass_specific_energy};
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::maintenance::Condition;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AuthoredPoweredCraftBatch {
    pub(super) input_mass: Mass,
    pub(super) work: Energy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredCraftLegProjection {
    pub(super) batches: u64,
    pub(super) input_mass: Mass,
    pub(super) work: Energy,
    pub(super) delegated_ticks: u64,
    pub(super) condition_after: Condition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PoweredCraftSequenceProjection {
    pub(super) batches: u64,
    pub(super) delegated_ticks: u64,
    pub(super) legs: Vec<PoweredCraftLegProjection>,
}

impl PoweredCraftSequenceProjection {
    pub(super) fn charge_energies(&self) -> impl Iterator<Item = Energy> + '_ {
        self.legs.iter().map(|leg| leg.work)
    }

    pub(super) fn charge_events(&self) -> u64 {
        u64::try_from(self.legs.len())
            .unwrap_or_else(|_| panic!("powered-craft leg count exceeds u64"))
    }

    pub(super) fn maximum_leg_batches(&self) -> u64 {
        self.legs.iter().map(|leg| leg.batches).max().unwrap_or(0)
    }
}

/// Returns the exact input mass and stored-work requirement for one authored transform batch.
pub(super) fn authored_batch(
    registries: &Registries,
    process: ProcessId,
    context: &'static str,
) -> AuthoredPoweredCraftBatch {
    let powered = registries
        .crafting()
        .get_powered(process)
        .unwrap_or_else(|| panic!("gameplay harness {context} powered craft disappeared"));
    let transform = registries
        .crafting()
        .get_manual(powered.transform())
        .unwrap_or_else(|| {
            panic!("gameplay harness {context} powered craft references missing manual transform")
        });
    let input_mass = transform.input_mass();
    AuthoredPoweredCraftBatch {
        input_mass,
        work: calculate_mass_specific_energy(input_mass, powered.specific_energy()),
    }
}

fn powered_input_mass(
    batch: AuthoredPoweredCraftBatch,
    batches: u64,
    context: &'static str,
) -> Mass {
    Mass::from_milligrams(
        batch
            .input_mass
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("gameplay harness {context} powered input mass overflowed")),
    )
}

fn maximum_feasible_batches(upper: u64, mut feasible: impl FnMut(u64) -> bool) -> u64 {
    let mut low = 0_u64;
    let mut high = upper;
    while low < high {
        let midpoint = low + (high - low).div_ceil(2);
        if feasible(midpoint) {
            low = midpoint;
        } else {
            high = midpoint - 1;
        }
    }
    low
}

/// Projects the fewest capacity-aware powered jobs needed to execute a declared batch workload.
///
/// Each leg is the largest currently feasible integral batch request for the live finite store and
/// the carried-forward equipment condition. This keeps the harness from inventing one player
/// interaction per authored transform batch when the production API can execute several batches in
/// one powered request.
pub(super) fn project_powered_craft_sequence(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    equipment: EquipmentDefinitionId,
    mut condition: Condition,
    drive: EnergyStoreId,
    batch: AuthoredPoweredCraftBatch,
    requested_batches: u64,
    context: &'static str,
) -> PoweredCraftSequenceProjection {
    let mut batches = 0_u64;
    let mut delegated_ticks = 0_u64;
    let mut legs = Vec::new();
    while batches < requested_batches {
        let remaining = requested_batches
            .checked_sub(batches)
            .unwrap_or_else(|| unreachable!("projected powered batches cannot exceed request"));
        let leg_batches = maximum_feasible_batches(remaining, |candidate| {
            candidate == 0
                || project_powered_craft_equipment_work(
                    registries,
                    state,
                    process,
                    powered_input_mass(batch, candidate, context),
                    equipment,
                    condition,
                    drive,
                )
                .is_ok()
        });
        if leg_batches == 0 {
            break;
        }
        let input_mass = powered_input_mass(batch, leg_batches, context);
        let projection = project_powered_craft_equipment_work(
            registries, state, process, input_mass, equipment, condition, drive,
        )
        .unwrap_or_else(|error| {
            panic!("gameplay harness {context} feasible packed-work projection failed: {error}")
        });
        let leg = PoweredCraftLegProjection {
            batches: leg_batches,
            input_mass,
            work: projection.required_energy(),
            delegated_ticks: projection.duration().value(),
            condition_after: projection.condition_after(),
        };
        batches = batches
            .checked_add(leg.batches)
            .unwrap_or_else(|| panic!("gameplay harness {context} batch count overflowed"));
        delegated_ticks = delegated_ticks
            .checked_add(leg.delegated_ticks)
            .unwrap_or_else(|| panic!("gameplay harness {context} delegated time overflowed"));
        condition = leg.condition_after;
        legs.push(leg);
    }
    PoweredCraftSequenceProjection {
        batches,
        delegated_ticks,
        legs,
    }
}
