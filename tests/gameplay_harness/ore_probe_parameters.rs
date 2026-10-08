//! Deterministic physical parameters for the ore-preparation capability probe.

use deep_hearth::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, EQUIPMENT_DRY_SCREEN, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::COMPOSITION_PARTS_PER_MILLION;
use deep_hearth::ore_processing::resolve_representable_screening_mass;
use deep_hearth::registry::Registries;

use super::equipment_support::nominal_equipment_mass_capability;
use super::production_support::varied_healthy_condition;
use super::seed::mix64;

#[derive(Clone, Copy)]
pub(super) struct OrePreparationSetup {
    pub(super) batch_mass: Mass,
    pub(super) copper_ppm: u32,
    pub(super) clay_share_ppm: u32,
    pub(super) crusher_condition: Condition,
    pub(super) grinder_condition: Condition,
    pub(super) screen_condition: Condition,
    pub(super) separator_condition: Condition,
    pub(super) drive_energy: Energy,
}

pub(super) fn probe_parameters(registries: &Registries, seed: u64) -> OrePreparationSetup {
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let grinder = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical grinder definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical screen definition disappeared"));
    let fine_grind = registries
        .ore_processing()
        .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
        .unwrap_or_else(|| panic!("canonical fine-grind definition disappeared"));
    let distribution = grinder.output_particle_size_distribution();
    let mut batch_limits = vec![
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            crusher.max_batch_mass_capability(),
        ),
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_GRINDING_MILL,
            grinder.max_batch_mass_capability(),
        ),
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_DRY_SCREEN,
            screening.max_batch_mass_capability(),
        ),
    ];
    if distribution
        .classes()
        .iter()
        .any(|class| class.range().minimum_diameter() > screening.aperture())
    {
        batch_limits.push(nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_GRINDING_MILL,
            fine_grind.max_batch_mass_capability(),
        ));
    }
    let maximum_batch = batch_limits
        .into_iter()
        .map(Mass::milligrams)
        .min()
        .unwrap_or_else(|| panic!("ore preparation probe has no authored batch constraints"));
    let minimum_batch = maximum_batch.div_ceil(2).max(1);
    let requested_batch = Mass::from_milligrams(
        minimum_batch + mix64(seed ^ 0x0AE5_1A5E) % (maximum_batch - minimum_batch + 1),
    );
    let batch_mass = resolve_representable_screening_mass(screening, distribution, requested_batch)
        .unwrap_or_else(|error| {
            panic!("ore preparation canonical screening batch projection failed: {error}")
        });
    assert!(
        !batch_mass.is_zero(),
        "authored screen partition has no representable nonzero batch within generated equipment limits"
    );
    let copper_ppm = 300_000 + (mix64(seed ^ 0xC0FF_EE11) % 400_001) as u32;
    let clay_share_ppm = 100_000 + (mix64(seed ^ 0x4741_4E47_5545_4D49) % 500_001) as u32;
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("ore preparation drive definition disappeared"));
    let fill_ppm = 150_000_u32
        + u32::try_from(mix64(seed ^ 0x454E_4552_4759_4845) % 800_001)
            .unwrap_or_else(|_| unreachable!("bounded ore drive fill ratio fits u32"));
    let varied_budget_nj = drive_capacity
        .nanojoules()
        .checked_mul(u128::from(fill_ppm))
        .map(|value| value / u128::from(COMPOSITION_PARTS_PER_MILLION))
        .unwrap_or_else(|| panic!("ore preparation drive-fill projection overflowed"));
    let drive_energy = Energy::from_nanojoules(varied_budget_nj.max(1));
    OrePreparationSetup {
        batch_mass,
        copper_ppm,
        clay_share_ppm,
        crusher_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            mix64(seed ^ 0x4352_5553_4843_4F4E),
        ),
        grinder_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_GRINDING_MILL,
            mix64(seed ^ 0x4752_494E_4443_4F4E),
        ),
        screen_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_DRY_SCREEN,
            mix64(seed ^ 0x5343_5245_454E_434F),
        ),
        separator_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_GRAVITY_SEPARATOR,
            mix64(seed ^ 0x5345_5041_5241_544F),
        ),
        drive_energy,
    }
}
