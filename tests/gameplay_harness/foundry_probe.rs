//! Focused pure-copper melt/cast capability probe.

use super::foundry_setup::setup_foundry_probe;
use super::seed::mix64;
use super::support::nominal_equipment_mass_capability;
use deep_hearth::content::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_ELECTRIC_FURNACE, PROCESS_CAST_PURE_COPPER,
    PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::TickSpan;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::thermal::{
    CastingRequest, MeltingRequest, resolve_casting_process, resolve_melting_process,
};

fn finish_operation(registries: &Registries, state: &mut AppState, duration: TickSpan) {
    for _ in 0..duration.value() {
        advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("foundry probe tick failed: {error}"));
    }
}

fn stockpile_first_lot(
    state: &AppState,
    stockpile: StockpileId,
) -> deep_hearth::inventory::MaterialLotId {
    state
        .inventory()
        .lot_ids(stockpile)
        .next()
        .unwrap_or_else(|| panic!("foundry probe expected output lot is missing"))
}

fn probe_mass(registries: &Registries, seed: u64) -> Mass {
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical melting definition disappeared"));
    let casting = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical casting definition disappeared"));
    let melt_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_ELECTRIC_FURNACE,
        melting.max_batch_mass_capability(),
    );
    let cast_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_CASTING_MOLD,
        casting.max_batch_mass_capability(),
    );
    let maximum = melt_maximum.milligrams().min(cast_maximum.milligrams());
    assert!(maximum > 0, "foundry probe requires a nonzero legal batch");
    let minimum = maximum.div_ceil(2);
    Mass::from_milligrams(minimum + mix64(seed ^ 0xF0A1_DA7A) % (maximum - minimum + 1))
}

pub(super) fn run_foundry_capability_probe(registries: &Registries, seed: u64) {
    let mass = probe_mass(registries, seed);
    let (mut state, ids) = setup_foundry_probe(registries, seed, mass);
    let pure_selection = [MaterialLotSelection::new(ids.pure_copper_lot, mass)];
    let melt = resolve_melting_process(
        registries,
        &state,
        MeltingRequest::new(
            PROCESS_MELT_PURE_COPPER,
            ids.pure_copper_source,
            &pure_selection,
            ids.furnace,
            ids.electrical_buffer,
        ),
    )
    .unwrap_or_else(|error| panic!("foundry probe pure-copper melt failed: {error}"));
    let melt_duration = melt.process_resolution().duration();
    validate_start_process(
        registries,
        &state,
        melt.process_resolution(),
        ids.pure_copper_source,
        ids.molten_vessel,
    )
    .unwrap_or_else(|error| panic!("foundry probe melt start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe melt commit failed: {error}"));
    finish_operation(registries, &mut state, melt_duration);

    let molten_lot = stockpile_first_lot(&state, ids.molten_vessel);
    let molten_selection = [MaterialLotSelection::new(molten_lot, mass)];
    let casting = resolve_casting_process(
        registries,
        &state,
        CastingRequest::new(
            PROCESS_CAST_PURE_COPPER,
            ids.molten_vessel,
            &molten_selection,
            ids.mold,
            ids.heat_sink,
        ),
    )
    .unwrap_or_else(|error| panic!("foundry probe pure-copper casting failed: {error}"));
    let cast_duration = casting.process_resolution().duration();
    validate_start_process(
        registries,
        &state,
        casting.process_resolution(),
        ids.molten_vessel,
        ids.cast_storage,
    )
    .unwrap_or_else(|error| panic!("foundry probe casting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe casting commit failed: {error}"));
    finish_operation(registries, &mut state, cast_duration);

    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("foundry probe final state audit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.cast_storage)
            .map(|stockpile| stockpile.stored_mass()),
        Some(mass),
        "foundry capability probe must conserve cast output mass"
    );
    std::println!(
        "CAPABILITY FOUNDRY seed=0x{seed:016X} reachability=bootstrapped-industrial batch={}mg melt={}t cast={}t matter=conserved",
        mass.milligrams(),
        melt_duration.value(),
        cast_duration.value(),
    );
}
