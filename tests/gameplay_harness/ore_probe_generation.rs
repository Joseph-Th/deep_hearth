//! Deterministic physical setup generation for the ore-preparation capability probe.

use super::*;

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
    // Stored work is scenario pressure, not a forecast of future process requirements. Vary it as
    // a fraction of the authored store capacity so content rebalance does not silently collapse the
    // organic range into always-full or always-empty cases. Canonical resolvers still expose the
    // first real finite-work blocker for the unchanged generated batch.
    let fill_ppm = 150_000_u32
        + u32::try_from(mix64(seed ^ 0x454E_4552_4759_4845) % 800_001)
            .unwrap_or_else(|_| unreachable!("bounded ore drive fill ratio fits u32"));
    let varied_budget_nj = drive_capacity
        .nanojoules()
        .checked_mul(u128::from(fill_ppm))
        .map(|value| value / 1_000_000)
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
