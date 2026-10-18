//! Same-world reinvestment from cast copper into an owned, operating helve hammer.

use super::*;
use deep_hearth::content::{FORM_BOARD, FORM_HANDLE, PROCESS_POWER_HAMMER_CAST_COPPER_WINDING};
use deep_hearth::crafting::{
    player_powered_craft_options, validate_start_player_powered_craft_option,
};
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::labor::{PlayerWork, validate_start_manual_power};
use deep_hearth::simulation::advance_tick;
use deep_hearth::surface::{
    player_surface_gathering_options, validate_start_player_surface_gathering_option,
};

pub(super) fn execute_powered_reinvestment(
    registries: &Registries,
    state: &mut AppState,
    cast: StockpileId,
    parts: StockpileId,
    frame_saw: EquipmentId,
    treadle_hammer: EquipmentId,
    crank: EquipmentId,
    flywheel: EnergyStoreId,
    electrical: EnergyStoreId,
    initial_matter: deep_hearth::core::quantity::AggregateMass,
    seed: u64,
    manual_winding_ticks: u64,
) {
    let started_at = state.tick().value();
    let upgrade_target = EQUIPMENT_TIMBER_HELVE_HAMMER;
    let visible_upgrade = |state: &AppState| {
        player_equipment_upgrade_options(registries, state, parts)
            .unwrap_or_else(|error| panic!("helve material catalog failed: {error}"))
            .into_iter()
            .find(|option| {
                option.equipment() == treadle_hammer && option.target_definition() == upgrade_target
            })
            .unwrap_or_else(|| panic!("treadle hammer must disclose its helve upgrade"))
    };
    assert!(!visible_upgrade(state).material_ready());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(cast)
            .unwrap_or_else(|| panic!("cast stock disappeared"))
            .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)),
        Mass::from_milligrams(80_000),
    );

    // The player discovers and gathers a finite local timber opportunity after casting, not a
    // newly materialized finished component. The same resource stays untouched in other branches.
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let options = player_surface_gathering_options(registries, state)
        .unwrap_or_else(|error| panic!("helve timber observations failed: {error}"));
    let timber_options = options
        .into_iter()
        .filter(|option| option.commodity() == timber)
        .collect::<Vec<_>>();
    assert_eq!(
        timber_options.len(),
        1,
        "one local fallen-timber deposit must be visible"
    );
    let opportunity = timber_options[0];
    let raw_timber = Mass::from_milligrams(3_000_000);
    assert_eq!(opportunity.source_mass(), raw_timber);
    assert!(opportunity.maximum_batch_mass() >= raw_timber);
    validate_start_player_surface_gathering_option(registries, state, opportunity, raw_timber)
        .unwrap_or_else(|error| panic!("helve timber gathering was rejected: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("helve timber gathering could not start: {error}"));
    let work = match state.player_work().active() {
        Some(PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("helve timber did not reserve gathering attention: {other:?}"),
    };
    let gathering_ticks = work.completes_at().value() - state.tick().value();
    assert!(gathering_ticks > 0);
    for elapsed in 1..=gathering_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("helve timber gathering tick failed: {error}"));
        if elapsed == gathering_ticks {
            let completed = outcome
                .surface_gathering()
                .unwrap_or_else(|| panic!("helve timber gathering had no receipt"));
            assert_eq!(completed.gathered_mass(), raw_timber);
            assert_eq!(state.player_work().active(), None);
        } else {
            assert!(outcome.surface_gathering().is_none());
        }
    }
    let carried = state
        .logistics()
        .player()
        .unwrap_or_else(|| panic!("helve player disappeared"))
        .carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap_or_else(|| panic!("helve carried stockpile disappeared"))
            .get_mass(timber),
        raw_timber,
    );

    // The actor selects ordinary visible production paths using owned equipment. No recipe or
    // lot is injected into the workshop after the original fallen timber was gathered.
    let board = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let board_before = state
        .inventory()
        .get_stockpile(parts)
        .unwrap_or_else(|| panic!("helve parts stockpile disappeared"))
        .get_mass(board);
    let (saw, _) = manual_craft_plan_with_available_equipment(
        registries,
        state,
        &[carried],
        &[frame_saw],
        board,
        Mass::from_milligrams(800_000),
        "helve reinforcement boards",
    );
    let boards_ticks =
        execute_manual_craft(registries, state, saw, parts, "helve reinforcement boards").value();
    let board_gained = state
        .inventory()
        .get_stockpile(parts)
        .unwrap_or_else(|| panic!("helve board output disappeared"))
        .get_mass(board)
        .checked_sub(board_before)
        .unwrap_or_else(|| panic!("helve board shaping lost inventory matter"));
    assert!(board_gained >= Mass::from_milligrams(800_000));
    let (handles, _) = manual_craft_plan_with_available_equipment(
        registries,
        state,
        &[carried],
        &[frame_saw],
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(400_000),
        "helve reinforcement handles",
    );
    let handles_ticks = execute_manual_craft(
        registries,
        state,
        handles,
        parts,
        "helve reinforcement handles",
    )
    .value();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap_or_else(|| panic!("helve material custody disappeared"))
            .get_mass(timber),
        Mass::ZERO,
    );
    let (bearing, _) = manual_craft_plan_with_available_equipment(
        registries,
        state,
        &[cast],
        &[], // The 20 g hand batch ties assisted time; avoid wearing the inherited hammer.
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        "helve cast-copper bearing",
    );
    let bearing_ticks = execute_manual_craft(
        registries,
        state,
        bearing,
        parts,
        "helve cast-copper bearing",
    )
    .value();
    let option = visible_upgrade(state);
    assert!(
        option.material_ready(),
        "gathered and fabricated additions must unlock the helve"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(cast)
            .unwrap_or_else(|| panic!("post-bearing cast stock vanished"))
            .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)),
        Mass::from_milligrams(60_000),
    );
    let helve = validate_upgrade_player_equipment_option(registries, state, &option)
        .unwrap_or_else(|error| panic!("acquired helve upgrade rejected: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("acquired helve upgrade commit failed: {error}"));
    assert_eq!(
        helve, treadle_hammer,
        "upgrade must retain inherited hammer identity"
    );

    // The remaining 60 g of the same cast is enough for a real delegated electrical winding.
    let transform = PROCESS_COLD_WORK_CAST_COPPER_WINDING;
    let select_machine = |state: &AppState| {
        player_powered_craft_options(registries, state, cast, transform)
            .unwrap_or_else(|error| panic!("post-helve machine catalog failed: {error}"))
            .into_iter()
            .find(|option| {
                option.process() == PROCESS_POWER_HAMMER_CAST_COPPER_WINDING
                    && option.equipment() == helve
                    && option.energy_store() == flywheel
            })
            .unwrap_or_else(|| panic!("acquired helve must expose an actual powered winding job"))
    };
    let uncharged = select_machine(state);
    assert_eq!(uncharged.available_batches(), 1);
    let stored = state
        .energy()
        .get_store(flywheel)
        .unwrap_or_else(|| panic!("inherited flywheel disappeared"))
        .stored();
    let charge = uncharged
        .required_energy()
        .checked_sub(stored)
        .unwrap_or(Energy::ZERO);
    let charge_ticks = if charge.is_zero() {
        0
    } else {
        let validated = validate_start_manual_power(
            registries,
            state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, flywheel, charge),
        )
        .unwrap_or_else(|error| panic!("post-helve mechanical charge rejected: {error}"));
        let work = validated.work();
        validated
            .commit(state)
            .unwrap_or_else(|error| panic!("post-helve charge commit failed: {error}"));
        finish_manual_power_work(registries, state, work, "post-helve mechanical charge")
    };
    let ready = select_machine(state);
    assert!(ready.charge_ready());
    let job = validate_start_player_powered_craft_option(registries, state, ready, parts)
        .unwrap_or_else(|error| panic!("first acquired helve job rejected: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("first acquired helve job commit failed: {error}"));
    assert_eq!(state.player_work().active(), None);
    let machine_ticks = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("first helve production job disappeared"))
        .active_duration()
        .value();
    finish_uninterrupted_production_job(registries, state, job, "first acquired helve job");
    let cast_remaining = state
        .inventory()
        .get_stockpile(cast)
        .unwrap_or_else(|| panic!("first helve cast custody disappeared"))
        .stored_mass();
    assert_eq!(cast_remaining, Mass::ZERO);
    let output = state
        .inventory()
        .get_stockpile(parts)
        .unwrap_or_else(|| panic!("first helve finished stockpile disappeared"));
    assert!(
        output.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING))
            >= Mass::from_milligrams(54_000),
    );
    assert!(
        output.get_mass(board)
            >= board_gained
                .checked_sub(Mass::from_milligrams(800_000))
                .unwrap_or_else(|| unreachable!("board production covers assembly"))
    );
    let installed_before = state
        .energy()
        .get_store(electrical)
        .unwrap_or_else(|| panic!("acquired helve electrical buffer disappeared"));
    assert_eq!(installed_before.stored(), Energy::ZERO);
    let before_mass = installed_before.embodied_mass();
    let buffer_option = player_energy_store_upgrade_options(registries, state, parts)
        .unwrap_or_else(|error| panic!("helve-wound buffer catalog failed: {error}"))
        .into_iter()
        .find(|option| {
            option.store() == electrical && option.target() == ENERGY_COPPER_WOUND_ELECTRICAL_BUFFER
        })
        .unwrap_or_else(|| panic!("powered winding must expose the wound buffer choice"));
    assert!(buffer_option.material_ready());
    let installed = validate_upgrade_player_energy_store_option(registries, state, &buffer_option)
        .unwrap_or_else(|error| panic!("helve-made winding upgrade rejected: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("helve-made winding upgrade commit failed: {error}"));
    assert_eq!(installed, electrical);
    let installed_after = state
        .energy()
        .get_store(electrical)
        .unwrap_or_else(|| panic!("helve-made wound buffer vanished"));
    assert_eq!(
        installed_after.definition(),
        ENERGY_COPPER_WOUND_ELECTRICAL_BUFFER
    );
    assert_eq!(installed_after.stored(), Energy::ZERO);
    assert_eq!(
        installed_after.embodied_mass().checked_sub(before_mass),
        Some(Mass::from_milligrams(54_000)),
    );
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!("first acquired helve matter audit failed: {error}"))
            .total(),
        initial_matter,
    );
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("first acquired helve state invalid: {error}"));
    let base_capacity = registries
        .energy()
        .get_store(ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("original electrical buffer definition vanished"))
        .capacity()
        .nanojoules();
    let upgraded_capacity = registries
        .energy()
        .get_store(ENERGY_COPPER_WOUND_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("upgraded electrical buffer definition vanished"))
        .capacity()
        .nanojoules();
    assert!(upgraded_capacity > base_capacity);
    reviewln!(
        "FIRST FOUNDRY HELVE REINVESTMENT seed=0x{seed:016X} continuity=one-exact-local-body source=[cast:80000mg finite-timber:3000000mg] acquired=[timber:3000000mg gather-attention:{gathering_ticks}t] fabricated=[board:{}mg attention:{boards_ticks}t handles:400000mg attention:{handles_ticks}t copper-bearing:20000mg attention:{bearing_ticks}t] capital=[upgraded:treadle-to-helve identity:preserved] production=[winding:54000mg charge-attention:{charge_ticks}t autonomous:{machine_ticks}t cast-remains:0mg] electrical=[upgrade:plate-to-wound identity:preserved capacity:{base_capacity}->{upgraded_capacity}nJ] comparison=[manual-winding:{manual_winding_ticks}t acquired-helve:{}t] elapsed:{}t matter=conserved",
        board_gained.milligrams(),
        state.tick().value() - started_at,
        state.tick().value() - started_at,
    );
}
