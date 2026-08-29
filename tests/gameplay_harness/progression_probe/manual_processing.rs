//! Bounded zero-powered ore-processing fallback from owned ore to a usable copper reinforcement.

use deep_hearth::content::gameplay_fixture::seed_composed_lot;
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, FORM_CRUSHED, FORM_NATIVE_METAL, FORM_ORE,
    FORM_REINFORCEMENT, MATERIAL_COPPER, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_SEPARATE_NATIVE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::inventory::MaterialLotSelection;
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

use super::super::seed::mix64;
use super::super::support::{ROOM_TEMPERATURE, add_solid_stockpile};
use super::super::{
    ore_fixture::copper_ore_composition, production_support::select_stockpile_mass,
};
use super::{advance_exact, craft_batches, native_input_for_upgrade};

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
    initialize_player_survival(registries, &mut state).unwrap_or_else(|error| {
        panic!("manual processing fallback survival setup failed: {error}")
    });
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
    validate_start_manual_comminution(registries, &state, &breaking, ore, crushed)
        .unwrap_or_else(|error| panic!("manual processing fallback breaking start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("manual processing fallback breaking commit failed: {error}")
        });
    advance_exact(registries, &mut state, break_ticks);
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
        "manual processing hand-broken feed",
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
    validate_start_manual_constituent_separation(
        registries, &state, &sorting, crushed, native, residue,
    )
    .unwrap_or_else(|error| panic!("manual processing fallback sorting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual processing fallback sorting commit failed: {error}"));
    advance_exact(registries, &mut state, sort_ticks);
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
