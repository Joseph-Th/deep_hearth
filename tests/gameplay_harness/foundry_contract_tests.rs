//! Cheap foundry generator contracts kept beside the focused foundry executable.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_THERMAL_SINK, ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER,
    ENERGY_COPPER_RACK_ELECTRICAL_BUFFER, ENERGY_STONE_THERMAL_SINK,
    EQUIPMENT_DOUBLE_WOUND_TREADLE_DYNAMO, EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
    EQUIPMENT_FOUR_POT_ARC_CRUCIBLE_FURNACE, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
    EQUIPMENT_STONE_INGOT_MOLD, EQUIPMENT_TIMBER_TREADLE_DYNAMO, FORM_SCRAP,
    MANUAL_POWER_TREADLE_DYNAMO, MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER,
    PROCESS_MELT_PURE_COPPER, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{validate_assemble_energy_store, validate_upgrade_energy_store};
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_player_process;
use deep_hearth::thermal::{
    CastingRequest, MeltingRequest, calculate_fusion_heat, calculate_sensible_heat,
    resolve_casting_process, resolve_melting_process,
};

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::nominal_equipment_mass_capability;
use super::foundry_probe_generation::probe_setup;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::production_timing::finish_uninterrupted_production_job;
use super::world_admission::admit_stationary_player;

