//! Deterministic scenario-input generation for the maintained workshop matrix.

use deep_hearth::content::{
    EQUIPMENT_JAW_CRUSHER, MATERIAL_WOOD, PROCESS_CRUSH_ORE, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
};
use deep_hearth::core::quantity::{Area, Force, Mass};
use deep_hearth::maintenance::{CONDITION_PARTS_PER_MILLION, Condition};
use deep_hearth::registry::Registries;
use deep_hearth::structural::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralLoadMode, calculate_weight_force_ceiling,
};

use super::report::{
    MaintenancePreference, PowerPreference, ScenarioPolicyVariation, StructuralPreference,
};
use super::seed::mix64;
use super::{condition, nominal_equipment_mass_capability};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioVariation {
    pub(super) world_seed: u64,
    pub(super) behavior_seed: u64,
    pub(super) ore: ScenarioOreVariation,
    pub(super) crusher: ScenarioCrusherVariation,
    pub(super) structure: ScenarioStructureVariation,
    pub(super) delivery: ScenarioDeliveryVariation,
    pub(super) policy: ScenarioPolicyVariation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioOreVariation {
    pub(super) ore_copper_ppm: u32,
    pub(super) batch_mass: Mass,
    pub(super) planned_batches: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioCrusherVariation {
    pub(super) initial_crusher_condition: Condition,
    pub(super) small_drive_batch_budget: u8,
    pub(super) large_drive_batch_budget: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioStructureVariation {
    pub(super) compact_support_area: Area,
    pub(super) reinforced_support_area: Area,
    pub(super) reinforced_background_mass: Mass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioDeliveryVariation {
    pub(super) mass: Mass,
    pub(super) destination_is_compact: bool,
    pub(super) delivery_at_tick: u64,
    pub(super) force_power_deadline: bool,
}

impl ScenarioVariation {
    pub(super) fn from_seeds(
        registries: &Registries,
        world_seed: u64,
        behavior_seed: u64,
        anchor_index: Option<usize>,
    ) -> Self {
        let a = mix64(world_seed);
        let b = mix64(a);
        let c = mix64(b);
        let d = mix64(c);
        let e = mix64(d);
        let f = mix64(e);
        let g = mix64(f);
        let h = mix64(g);
        let i = mix64(h);
        let crusher_definition = registries
            .equipment()
            .get_equipment(EQUIPMENT_JAW_CRUSHER)
            .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
        let crusher_process = registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .unwrap_or_else(|| panic!("canonical crusher process definition disappeared"));
        let maximum_batch = nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            crusher_process.max_batch_mass_capability(),
        )
        .milligrams();
        assert!(
            maximum_batch > 0,
            "canonical crusher batch limit must be nonzero"
        );
        let minimum_batch = maximum_batch.div_ceil(2);
        let batch_mass = minimum_batch + c % (maximum_batch - minimum_batch + 1);
        let planned_batches = 4 + (a % 3) as u8;

        let thresholds = crusher_definition.maintenance_thresholds();
        let initial_condition = match anchor_index.map(|index| index % 3) {
            Some(0) => {
                let warning = thresholds.warning_below().parts_per_million();
                warning + (CONDITION_PARTS_PER_MILLION - warning).div_ceil(2)
            }
            Some(1) => {
                let critical = thresholds.critical_below().parts_per_million();
                let warning = thresholds.warning_below().parts_per_million();
                critical + (warning - critical).div_ceil(2)
            }
            Some(2) => thresholds
                .critical_below()
                .parts_per_million()
                .div_ceil(2)
                .max(1),
            None => 1 + (e % u64::from(CONDITION_PARTS_PER_MILLION)) as u32,
            Some(_) => unreachable!("anchor condition modulo is exhaustive"),
        };
        let required_large_batches = 1 + (h % 2) as u8;
        let small_drive_batch_budget = planned_batches - required_large_batches;
        let large_drive_batch_budget = required_large_batches + ((h >> 1) & 1) as u8;

        let crusher_weight =
            calculate_weight_force_ceiling(crusher_definition.mass(), registries.core().gravity());
        let target_low = 350_000_u32;
        let target_high = 750_000_u32;
        let compact_target_ppm = target_low + (b % u64::from(target_high - target_low + 1)) as u32;
        let compact_area =
            support_area_for_utilization(registries, crusher_weight, compact_target_ppm);
        let reinforced_background_mass =
            scale_mass(crusher_definition.mass(), 100_000 + (f % 900_001) as u32);
        let reinforced_loaded_mass = crusher_definition
            .mass()
            .checked_add(reinforced_background_mass)
            .unwrap_or_else(|| panic!("gameplay harness reinforced support mass overflowed"));
        let reinforced_target_ppm =
            target_low + (d % u64::from(target_high - target_low + 1)) as u32;
        let reinforced_area = support_area_for_utilization(
            registries,
            calculate_weight_force_ceiling(reinforced_loaded_mass, registries.core().gravity()),
            reinforced_target_ppm,
        );
        let delivery_mass = scale_mass(crusher_definition.mass(), 150_000 + (g % 850_001) as u32);
        let behavior_a = mix64(behavior_seed);
        let behavior_b = mix64(behavior_a);
        let behavior_c = mix64(behavior_b);
        let power_preference = match behavior_a % 3 {
            0 => PowerPreference::PreserveReserve,
            1 => PowerPreference::ProtectCondition,
            2 => PowerPreference::FinishSooner,
            _ => unreachable!("modulo three must be exhaustive"),
        };
        let maintenance_preference = if behavior_b.is_multiple_of(2) {
            MaintenancePreference::ServiceAtWarning
        } else {
            MaintenancePreference::ServiceAtCritical
        };
        let structural_preference = if behavior_c.is_multiple_of(2) {
            StructuralPreference::PreserveMargin
        } else {
            StructuralPreference::MoveOnlyForFailure
        };
        Self {
            world_seed,
            behavior_seed,
            ore: ScenarioOreVariation {
                ore_copper_ppm: 450_000 + (b % 300_001) as u32,
                batch_mass: Mass::from_milligrams(batch_mass),
                planned_batches,
            },
            crusher: ScenarioCrusherVariation {
                initial_crusher_condition: condition(initial_condition),
                small_drive_batch_budget,
                large_drive_batch_budget,
            },
            structure: ScenarioStructureVariation {
                compact_support_area: compact_area,
                reinforced_support_area: reinforced_area,
                reinforced_background_mass,
            },
            delivery: ScenarioDeliveryVariation {
                mass: delivery_mass,
                destination_is_compact: i.is_multiple_of(2),
                delivery_at_tick: 0,
                force_power_deadline: anchor_index == Some(0),
            },
            policy: ScenarioPolicyVariation {
                power_preference,
                maintenance_preference,
                structural_preference,
            },
        }
    }
}

fn divide_ceiling(numerator: u128, denominator: u128) -> u128 {
    assert!(denominator > 0, "gameplay harness divisor must be nonzero");
    if numerator == 0 {
        0
    } else {
        1 + (numerator - 1) / denominator
    }
}

fn support_area_for_utilization(
    registries: &Registries,
    carried_load: Force,
    target_utilization_ppm: u32,
) -> Area {
    assert!(target_utilization_ppm > 0);
    let profile = registries
        .structural()
        .get_profile(STRUCTURAL_PROFILE_AXIAL_COMPRESSION)
        .unwrap_or_else(|| panic!("canonical compression profile disappeared"));
    let material = registries
        .materials()
        .get_material(MATERIAL_WOOD)
        .unwrap_or_else(|| panic!("canonical wood material disappeared"));
    let mechanical = material.properties().mechanical();
    let strength_kpa = match profile.load_mode() {
        StructuralLoadMode::Compression => mechanical.compressive_strength_kpa(),
        StructuralLoadMode::Tension => mechanical.tensile_strength_kpa(),
    };
    assert!(
        strength_kpa > 0,
        "canonical support material must have nonzero strength"
    );
    let required_capacity = divide_ceiling(
        carried_load
            .millinewtons()
            .checked_mul(u128::from(STRUCTURAL_PARTS_PER_MILLION))
            .unwrap_or_else(|| panic!("gameplay harness support-capacity scaling overflowed")),
        u128::from(target_utilization_ppm),
    );
    let area = divide_ceiling(required_capacity, u128::from(strength_kpa));
    let area = u64::try_from(area).unwrap_or_else(|_| {
        panic!("gameplay harness support area exceeds authored quantity range")
    });
    Area::from_square_millimeters(area.max(1))
}

fn scale_mass(mass: Mass, parts_per_million: u32) -> Mass {
    let scaled = u128::from(mass.milligrams()) * u128::from(parts_per_million)
        / u128::from(STRUCTURAL_PARTS_PER_MILLION);
    let scaled = u64::try_from(scaled)
        .unwrap_or_else(|_| panic!("gameplay harness mass scaling overflowed"));
    Mass::from_milligrams(scaled.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deep_hearth::content::build_registries;

    #[test]
    fn behavior_seed_never_changes_physical_scenario_inputs() {
        let registries = build_registries();
        let first = ScenarioVariation::from_seeds(&registries, 0x1234, 0x1111, None);
        let second = ScenarioVariation::from_seeds(&registries, 0x1234, 0x2222, None);

        assert_eq!(first.world_seed, second.world_seed);
        assert_eq!(first.ore, second.ore);
        assert_eq!(first.crusher, second.crusher);
        assert_eq!(first.structure, second.structure);
        assert_eq!(first.delivery, second.delivery);
    }

    #[test]
    fn world_seed_never_changes_player_policy() {
        let registries = build_registries();
        let first = ScenarioVariation::from_seeds(&registries, 0x1234, 0xCAFE, None);
        let second = ScenarioVariation::from_seeds(&registries, 0x5678, 0xCAFE, None);

        assert_eq!(first.behavior_seed, second.behavior_seed);
        assert_eq!(first.policy, second.policy);
    }
}
