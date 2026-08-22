//! Focused pure-copper melt/cast capability probe.

use super::foundry_setup::{FoundrySetup, setup_foundry_probe};
use super::production_support::{
    finish_production_job, select_stockpile_mass, varied_healthy_condition,
};
use super::seed::mix64;
use super::support::{ROOM_TEMPERATURE, nominal_equipment_mass_capability};
use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, EQUIPMENT_CASTING_MOLD, EQUIPMENT_ELECTRIC_FURNACE, MATERIAL_COPPER,
    PROCESS_CAST_PURE_COPPER, PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass, Temperature};
use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::material::MaterialComposition;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::thermal::{
    CastingRequest, MeltingRequest, calculate_fusion_heat, calculate_sensible_heat,
    resolve_casting_process, resolve_melting_process,
};

fn probe_setup(registries: &Registries, seed: u64) -> FoundrySetup {
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
    let mass = Mass::from_milligrams(minimum + mix64(seed ^ 0xF0A1_DA7A) % (maximum - minimum + 1));
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("foundry probe copper melting point disappeared"));
    let ambient = ROOM_TEMPERATURE.millikelvin();
    let melting = melting_point.millikelvin();
    assert!(
        melting > ambient,
        "foundry probe requires copper to melt above room temperature"
    );
    let preheat_span = (melting - ambient) * 3 / 4;
    let input_temperature = Temperature::from_millikelvin(
        ambient + (mix64(seed ^ 0x5448_4552_4D41_4C49) % (u64::from(preheat_span) + 1)) as u32,
    );
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let sensible = calculate_sensible_heat(
        registries.materials(),
        mass,
        &composition,
        input_temperature,
        melting_point,
    )
    .unwrap_or_else(|error| panic!("foundry probe sensible heating calculation failed: {error}"))
    .energy();
    let fusion = calculate_fusion_heat(registries.materials(), mass, MATERIAL_COPPER)
        .unwrap_or_else(|error| panic!("foundry probe fusion calculation failed: {error}"))
        .energy();
    let required_electrical = sensible
        .checked_add(fusion)
        .unwrap_or_else(|| panic!("foundry probe required electrical energy overflowed"));
    let electrical_capacity = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("foundry probe electrical-buffer definition disappeared"));
    let electrical_slack = electrical_capacity
        .checked_sub(required_electrical)
        .unwrap_or_else(|| panic!("foundry electrical buffer cannot power a legal melt batch"));
    let headroom_ppm = 50_000 + (mix64(seed ^ 0x454C_4543_4845_4144) % 550_001) as u32;
    let electrical_headroom = Energy::from_nanojoules(
        electrical_slack
            .nanojoules()
            .checked_mul(u128::from(headroom_ppm))
            .map(|scaled| scaled / 1_000_000)
            .unwrap_or_else(|| panic!("foundry electrical headroom scaling overflowed")),
    );
    let electrical_energy = required_electrical
        .checked_add(electrical_headroom)
        .unwrap_or_else(|| panic!("foundry initial electrical energy overflowed"));
    FoundrySetup {
        mass,
        input_temperature,
        furnace_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_ELECTRIC_FURNACE,
            mix64(seed ^ 0x4655_524E_4143_4543),
        ),
        mold_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_CASTING_MOLD,
            mix64(seed ^ 0x4D4F_4C44_434F_4E44),
        ),
        electrical_energy,
    }
}

pub(super) fn run_foundry_capability_probe(registries: &Registries, seed: u64) {
    let setup = probe_setup(registries, seed);
    let mass = setup.mass;
    let input_temperature = setup.input_temperature;
    let initial_furnace_condition = setup.furnace_condition;
    let initial_mold_condition = setup.mold_condition;
    let (mut state, ids) = setup_foundry_probe(registries, seed, setup);
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("foundry initial matter accounting failed: {error}"))
        .total();
    let initial_electrical = state
        .energy()
        .get_store(ids.electrical_buffer)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry electrical buffer disappeared after setup"));
    let initial_thermal = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared after setup"));
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
    let melt_job = validate_start_process(
        registries,
        &state,
        melt.process_resolution(),
        ids.pure_copper_source,
        ids.molten_vessel,
    )
    .unwrap_or_else(|error| panic!("foundry probe melt start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe melt commit failed: {error}"));
    finish_production_job(
        registries,
        &mut state,
        melt_job,
        melt_duration,
        "foundry melt",
    );

    let molten_selection =
        select_stockpile_mass(&state, ids.molten_vessel, mass, "foundry molten output");
    let casting = resolve_casting_process(
        registries,
        &state,
        CastingRequest::new(
            PROCESS_CAST_PURE_COPPER,
            ids.molten_vessel,
            molten_selection.as_slice(),
            ids.mold,
            ids.heat_sink,
        ),
    )
    .unwrap_or_else(|error| panic!("foundry probe pure-copper casting failed: {error}"));
    let cast_duration = casting.process_resolution().duration();
    let released_heat = casting.released_energy();
    let cast_job = validate_start_process(
        registries,
        &state,
        casting.process_resolution(),
        ids.molten_vessel,
        ids.cast_storage,
    )
    .unwrap_or_else(|error| panic!("foundry probe casting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe casting commit failed: {error}"));
    finish_production_job(
        registries,
        &mut state,
        cast_job,
        cast_duration,
        "foundry casting",
    );

    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("foundry probe final state audit failed: {error}"));
    let final_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("foundry final matter accounting failed: {error}"))
        .total();
    let final_electrical = state
        .energy()
        .get_store(ids.electrical_buffer)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry electrical buffer disappeared after processing"));
    let final_thermal = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared after processing"));
    assert_eq!(
        final_matter, initial_matter,
        "foundry melt/cast cycle must conserve represented matter"
    );
    assert_eq!(
        initial_electrical.checked_sub(melt.required_energy()),
        Some(final_electrical),
        "foundry melt must consume exactly its resolved electrical energy"
    );
    assert_eq!(
        initial_thermal.checked_add(released_heat),
        Some(final_thermal),
        "foundry casting must recover exactly its resolved released heat into the finite sink"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.cast_storage)
            .map(|stockpile| stockpile.stored_mass()),
        Some(mass),
        "foundry capability probe must conserve cast output mass"
    );
    std::println!(
        "CAPABILITY FOUNDRY seed=0x{seed:016X} reachability=bootstrapped-industrial installation=required+structurally-supported role=capability-evidence player-loop=not-claimed system-depth=[phase-change,finite-electrical-input,finite-thermal-recovery,wear] batch={}mg input={}mK initial-condition=[furnace:{} mold:{}ppm] electrical=[initial:{}nJ melt:{}nJ remaining:{}nJ] thermal=[released:{}nJ sink:{}nJ] melt={}t cast={}t matter=conserved",
        mass.milligrams(),
        input_temperature.millikelvin(),
        initial_furnace_condition.parts_per_million(),
        initial_mold_condition.parts_per_million(),
        initial_electrical.nanojoules(),
        melt.required_energy().nanojoules(),
        final_electrical.nanojoules(),
        released_heat.nanojoules(),
        final_thermal.nanojoules(),
        melt_duration.value(),
        cast_duration.value(),
    );
}