#[test]
fn settlement_foundry_upgrade_executes_one_authored_batch_through_canonical_work() {
    let registries = build_registries();
    let melting_definition = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("settlement foundry melting definition disappeared"));
    let casting_definition = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("settlement foundry casting definition disappeared"));
    let settlement_cast_mass = nominal_equipment_mass_capability(
        &registries,
        EQUIPMENT_FOUR_POT_ARC_CRUCIBLE_FURNACE,
        melting_definition.max_batch_mass_capability(),
    )
    .min(nominal_equipment_mass_capability(
        &registries,
        EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
        casting_definition.max_batch_mass_capability(),
    ));
    assert!(
        !settlement_cast_mass.is_zero(),
        "settlement foundry authored furnace/mold batch must remain nonzero"
    );
    let mut required_parts = BTreeMap::<CommodityKey, Mass>::new();
    for equipment in [
        EQUIPMENT_DOUBLE_WOUND_TREADLE_DYNAMO,
        EQUIPMENT_FOUR_POT_ARC_CRUCIBLE_FURNACE,
        EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
    ] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| panic!("settlement foundry equipment lost assembly profile"));
        for input in profile.inputs() {
            let total = required_parts
                .entry(input.commodity())
                .or_insert(Mass::ZERO);
            *total = total
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("settlement foundry equipment material overflowed"));
        }
    }
    for store in [
        ENERGY_COPPER_RACK_ELECTRICAL_BUFFER,
        ENERGY_COPPER_BANDED_STONE_THERMAL_SINK,
    ] {
        let profile = registries
            .energy()
            .get_store(store)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| panic!("settlement foundry energy store lost assembly profile"));
        for input in profile.inputs() {
            let total = required_parts
                .entry(input.commodity())
                .or_insert(Mass::ZERO);
            *total = total
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("settlement foundry energy material overflowed"));
        }
    }
    let parts_capacity = required_parts
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("settlement foundry parts capacity overflowed"));

    let mut state = AppState::new();
    let parts = seed_stockpile(
        &mut state,
        parts_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in required_parts {
        let _ = seed_lot(
            &registries,
            &mut state,
            parts,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let feed = seed_stockpile(
        &mut state,
        settlement_cast_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let _ = seed_lot(
        &registries,
        &mut state,
        feed,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        settlement_cast_mass,
        ROOM_TEMPERATURE,
    );
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("settlement foundry copper melting point disappeared"));
    let molten = seed_stockpile(
        &mut state,
        settlement_cast_mass,
        StockpileStorageProfile::new(false, true, melting_point)
            .unwrap_or_else(|error| panic!("settlement molten storage profile failed: {error}")),
    );
    let cast = seed_stockpile(
        &mut state,
        settlement_cast_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    admit_stationary_player(
        &registries,
        &mut state,
        &[parts, feed, molten, cast],
        &[],
        "settlement foundry",
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("settlement foundry matter setup failed: {error}"))
        .total();

    let assemble_equipment = |state: &mut AppState, definition| {
        validate_assemble_equipment(&registries, state, definition, parts)
            .unwrap_or_else(|error| panic!("settlement base equipment assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("settlement base equipment commit failed: {error}"))
    };
    let base_dynamo = assemble_equipment(&mut state, EQUIPMENT_TIMBER_TREADLE_DYNAMO);
    let dynamo = validate_upgrade_equipment(
        &registries,
        &state,
        base_dynamo,
        EQUIPMENT_DOUBLE_WOUND_TREADLE_DYNAMO,
        parts,
    )
    .unwrap_or_else(|error| panic!("settlement dynamo upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement dynamo upgrade commit failed: {error}"));
    let base_furnace = assemble_equipment(&mut state, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE);
    let furnace = validate_upgrade_equipment(
        &registries,
        &state,
        base_furnace,
        EQUIPMENT_FOUR_POT_ARC_CRUCIBLE_FURNACE,
        parts,
    )
    .unwrap_or_else(|error| panic!("settlement furnace upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement furnace upgrade commit failed: {error}"));
    let base_mold = assemble_equipment(&mut state, EQUIPMENT_STONE_INGOT_MOLD);
    let mold = validate_upgrade_equipment(
        &registries,
        &state,
        base_mold,
        EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
        parts,
    )
    .unwrap_or_else(|error| panic!("settlement mold upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement mold upgrade commit failed: {error}"));

    let electrical = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER,
        parts,
    )
    .unwrap_or_else(|error| panic!("settlement base electrical buffer assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement base electrical buffer commit failed: {error}"));
    validate_upgrade_energy_store(
        &registries,
        &state,
        electrical,
        ENERGY_COPPER_RACK_ELECTRICAL_BUFFER,
        parts,
    )
    .unwrap_or_else(|error| panic!("settlement electrical buffer upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement electrical buffer upgrade commit failed: {error}"));
    let heat_sink =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_THERMAL_SINK, parts)
            .unwrap_or_else(|error| panic!("settlement base thermal sink assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("settlement base thermal sink commit failed: {error}"));
    validate_upgrade_energy_store(
        &registries,
        &state,
        heat_sink,
        ENERGY_COPPER_BANDED_STONE_THERMAL_SINK,
        parts,
    )
    .unwrap_or_else(|error| panic!("settlement thermal sink upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement thermal sink upgrade commit failed: {error}"));

    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let required_energy = calculate_sensible_heat(
        registries.materials(),
        settlement_cast_mass,
        &composition,
        ROOM_TEMPERATURE,
        melting_point,
    )
    .unwrap_or_else(|error| panic!("settlement foundry sensible heat failed: {error}"))
    .energy()
    .checked_add(
        calculate_fusion_heat(
            registries.materials(),
            settlement_cast_mass,
            MATERIAL_COPPER,
        )
        .unwrap_or_else(|error| panic!("settlement foundry fusion heat failed: {error}"))
        .energy(),
    )
    .unwrap_or_else(|| panic!("settlement foundry melt energy overflowed"));
    assert!(
        required_energy
            <= registries
                .energy()
                .get_store(ENERGY_COPPER_RACK_ELECTRICAL_BUFFER)
                .unwrap_or_else(|| panic!("settlement electrical definition disappeared"))
                .capacity()
    );
    let charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_TREADLE_DYNAMO,
            dynamo,
            electrical,
            required_energy,
        ),
    )
    .unwrap_or_else(|error| panic!("settlement foundry manual charge failed: {error}"));
    let charge_work = charge.work();
    charge
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("settlement foundry manual charge commit failed: {error}"));
    let _charge_ticks = finish_manual_power_work(
        &registries,
        &mut state,
        charge_work,
        "settlement foundry charging",
    );

    let feed_selection = select_stockpile_mass(
        &state,
        feed,
        settlement_cast_mass,
        "settlement foundry copper feed",
    );
    let melting = resolve_melting_process(
        &registries,
        &state,
        MeltingRequest::new(
            PROCESS_MELT_PURE_COPPER,
            feed,
            feed_selection.as_slice(),
            furnace,
            electrical,
        ),
    )
    .unwrap_or_else(|error| panic!("settlement foundry melt resolution failed: {error}"));
    assert_eq!(melting.required_energy(), required_energy);
    let melt_job = validate_start_player_process(
        &registries,
        &state,
        melting.process_resolution(),
        feed,
        molten,
    )
    .unwrap_or_else(|error| panic!("settlement foundry melt start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement foundry melt commit failed: {error}"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        melt_job,
        "settlement foundry melt",
    );

    let molten_selection = select_stockpile_mass(
        &state,
        molten,
        settlement_cast_mass,
        "settlement foundry molten feed",
    );
    let casting = resolve_casting_process(
        &registries,
        &state,
        CastingRequest::new(
            PROCESS_CAST_PURE_COPPER,
            molten,
            molten_selection.as_slice(),
            mold,
            heat_sink,
        ),
    )
    .unwrap_or_else(|error| panic!("settlement foundry casting resolution failed: {error}"));
    let cast_job = validate_start_player_process(
        &registries,
        &state,
        casting.process_resolution(),
        molten,
        cast,
    )
    .unwrap_or_else(|error| panic!("settlement foundry cast start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement foundry cast commit failed: {error}"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        cast_job,
        "settlement foundry cast",
    );

    assert_eq!(
        state
            .inventory()
            .get_stockpile(cast)
            .map(|stockpile| stockpile.stored_mass()),
        Some(settlement_cast_mass)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("settlement foundry final state invalid: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("settlement foundry matter audit failed: {error}"))
            .total(),
        matter_before,
        "settlement batch foundry must conserve matter across assembly, upgrades, melt, and cast"
    );
}

#[test]
fn foundry_generation_covers_authored_feed_forms_and_varies_conditions() {
    let registries = build_registries();
    let authored_feed_forms = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical copper melting definition disappeared"))
        .solid_forms()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert!(
        !authored_feed_forms.is_empty(),
        "canonical copper melting must retain at least one solid recovery feed form"
    );

    let sample_count = authored_feed_forms.len().max(8);
    let setups = (1_u64
        ..=u64::try_from(sample_count)
            .unwrap_or_else(|_| unreachable!("bounded foundry sample count fits u64")))
        .map(|seed| probe_setup(&registries, seed))
        .collect::<Vec<_>>();
    assert_eq!(
        setups
            .iter()
            .map(|setup| setup.feed_form)
            .collect::<BTreeSet<_>>(),
        authored_feed_forms,
        "bounded foundry generation must exercise every authored pure-copper recovery feed form"
    );
    assert!(
        setups
            .iter()
            .map(|setup| (setup.mass.milligrams(), setup.preheat_target.millikelvin()))
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "foundry generation collapsed to one thermal batch"
    );
    assert!(
        setups
            .iter()
            .map(|setup| {
                (
                    setup.furnace_condition.parts_per_million(),
                    setup.mold_condition.parts_per_million(),
                    setup.electrical_energy.nanojoules(),
                    setup.thermal_sink_energy.nanojoules(),
                )
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "foundry generation collapsed to one operating state"
    );
}
