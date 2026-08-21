//! Deterministic scenario-input generation for the maintained workshop matrix.

use deep_hearth::content::{
    EQUIPMENT_JAW_CRUSHER, MATERIAL_WOOD, PROCESS_CRUSH_ORE, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
};
use deep_hearth::core::quantity::{Area, Force, Mass};
use deep_hearth::maintenance::{CONDITION_PARTS_PER_MILLION, Condition, MaintenanceBand};
use deep_hearth::registry::Registries;
use deep_hearth::structural::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralLoadMode, calculate_weight_force_ceiling,
};

use super::condition;
use super::configuration::MaintainedAnchor;
use super::report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioPolicyVariation,
    StructuralPreference,
};
use super::seed::mix64;
use super::support::nominal_equipment_mass_capability;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioVariation {
    pub(super) world_seed: u64,
    pub(super) behavior_seed: u64,
    pub(super) survival: ScenarioSurvivalVariation,
    pub(super) ore: ScenarioOreVariation,
    pub(super) crusher: ScenarioCrusherVariation,
    pub(super) structure: ScenarioStructureVariation,
    pub(super) delivery: ScenarioDeliveryVariation,
    pub(super) policy: ScenarioPolicyVariation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSurvivalVariation {
    pub(super) start_at_hydration_warning: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioOreVariation {
    pub(super) ore_copper_ppm: u32,
    pub(super) nominal_batch_mass: Mass,
    pub(super) order_mass: Mass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioCrusherVariation {
    pub(super) initial_crusher_condition: Condition,
    pub(super) small_drive_batch_budget: u8,
    pub(super) small_drive_partial_batch_ppm: u32,
    pub(super) large_drive_batch_budget: u8,
    pub(super) large_drive_partial_batch_ppm: u32,
    pub(super) maintenance_replacement_units: u8,
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
}

fn ensure_generated_critical_recovery_supply(
    maintenance_band: MaintenanceBand,
    replacement_units: u8,
) -> u8 {
    if maintenance_band == MaintenanceBand::Critical {
        replacement_units.max(1)
    } else {
        replacement_units
    }
}

impl ScenarioVariation {
    pub(super) fn from_seeds(
        registries: &Registries,
        world_seed: u64,
        behavior_seed: u64,
        anchor: Option<MaintainedAnchor>,
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
        let j = mix64(i);
        let k = mix64(j);
        let l = mix64(k);
        let m = mix64(l);
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
        let nominal_batch_mass = minimum_batch + c % (maximum_batch - minimum_batch + 1);
        let nominal_batch_count = 4 + (a % 4) as u8;
        let order_mass = nominal_batch_mass
            .checked_mul(u64::from(nominal_batch_count))
            .unwrap_or_else(|| panic!("gameplay harness work-order mass overflowed"));

        let thresholds = crusher_definition.maintenance_thresholds();
        let initial_condition = match anchor {
            Some(MaintainedAnchor::NormalBaseline | MaintainedAnchor::AdaptiveEnergy) => {
                let warning = thresholds.warning_below().parts_per_million();
                warning + (CONDITION_PARTS_PER_MILLION - warning).div_ceil(2)
            }
            Some(MaintainedAnchor::SurvivalRecovery) => {
                let warning = thresholds.warning_below().parts_per_million();
                warning + (CONDITION_PARTS_PER_MILLION - warning) * 3 / 4
            }
            Some(MaintainedAnchor::WarningMaintenance | MaintainedAnchor::ManualRecovery) => {
                let critical = thresholds.critical_below().parts_per_million();
                let warning = thresholds.warning_below().parts_per_million();
                critical + (warning - critical).div_ceil(2)
            }
            Some(MaintainedAnchor::CriticalMaintenance) => thresholds
                .critical_below()
                .parts_per_million()
                .div_ceil(2)
                .max(1),
            Some(MaintainedAnchor::ConditionPressure) => {
                let critical = thresholds.critical_below().parts_per_million();
                let warning = thresholds.warning_below().parts_per_million();
                assert!(
                    warning > critical,
                    "condition-pressure anchor requires a nonempty warning condition band"
                );
                critical + (warning - critical).min(1_000)
            }
            None => 1 + (e % u64::from(CONDITION_PARTS_PER_MILLION)) as u32,
        };
        let initial_crusher_condition = condition(initial_condition);
        let (
            small_drive_batch_budget,
            small_drive_partial_batch_ppm,
            large_drive_batch_budget,
            large_drive_partial_batch_ppm,
            mut maintenance_replacement_units,
        ) = match anchor {
            Some(MaintainedAnchor::AdaptiveEnergy) => {
                (nominal_batch_count.saturating_sub(1), 450_000, 0, 0, 1)
            }
            Some(MaintainedAnchor::ManualRecovery) => {
                (nominal_batch_count.saturating_sub(2), 0, 0, 0, 1)
            }
            Some(MaintainedAnchor::SurvivalRecovery) => (
                nominal_batch_count.saturating_sub(5),
                250_000,
                0,
                250_000,
                0,
            ),
            Some(MaintainedAnchor::ConditionPressure) => (nominal_batch_count, 0, 2, 0, 1),
            Some(
                MaintainedAnchor::NormalBaseline
                | MaintainedAnchor::WarningMaintenance
                | MaintainedAnchor::CriticalMaintenance,
            ) => (nominal_batch_count, 0, 1 + (h % 2) as u8, 0, 1),
            None => (
                (h % (u64::from(nominal_batch_count) + 1)) as u8,
                100_000 + (l % 800_001) as u32,
                (j % 3) as u8,
                if m.is_multiple_of(3) {
                    0
                } else {
                    100_000 + (m % 800_001) as u32
                },
                (k % 3) as u8,
            ),
        };
        if anchor.is_none() {
            maintenance_replacement_units = ensure_generated_critical_recovery_supply(
                thresholds.classify(initial_crusher_condition),
                maintenance_replacement_units,
            );
        }

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
        let delivery_mass = scale_mass(crusher_definition.mass(), 500_000 + (g % 2_500_001) as u32);
        let behavior_a = mix64(behavior_seed);
        let behavior_b = mix64(behavior_a);
        let behavior_c = mix64(behavior_b);
        let behavior_d = mix64(behavior_c);
        let power_preference = match behavior_a % 2 {
            0 => PowerPreference::PreserveReserve,
            1 => PowerPreference::FinishSooner,
            _ => unreachable!("modulo two must be exhaustive"),
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
        let energy_recovery_preference = if behavior_d.is_multiple_of(2) {
            EnergyRecoveryPreference::ProtectSurvival
        } else {
            EnergyRecoveryPreference::SpendSurvivalReserve
        };
        Self {
            world_seed,
            behavior_seed,
            survival: ScenarioSurvivalVariation {
                start_at_hydration_warning: anchor == Some(MaintainedAnchor::SurvivalRecovery),
            },
            ore: ScenarioOreVariation {
                ore_copper_ppm: 450_000 + (b % 300_001) as u32,
                nominal_batch_mass: Mass::from_milligrams(nominal_batch_mass),
                order_mass: Mass::from_milligrams(order_mass),
            },
            crusher: ScenarioCrusherVariation {
                initial_crusher_condition,
                small_drive_batch_budget,
                small_drive_partial_batch_ppm,
                large_drive_batch_budget,
                large_drive_partial_batch_ppm,
                maintenance_replacement_units,
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
            },
            policy: ScenarioPolicyVariation {
                power_preference,
                energy_recovery_preference,
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

    #[test]
    fn generated_critical_start_cannot_have_empty_recovery_supply() {
        assert_eq!(
            ensure_generated_critical_recovery_supply(MaintenanceBand::Critical, 0),
            1
        );
        assert_eq!(
            ensure_generated_critical_recovery_supply(MaintenanceBand::Critical, 2),
            2
        );
        assert_eq!(
            ensure_generated_critical_recovery_supply(MaintenanceBand::Normal, 0),
            0
        );
    }
}
