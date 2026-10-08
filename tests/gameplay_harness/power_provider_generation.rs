//! Ordinary workload and inherited-pressure generation for human-power evaluation.

use deep_hearth::content::{
    EQUIPMENT_STONE_CRUSHER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_CRUSH_ORE,
    PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::registry::Registries;

use super::bulk_fieldwork_workload::{
    BULK_FIELDWORK_ORDER_MAX_BATCHES, BULK_FIELDWORK_ORDER_MIN_BATCHES, bulk_fieldwork_order_mass,
    primitive_quarry_batch_mass,
};
use super::equipment_support::nominal_equipment_mass_capability;
use super::primitive_workload::{STOCKPILE_WORK_ORDER_CYCLES, primitive_mining_cycle_mass};
use super::seed::mix64;
use super::settlement_demand::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, organic_lumber_batches,
};
use super::stationary_survival_start::StationarySurvivalStart;

#[derive(Clone, Copy)]
pub(super) enum PowerProjectEra {
    Primitive,
    Settlement,
}

pub(super) fn power_project_survival_start(
    seed: u64,
    maintained: bool,
    era: PowerProjectEra,
) -> StationarySurvivalStart {
    if maintained {
        return StationarySurvivalStart::FullReserve;
    }

    let stratum = usize::try_from(seed & 0b11)
        .unwrap_or_else(|_| unreachable!("two-bit power stratum fits usize"));
    let primitive = [
        StationarySurvivalStart::FullReserve,
        StationarySurvivalStart::HungerWarningBoundary,
        StationarySurvivalStart::FullReserve,
        StationarySurvivalStart::HydrationWarningBoundary,
    ];
    let settlement = [
        StationarySurvivalStart::HydrationWarningBoundary,
        StationarySurvivalStart::FullReserve,
        StationarySurvivalStart::HungerWarningBoundary,
        StationarySurvivalStart::FullReserve,
    ];
    match era {
        PowerProjectEra::Primitive => primitive[stratum],
        PowerProjectEra::Settlement => settlement[stratum],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrimitiveCrushingWorkload {
    RoutineStockpile,
    BulkFieldwork,
}

pub(super) const ROUTINE_STOCKPILE_MIN_CYCLES: u64 = STOCKPILE_WORK_ORDER_CYCLES * 2 / 3;
pub(super) const ROUTINE_STOCKPILE_MAX_CYCLES: u64 = STOCKPILE_WORK_ORDER_CYCLES * 2;

impl PrimitiveCrushingWorkload {
    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::RoutineStockpile => "routine-stockpile",
            Self::BulkFieldwork => "bulk-fieldwork-ore",
        }
    }
}

