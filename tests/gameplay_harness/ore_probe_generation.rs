//! Deterministic physical setup generation for the ore-preparation capability probe.

use super::*;

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

pub(crate) fn probe_parameters(registries: &Registries, seed: u64) -> OrePreparationSetup {
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
    let aperture = screening.aperture();
    let mut undersize_weight = 0_u64;
    for class in distribution.classes() {
        let range = class.range();
        if range.maximum_diameter() <= aperture {
            undersize_weight += u64::from(class.weight());
        } else if range.minimum_diameter() <= aperture {
            panic!(
                "authored grinder particle class {}..={}um crosses screen aperture {}um",
                range.minimum_diameter().micrometers(),
                range.maximum_diameter().micrometers(),
                aperture.micrometers()
            );
        }
    }
    let total_weight = distribution.total_weight();
    let representable_unit = total_weight / greatest_common_divisor(total_weight, undersize_weight);

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
    if undersize_weight < total_weight {
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
    let maximum_units = maximum_batch / representable_unit;
    assert!(
        maximum_units > 0,
        "authored screen partition cannot be represented within the equipment batch limits"
    );
    let minimum_units = maximum_units.div_ceil(2);
    let unit_count =
        minimum_units + mix64(seed ^ 0x0AE5_1A5E) % (maximum_units - minimum_units + 1);
    let batch_mass = Mass::from_milligrams(representable_unit * unit_count);
    let copper_ppm = 300_000 + (mix64(seed ^ 0xC0FF_EE11) % 400_001) as u32;
    let clay_share_ppm = 100_000 + (mix64(seed ^ 0x4741_4E47_5545_4D49) % 500_001) as u32;
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("ore preparation drive definition disappeared"));
    // Stored work is scenario pressure, not a forecast of future process requirements. The probe
    // attempts the generated batch unchanged and lets each canonical resolver expose the first
    // real finite-work blocker. This keeps capability evaluation from duplicating chain physics.
    let varied_budget = Energy::from_nanojoules(
        20_000_000_000_000_u128
            + u128::from(mix64(seed ^ 0x454E_4552_4759_4845) % 160_000_000_000_001),
    );
    let drive_energy = std::cmp::min(varied_budget, drive_capacity);
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

pub(super) struct OreProbeEpisode {
    pub(super) case: FocusedProbeCase,
    pub(super) state: AppState,
    pub(super) ids: OrePreparationProbeIds,
    pub(super) batch_mass: Mass,
    pub(super) initial_matter: AggregateMass,
    pub(super) initial_energy: Energy,
    pub(super) initial_crusher_condition: Condition,
    pub(super) initial_grinder_condition: Condition,
    pub(super) initial_screen_condition: Condition,
    pub(super) initial_separator_condition: Condition,
    pub(super) input_composition: MaterialComposition,
    pub(super) input_copper_ppm: u32,
    pub(super) input_stone_ppm: u32,
    pub(super) input_clay_ppm: u32,
}

pub(super) fn prepare_ore_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) -> OreProbeEpisode {
    let seed = case.seed();
    let setup = probe_parameters(registries, seed);
    let batch_mass = setup.batch_mass;
    let initial_crusher_condition = setup.crusher_condition;
    let initial_grinder_condition = setup.grinder_condition;
    let initial_screen_condition = setup.screen_condition;
    let (state, ids) = setup_ore_preparation_probe(registries, seed, setup);
    let initial_separator_condition = state
        .equipment()
        .get_equipment(ids.separator)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation assembled separator disappeared"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("ore preparation initial matter accounting failed: {error}"))
        .total();
    let initial_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared"));
    let input_composition = state
        .inventory()
        .get_lot(ids.ore_lot)
        .unwrap_or_else(|| panic!("ore preparation input lot disappeared after setup"))
        .composition()
        .clone();
    let input_copper_ppm = input_composition.parts_per_million(MATERIAL_COPPER);
    let input_stone_ppm = input_composition.parts_per_million(MATERIAL_STONE);
    let input_clay_ppm = input_composition.parts_per_million(MATERIAL_CLAY);

    OreProbeEpisode {
        case,
        state,
        ids,
        batch_mass,
        initial_matter,
        initial_energy,
        initial_crusher_condition,
        initial_grinder_condition,
        initial_screen_condition,
        initial_separator_condition,
        input_composition,
        input_copper_ppm,
        input_stone_ppm,
        input_clay_ppm,
    }
}
