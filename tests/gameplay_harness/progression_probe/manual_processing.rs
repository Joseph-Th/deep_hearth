//! Bounded zero-powered ore-processing fallback from owned ore to a usable copper reinforcement.

use deep_hearth::content::gameplay_fixture::seed_composed_lot;
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, FORM_CRUSHED, FORM_NATIVE_METAL, FORM_ORE,
    FORM_REINFORCEMENT, MATERIAL_COPPER, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_SEPARATE_NATIVE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{TickSpan, WorldSeed};
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::material::{COMPOSITION_PARTS_PER_MILLION, CommodityKey};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ConstituentSeparationProcessDefinition, ManualComminutionRequest,
    ManualConstituentSeparationProcessDefinition, ManualConstituentSeparationRequest,
    resolve_manual_comminution_process, resolve_manual_constituent_separation_process,
    validate_start_manual_comminution, validate_start_manual_constituent_separation,
};
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};

use super::super::environment::ROOM_TEMPERATURE;
use super::super::inventory_support::add_solid_stockpile;
use super::super::material_selection::select_stockpile_mass;
use super::super::ore_fixture::copper_ore_composition;
use super::super::production_timing::finish_uninterrupted_production_job;
use super::super::seed::mix64;
use super::{craft_batches, native_input_for_upgrade};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnedOreManualBridgeReview {
    pub(super) feed_mass: Mass,
    pub(super) total_attention_ticks: u64,
    pub(super) manual_recovery_ppm: u32,
    pub(super) powered_recovery_ppm: u32,
    pub(super) metabolic_cost_nj: u128,
    pub(super) hydration_cost_ul: u64,
}

#[derive(Clone, Copy)]
pub(super) struct OwnedOreManualBridgePlan {
    pub(super) ore_source: StockpileId,
    pub(super) crushed_destination: StockpileId,
    pub(super) native_destination: StockpileId,
    pub(super) residue_destination: StockpileId,
    pub(super) shaped_destination: StockpileId,
    pub(super) copper_ppm: u32,
    pub(super) reinforcement_required: Mass,
}

/// Replays the real no-machine bridge from a player-owned ore parcel without mutating the source
/// episode. The caller supplies only stockpiles that already exist at the observed decision point;
/// this branch performs no fixture mutation and uses the same canonical work APIs as ordinary play.
pub(super) fn evaluate_owned_ore_manual_bridge(
    registries: &Registries,
    decision_state: &AppState,
    plan: OwnedOreManualBridgePlan,
) -> OwnedOreManualBridgeReview {
    let mut state = decision_state.clone();
    run_owned_ore_manual_bridge(registries, &mut state, plan)
}