pub(super) fn declared_primitive_crushing_project(
    registries: &Registries,
    seed: u64,
    store_definition: EnergyStoreDefinitionId,
) -> (Mass, Energy, PrimitiveCrushingWorkload) {
    let definition = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive power project crusher process disappeared"));
    let store = registries
        .energy()
        .get_store(store_definition)
        .unwrap_or_else(|| panic!("primitive power project accumulator disappeared"));
    let cycle_mass = primitive_mining_cycle_mass(registries, seed);
    let cycle_work = deep_hearth::energy::calculate_mass_specific_energy(
        cycle_mass,
        definition.specific_energy(),
    );
    assert!(
        !cycle_work.is_zero() && cycle_work <= store.capacity(),
        "ordinary primitive mining cycle must fit the current baseline accumulator"
    );

    let stratum = seed & 0b11;
    let workload = if stratum >= 2 {
        PrimitiveCrushingWorkload::BulkFieldwork
    } else {
        PrimitiveCrushingWorkload::RoutineStockpile
    };
    let mass = match workload {
        PrimitiveCrushingWorkload::RoutineStockpile => {
            let midpoint = (ROUTINE_STOCKPILE_MIN_CYCLES + ROUTINE_STOCKPILE_MAX_CYCLES) / 2;
            let (minimum, maximum) = if stratum == 0 {
                (ROUTINE_STOCKPILE_MIN_CYCLES, midpoint)
            } else {
                (midpoint + 1, ROUTINE_STOCKPILE_MAX_CYCLES)
            };
            let cycles = minimum + mix64(seed ^ 0x5052_494D_5F4F_5245) % (maximum - minimum + 1);
            Mass::from_milligrams(
                cycle_mass
                    .milligrams()
                    .checked_mul(cycles)
                    .unwrap_or_else(|| panic!("primitive power project mass overflowed")),
            )
        }
        PrimitiveCrushingWorkload::BulkFieldwork => {
            let quarry_batch = primitive_quarry_batch_mass(registries);
            let ordinary = bulk_fieldwork_order_mass(registries, seed);
            let ordinary_batches = ordinary.milligrams() / quarry_batch.milligrams();
            let ordinal = ordinary_batches - BULK_FIELDWORK_ORDER_MIN_BATCHES;
            let midpoint =
                (BULK_FIELDWORK_ORDER_MIN_BATCHES + BULK_FIELDWORK_ORDER_MAX_BATCHES) / 2;
            let (minimum, maximum) = if stratum == 2 {
                (BULK_FIELDWORK_ORDER_MIN_BATCHES, midpoint)
            } else {
                (midpoint + 1, BULK_FIELDWORK_ORDER_MAX_BATCHES)
            };
            let batches = minimum + ordinal % (maximum - minimum + 1);
            Mass::from_milligrams(
                quarry_batch
                    .milligrams()
                    .checked_mul(batches)
                    .unwrap_or_else(|| panic!("primitive power bulk project mass overflowed")),
            )
        }
    };
    (
        mass,
        deep_hearth::energy::calculate_mass_specific_energy(mass, definition.specific_energy()),
        workload,
    )
}

pub(super) fn declared_settlement_lumber_project(
    registries: &Registries,
    seed: u64,
) -> (Mass, Energy) {
    let definition = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement power project saw process disappeared"));
    let batch_mass = registries
        .crafting()
        .get_manual(definition.transform())
        .map(|transform| transform.input_mass())
        .unwrap_or_else(|| panic!("settlement power project manual saw transform disappeared"));
    let batches = organic_lumber_batches(
        seed & 0b11,
        mix64(seed ^ 0x5345_5454_4C55_4D42),
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
    );
    let mass = Mass::from_milligrams(
        batch_mass
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("settlement power project mass overflowed")),
    );
    (
        mass,
        deep_hearth::energy::calculate_mass_specific_energy(mass, definition.specific_energy()),
    )
}

pub(super) fn primitive_accumulator_for_current_crusher(
    registries: &Registries,
) -> EnergyStoreDefinitionId {
    let process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive power project crusher process disappeared"));
    let maximum_batch = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_CRUSHER,
        process.max_batch_mass_capability(),
    );
    let required = deep_hearth::energy::calculate_mass_specific_energy(
        maximum_batch,
        process.specific_energy(),
    );

    let candidates = registries
        .energy()
        .definitions()
        .filter(|definition| {
            definition.carrier() == process.energy_carrier()
                && definition.capacity() >= required
                && !definition.max_input_power().is_zero()
                && !definition.max_output_power().is_zero()
                && definition.assembly_profile().is_some_and(|assembly| {
                    assembly.inputs().iter().all(|input| {
                        matches!(input.commodity().material(), MATERIAL_STONE | MATERIAL_WOOD)
                    })
                })
        })
        .map(|definition| {
            let key = (
                definition.capacity().nanojoules(),
                definition
                    .assembly_profile()
                    .map(|assembly| assembly.input_mass().milligrams())
                    .unwrap_or(u64::MAX),
            );
            (definition.id(), key)
        })
        .collect::<Vec<_>>();
    let best_key = candidates
        .iter()
        .map(|(_, key)| *key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "no ordinary copper-free mechanical accumulator can fund one pristine crusher batch of {}mg requiring {}nJ",
                maximum_batch.milligrams(),
                required.nanojoules(),
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, key)| *key == best_key)
        .map(|(definition, _)| definition);
    let selected = best
        .next()
        .unwrap_or_else(|| unreachable!("best accumulator key came from one candidate"));
    assert!(
        best.next().is_none(),
        "ordinary copper-free accumulators are physically tied at the actor's minimum capacity/material key; author an observable preference instead of using definition identity"
    );
    selected
}
