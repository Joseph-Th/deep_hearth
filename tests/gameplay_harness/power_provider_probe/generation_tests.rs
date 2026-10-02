//! Content-relative variation contracts for ordinary power-provider workloads.

use std::collections::BTreeSet;

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};

use super::*;

#[test]
fn organic_power_workload_sampling_straddles_the_current_crossover_scale() {
    let registries = deep_hearth::content::build_registries();
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive crusher process disappeared"));
    let saw = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement saw process disappeared"));
    let bank = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .unwrap_or_else(|| panic!("settlement flywheel bank disappeared"));
    let saw_mass_per_bank = deep_hearth::energy::calculate_mass_specific_energy_capacity(
        bank.capacity(),
        saw.specific_energy(),
    );

    let store_definition = primitive_accumulator_for_current_crusher(&registries);
    let primitive_store = registries
        .energy()
        .get_store(store_definition)
        .unwrap_or_else(|| panic!("primitive accumulator disappeared"));
    let primitive_mass_per_charge = deep_hearth::energy::calculate_mass_specific_energy_capacity(
        primitive_store.capacity(),
        crusher.specific_energy(),
    );
    let primitive = (1_u64..=64)
        .map(|seed| {
            declared_primitive_crushing_project(&registries, seed, store_definition, Some(8)).0
        })
        .collect::<Vec<_>>();
    let settlement = (1_u64..=64)
        .map(|seed| declared_settlement_lumber_project(&registries, seed, Some(8)).0)
        .collect::<Vec<_>>();

    assert!(
        primitive.iter().all(|mass| mass
            .milligrams()
            .is_multiple_of(primitive_mass_per_charge.milligrams())),
        "primitive organic projects must remain whole current accumulator workloads"
    );
    assert!(
        settlement.iter().all(|mass| mass
            .milligrams()
            .is_multiple_of(saw_mass_per_bank.milligrams())),
        "settlement organic projects must remain whole current flywheel-backed saw workloads"
    );
    assert!(
        primitive
            .iter()
            .map(|mass| mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "fresh primitive roots must vary declared productive work"
    );
    assert!(
        settlement
            .iter()
            .map(|mass| mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "fresh settlement roots must vary declared productive work"
    );
    let primitive_units = primitive
        .iter()
        .map(|mass| mass.milligrams() / primitive_mass_per_charge.milligrams())
        .collect::<BTreeSet<_>>();
    let settlement_units = settlement
        .iter()
        .map(|mass| mass.milligrams() / saw_mass_per_bank.milligrams())
        .collect::<BTreeSet<_>>();
    assert!(primitive_units.iter().any(|units| *units < 8));
    assert!(primitive_units.iter().any(|units| *units >= 8));
    assert!(settlement_units.iter().any(|units| *units < 8));
    assert!(settlement_units.iter().any(|units| *units >= 8));

    let no_primitive_crossover = (1_u64..=64)
        .map(|seed| {
            declared_primitive_crushing_project(&registries, seed, store_definition, None).0
        })
        .map(|mass| mass.milligrams() / primitive_mass_per_charge.milligrams())
        .collect::<BTreeSet<_>>();
    let no_settlement_crossover = (1_u64..=64)
        .map(|seed| declared_settlement_lumber_project(&registries, seed, None).0)
        .map(|mass| mass.milligrams() / saw_mass_per_bank.milligrams())
        .collect::<BTreeSet<_>>();
    assert!(no_primitive_crossover.len() > 1);
    assert!(no_settlement_crossover.len() > 1);
}