/// Executes the same manual bridge on a live branch state. This exists so progression can compare
/// infrastructure-first play against a genuine manual-bootstrap branch without duplicating ore
/// processing arithmetic or bypassing canonical production work.
pub(super) fn run_owned_ore_manual_bridge(
    registries: &Registries,
    state: &mut AppState,
    plan: OwnedOreManualBridgePlan,
) -> OwnedOreManualBridgeReview {
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("owned-ore manual bridge lost its hand-breaking definition"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("owned-ore manual bridge lost its hand-sorting definition"));
    let powered_sorting = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("owned-ore manual bridge lost its powered comparison route"));
    let manual_recovery_ppm = sorting.target_recovery_ppm();
    let feed_mass = sorting
        .minimum_feed_mass_for_target_recovery(plan.reinforcement_required, plan.copper_ppm)
        .unwrap_or_else(|| {
            panic!("player-owned bulk ore cannot physically recover one manual reinforcement")
        });
    assert!(
        feed_mass <= breaking.max_batch_mass() && feed_mass <= sorting.max_batch_mass(),
        "player-owned bulk ore cannot fund one reinforcement inside the authored manual batch envelope"
    );
    assert!(
        state
            .inventory()
            .get_stockpile(plan.ore_source)
            .is_some_and(|stockpile| stockpile.stored_mass() >= feed_mass),
        "manual bridge requires its feed to be present in player-owned ore"
    );

    let matter_before = calculate_matter_accounting(state)
        .unwrap_or_else(|error| panic!("owned-ore manual bridge matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("owned-ore manual bridge player disappeared at decision point"));
    let ore_selections = select_stockpile_mass(
        state,
        plan.ore_source,
        feed_mass,
        "owned-ore manual bridge feed",
    );
    let breaking = resolve_manual_comminution_process(
        registries,
        state,
        ManualComminutionRequest::new(PROCESS_HAND_BREAK_ORE, plan.ore_source, &ore_selections),
    )
    .unwrap_or_else(|error| panic!("owned-ore manual bridge hand breaking failed: {error}"));
    let break_ticks = breaking.duration().value();
    let break_job = validate_start_manual_comminution(
        registries,
        state,
        &breaking,
        plan.ore_source,
        plan.crushed_destination,
    )
    .unwrap_or_else(|error| panic!("owned-ore manual bridge breaking start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("owned-ore manual bridge breaking commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        break_job,
        TickSpan::new(break_ticks),
        "owned-ore manual breaking",
    );

    let crushed_selections = select_stockpile_mass(
        state,
        plan.crushed_destination,
        feed_mass,
        "owned-ore manual sorting feed",
    );
    let sorting = resolve_manual_constituent_separation_process(
        registries,
        state,
        ManualConstituentSeparationRequest::new(
            PROCESS_HAND_SORT_NATIVE_COPPER,
            plan.crushed_destination,
            &crushed_selections,
        ),
    )
    .unwrap_or_else(|error| panic!("owned-ore manual bridge hand sorting failed: {error}"));
    let sort_ticks = sorting.duration().value();
    let recovered_native = sorting.target_mass();
    let residue_mass = sorting.residue_mass();
    assert_eq!(
        recovered_native, plan.reinforcement_required,
        "same-world manual bridge must recover exactly the one reinforcement parcel it planned"
    );
    assert_eq!(
        recovered_native.checked_add(residue_mass),
        Some(feed_mass),
        "same-world hand sorting must partition the complete selected ore feed"
    );
    let sort_job = validate_start_manual_constituent_separation(
        registries,
        state,
        &sorting,
        plan.crushed_destination,
        plan.native_destination,
        plan.residue_destination,
    )
    .unwrap_or_else(|error| panic!("owned-ore manual bridge sorting start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("owned-ore manual bridge sorting commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        sort_job,
        TickSpan::new(sort_ticks),
        "owned-ore manual sorting",
    );

    let cold_work_started_at = state.tick().value();
    craft_batches(
        registries,
        state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        plan.native_destination,
        plan.shaped_destination,
        1,
    );
    let cold_work_ticks = state
        .tick()
        .value()
        .checked_sub(cold_work_started_at)
        .unwrap_or_else(|| unreachable!("owned-ore manual bridge cold work cannot reverse time"));
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!("owned-ore manual bridge matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("owned-ore manual bridge state audit failed: {error}"));
    let survival_after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("owned-ore manual bridge player disappeared after work"));
    let metabolic_cost_nj = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| unreachable!("manual bridge cannot create metabolic reserve"))
        .nanojoules();
    let hydration_cost_ul = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| unreachable!("manual bridge cannot create hydration reserve"))
        .microliters();
    let total_attention_ticks = break_ticks
        .checked_add(sort_ticks)
        .and_then(|ticks| ticks.checked_add(cold_work_ticks))
        .unwrap_or_else(|| panic!("owned-ore manual bridge attention overflowed"));

    OwnedOreManualBridgeReview {
        feed_mass,
        total_attention_ticks,
        manual_recovery_ppm,
        powered_recovery_ppm: powered_sorting.target_recovery_ppm(),
        metabolic_cost_nj,
        hydration_cost_ul,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ManualProcessingSetup {
    pub(crate) ore_mass: Mass,
    pub(crate) copper_ppm: u32,
    pub(crate) clay_share_ppm: u32,
}

pub(crate) fn manual_processing_setup(registries: &Registries, seed: u64) -> ManualProcessingSetup {
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual processing fallback lost its hand-breaking definition"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual processing fallback lost its hand-sorting definition"));
    let reinforcement = native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let maximum_mass = breaking.max_batch_mass().min(sorting.max_batch_mass());
    assert!(
        !maximum_mass.is_zero(),
        "manual processing fallback requires a nonzero legal shared batch envelope"
    );
    let minimum_mass_mg: u64 = (u128::from(maximum_mass.milligrams()) * 55 / 100)
        .max(1)
        .try_into()
        .unwrap_or_else(|_| unreachable!("scaled bounded manual batch fits u64"));
    let ore_mass_mg = minimum_mass_mg
        + mix64(seed ^ 0x4841_4E44_4D41_5353) % (maximum_mass.milligrams() - minimum_mass_mg + 1);
    let recovery = u128::from(sorting.target_recovery_ppm());
    let required_copper_ppm = u128::from(reinforcement.milligrams())
        .checked_mul(1_000_000_000_000)
        .unwrap_or_else(|| panic!("manual fallback target scaling overflowed"))
        .div_ceil(u128::from(ore_mass_mg) * recovery);
    assert!(
        required_copper_ppm < u128::from(COMPOSITION_PARTS_PER_MILLION),
        "no single legal manual-processing batch can recover the copper required by the real primitive upgrade"
    );
    let minimum_copper_ppm = u32::try_from(required_copper_ppm)
        .unwrap_or_else(|_| unreachable!("bounded composition fits ppm"));
    let maximum_copper_ppm = minimum_copper_ppm
        .saturating_add(250_000)
        .min(COMPOSITION_PARTS_PER_MILLION - 1);
    let copper_ppm = minimum_copper_ppm
        + u32::try_from(
            mix64(seed ^ 0x4841_4E44_4752_4144)
                % u64::from(maximum_copper_ppm - minimum_copper_ppm + 1),
        )
        .unwrap_or_else(|_| unreachable!("bounded composition variation fits u32"));
    let clay_share_ppm = u32::try_from(mix64(seed ^ 0x4841_4E44_4741_4E47) % 750_001)
        .unwrap_or_else(|_| unreachable!("bounded gangue-share variation fits u32"));
    ManualProcessingSetup {
        ore_mass: Mass::from_milligrams(ore_mass_mg),
        copper_ppm,
        clay_share_ppm,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManualProcessingFallbackReview {
    pub(super) break_ticks: u64,
    pub(super) sort_ticks: u64,
    pub(super) cold_work_ticks: u64,
    pub(super) total_attention_ticks: u64,
    pub(super) ore_mass_mg: u64,
    pub(super) ore_copper_ppm: u32,
    pub(super) gangue_clay_share_ppm: u32,
    pub(super) recovered_native_mg: u64,
    pub(super) residue_mg: u64,
    pub(super) reinforcement_mg: u64,
    pub(super) native_remainder_mg: u64,
    pub(super) manual_recovery_ppm: u32,
    pub(super) powered_recovery_ppm: u32,
    pub(super) metabolic_cost_nj: u128,
    pub(super) hydration_cost_ul: u64,
}

pub(super) fn evaluate_manual_processing_fallback(
    registries: &Registries,
    seed: u64,
) -> ManualProcessingFallbackReview {
    let setup = manual_processing_setup(registries, seed);
    let composition = copper_ore_composition(setup.copper_ppm, setup.clay_share_ppm);
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x4841_4E44_5F4F_5245));
    let ore = add_solid_stockpile(&mut state, setup.ore_mass);
    let crushed = add_solid_stockpile(&mut state, setup.ore_mass);
    let native = add_solid_stockpile(&mut state, setup.ore_mass);
    let residue = add_solid_stockpile(&mut state, setup.ore_mass);
    let reinforcement_required =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let shaped = add_solid_stockpile(&mut state, reinforcement_required);
    let ore_lot = seed_composed_lot(
        registries,
        &mut state,
        ore,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        setup.ore_mass,
        ROOM_TEMPERATURE,
        composition.clone(),
    );
    // All route-fixture starting matter exists before actor admission.
    initialize_player_survival(registries, &mut state).unwrap_or_else(|error| {
        panic!("manual processing fallback survival setup failed: {error}")
    });
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual processing fallback matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("manual processing fallback player disappeared at setup"));

    let breaking = resolve_manual_comminution_process(
        registries,
        &state,
        ManualComminutionRequest::new(
            PROCESS_HAND_BREAK_ORE,
            ore,
            &[MaterialLotSelection::new(ore_lot, setup.ore_mass)],
        ),
    )
    .unwrap_or_else(|error| panic!("manual processing fallback hand breaking failed: {error}"));
    let break_ticks = breaking.duration().value();
    let break_job = validate_start_manual_comminution(registries, &state, &breaking, ore, crushed)
        .unwrap_or_else(|error| panic!("manual processing fallback breaking start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("manual processing fallback breaking commit failed: {error}")
        });
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        break_job,
        TickSpan::new(break_ticks),
        "manual processing fallback breaking",
    );
    assert_eq!(state.player_work().active(), None);
    let crushed_record = state
        .inventory()
        .get_stockpile(crushed)
        .unwrap_or_else(|| panic!("manual processing fallback crushed stockpile disappeared"));
    assert_eq!(crushed_record.stored_mass(), setup.ore_mass);
    assert_eq!(
        crushed_record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)),
        setup.ore_mass
    );

    let selections = select_stockpile_mass(
        &state,
        crushed,
        setup.ore_mass,
        "manual processing fallback sorting feed",
    );
    let sorting = resolve_manual_constituent_separation_process(
        registries,
        &state,
        ManualConstituentSeparationRequest::new(
            PROCESS_HAND_SORT_NATIVE_COPPER,
            crushed,
            selections.as_slice(),
        ),
    )
    .unwrap_or_else(|error| panic!("manual processing fallback hand sorting failed: {error}"));
    let sort_ticks = sorting.duration().value();
    let recovered_native = sorting.target_mass();
    let residue_mass = sorting.residue_mass();
    let sort_job = validate_start_manual_constituent_separation(
        registries, &state, &sorting, crushed, native, residue,
    )
    .unwrap_or_else(|error| panic!("manual processing fallback sorting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual processing fallback sorting commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        sort_job,
        TickSpan::new(sort_ticks),
        "manual processing fallback sorting",
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        recovered_native.checked_add(residue_mass),
        Some(setup.ore_mass),
        "manual sorting must partition all hand-broken matter"
    );
    assert!(
        recovered_native >= reinforcement_required,
        "one bounded manual-processing batch must recover enough copper for a real primitive reinforcement"
    );

    let cold_work_started_at = state.tick().value();
    craft_batches(
        registries,
        &mut state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        native,
        shaped,
        1,
    );
    let cold_work_ticks = state
        .tick()
        .value()
        .checked_sub(cold_work_started_at)
        .unwrap_or_else(|| unreachable!("manual fallback cold work cannot reverse time"));
    assert_eq!(
        state.inventory().get_stockpile(shaped).map(|stockpile| {
            stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
        }),
        Some(reinforcement_required),
        "manual ore processing must feed the ordinary reinforcement crafting route"
    );
    let native_remainder = state
        .inventory()
        .get_stockpile(native)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("manual processing fallback native stockpile disappeared"));
    assert_eq!(
        native_remainder,
        recovered_native
            .checked_sub(reinforcement_required)
            .unwrap_or_else(|| unreachable!("fallback recovery was checked above"))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "manual processing fallback matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("manual processing fallback state audit failed: {error}"));
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("manual processing fallback player disappeared after work"));
    let metabolic_cost_nj = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| unreachable!("manual processing cannot create metabolic reserve"))
        .nanojoules();
    let hydration_cost_ul = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| unreachable!("manual processing cannot create hydration reserve"))
        .microliters();
    let manual_recovery_ppm = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .map(ManualConstituentSeparationProcessDefinition::target_recovery_ppm)
        .unwrap_or_else(|| panic!("manual sorting definition disappeared during fallback review"));
    let powered_recovery_ppm = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .map(ConstituentSeparationProcessDefinition::target_recovery_ppm)
        .unwrap_or_else(|| panic!("powered sorting definition disappeared during fallback review"));
    assert!(manual_recovery_ppm < powered_recovery_ppm);
    let total_attention_ticks = break_ticks
        .checked_add(sort_ticks)
        .and_then(|ticks| ticks.checked_add(cold_work_ticks))
        .unwrap_or_else(|| panic!("manual processing fallback attention overflowed"));

    ManualProcessingFallbackReview {
        break_ticks,
        sort_ticks,
        cold_work_ticks,
        total_attention_ticks,
        ore_mass_mg: setup.ore_mass.milligrams(),
        ore_copper_ppm: setup.copper_ppm,
        gangue_clay_share_ppm: setup.clay_share_ppm,
        recovered_native_mg: recovered_native.milligrams(),
        residue_mg: residue_mass.milligrams(),
        reinforcement_mg: reinforcement_required.milligrams(),
        native_remainder_mg: native_remainder.milligrams(),
        manual_recovery_ppm,
        powered_recovery_ppm,
        metabolic_cost_nj,
        hydration_cost_ul,
    }
}
