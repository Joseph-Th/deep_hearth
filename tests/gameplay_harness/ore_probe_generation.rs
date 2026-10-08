//! Deterministic physical setup generation for the ore-preparation capability probe.

use super::super::ore_probe_parameters::probe_parameters;
use super::*;

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
    let (state, ids) = setup_ore_preparation_probe(registries, setup);
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
