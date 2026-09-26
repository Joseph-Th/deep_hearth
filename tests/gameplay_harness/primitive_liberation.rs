//! Ordinary primitive liberation episode shared by progression gates and exploratory reports.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::gameplay_fixture::seed_composed_lot;
use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_THERMAL_SINK,
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_STONE_CRUSHER,
    EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_STONE_SEPARATOR, EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
    EQUIPMENT_TIMBER_TREADLE_DRIVE, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_TREADLE_DYNAMO, MANUAL_POWER_WALKING_WHEEL, MATERIAL_COPPER,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{EnergyCarrier, EnergyStoreId};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::{MaterialLotId, StockpileId};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::resolve_representable_screening_mass;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::inventory_support::add_solid_stockpile;
use super::manual_ore_recovery::{ManualOreRecoveryPlan, evaluate_manual_ore_recovery};
use super::ore_fixture::copper_ore_composition;
use super::seed::mix64;

#[path = "primitive_liberation/acquisition.rs"]
mod acquisition;
#[path = "primitive_liberation/cleanup.rs"]
mod cleanup;
#[path = "primitive_liberation/comparison.rs"]
mod comparison;
#[path = "primitive_liberation/primary.rs"]
mod primary;
#[path = "primitive_liberation/scavenging.rs"]
mod scavenging;
#[path = "primitive_liberation/support.rs"]
mod support;

const PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES: u64 = 8;

#[derive(Clone, Copy)]
struct PrimitiveLiberationBootstrap {
    ore: StockpileId,
    crushed: StockpileId,
    ground: StockpileId,
    undersize: StockpileId,
    oversize: StockpileId,
    concentrate: StockpileId,
    tailings: StockpileId,
    fine_tailings: StockpileId,
    exhausted_tailings: StockpileId,
    native_copper: StockpileId,
    manual_crushed: StockpileId,
    manual_native: StockpileId,
    manual_residue: StockpileId,
    ore_lot: MaterialLotId,
}

fn bootstrap_liberation_inventory(
    registries: &Registries,
    state: &mut AppState,
    batch_mass: Mass,
    copper_ppm: u32,
    clay_share_ppm: u32,
) -> PrimitiveLiberationBootstrap {
    let ore = add_solid_stockpile(state, batch_mass);
    let crushed = add_solid_stockpile(state, batch_mass);
    let ground = add_solid_stockpile(state, batch_mass);
    let undersize = add_solid_stockpile(state, batch_mass);
    let oversize = add_solid_stockpile(state, batch_mass);
    let concentrate = add_solid_stockpile(state, batch_mass);
    let tailings = add_solid_stockpile(state, batch_mass);
    let fine_tailings = add_solid_stockpile(state, batch_mass);
    let exhausted_tailings = add_solid_stockpile(state, batch_mass);
    let native_copper = add_solid_stockpile(state, batch_mass);
    let manual_crushed = add_solid_stockpile(state, batch_mass);
    let manual_native = add_solid_stockpile(state, batch_mass);
    let manual_residue = add_solid_stockpile(state, batch_mass);
    let ore_lot = seed_composed_lot(
        registries,
        state,
        ore,
        CommodityKey::new(MATERIAL_COPPER, deep_hearth::content::FORM_ORE),
        batch_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(copper_ppm, clay_share_ppm),
    );
    PrimitiveLiberationBootstrap {
        ore,
        crushed,
        ground,
        undersize,
        oversize,
        concentrate,
        tailings,
        fine_tailings,
        exhausted_tailings,
        native_copper,
        manual_crushed,
        manual_native,
        manual_residue,
        ore_lot,
    }
}

#[derive(Clone)]
struct PrimitiveLiberationScenario {
    charge_policy: support::ChargePolicy,
    charges: Vec<support::ChargeReport>,
    state: AppState,
    batch_mass: Mass,
    copper_ppm: u32,
    ore: deep_hearth::inventory::StockpileId,
    crushed: deep_hearth::inventory::StockpileId,
    ground: deep_hearth::inventory::StockpileId,
    undersize: deep_hearth::inventory::StockpileId,
    oversize: deep_hearth::inventory::StockpileId,
    concentrate: deep_hearth::inventory::StockpileId,
    tailings: deep_hearth::inventory::StockpileId,
    fine_tailings: deep_hearth::inventory::StockpileId,
    exhausted_tailings: deep_hearth::inventory::StockpileId,
    native_copper: deep_hearth::inventory::StockpileId,
    ore_lot: deep_hearth::inventory::MaterialLotId,
    crusher: EquipmentId,
    quern: EquipmentId,
    screen: EquipmentId,
    separator: EquipmentId,
    treadle: EquipmentId,
    drive: EnergyStoreId,
}

#[derive(Debug)]
struct PrimitiveLiberationCampaignLifecycle {
    batch_charge_ticks: Vec<u64>,
    elapsed_ticks: u64,
    metabolic_cost_nj: u128,
    hydration_cost_ul: u64,
    crusher_condition_ppm: u32,
    quern_condition_ppm: u32,
    screen_condition_ppm: u32,
    separator_condition_ppm: u32,
    treadle_condition_ppm: u32,
}

fn run_powered_campaign_lifecycle(
    registries: &Registries,
    mut state: AppState,
    batches: &[PrimitiveLiberationBootstrap],
    crusher: EquipmentId,
    quern: EquipmentId,
    screen: EquipmentId,
    separator: EquipmentId,
    treadle: EquipmentId,
    drive: EnergyStoreId,
) -> PrimitiveLiberationCampaignLifecycle {
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("liberation campaign matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation campaign player disappeared before execution"));
    let started_at = state.tick().value();
    let mut batch_charge_ticks = Vec::with_capacity(batches.len());

    for bootstrap in batches {
        let batch_mass = state_mass(bootstrap, &state);
        let copper_ppm = state
            .inventory()
            .get_lot(bootstrap.ore_lot)
            .map(|lot| lot.composition().parts_per_million(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("liberation campaign ore lot disappeared"));
        let mut scenario = PrimitiveLiberationScenario {
            charge_policy: support::ChargePolicy::BatchDemand,
            charges: Vec::new(),
            state,
            batch_mass,
            copper_ppm,
            ore: bootstrap.ore,
            crushed: bootstrap.crushed,
            ground: bootstrap.ground,
            undersize: bootstrap.undersize,
            oversize: bootstrap.oversize,
            concentrate: bootstrap.concentrate,
            tailings: bootstrap.tailings,
            fine_tailings: bootstrap.fine_tailings,
            exhausted_tailings: bootstrap.exhausted_tailings,
            native_copper: bootstrap.native_copper,
            ore_lot: bootstrap.ore_lot,
            crusher,
            quern,
            screen,
            separator,
            treadle,
            drive,
        };
        let primary = primary::run(registries, &mut scenario);
        let scavenged = scavenging::run(registries, &mut scenario, &primary);
        let _cleaned = cleanup::run(registries, &mut scenario);
        batch_charge_ticks.push(
            scenario
                .charges
                .iter()
                .try_fold(0_u64, |total, charge| total.checked_add(charge.ticks))
                .unwrap_or_else(|| panic!("liberation campaign charge attention overflowed")),
        );
        assert!(
            !scavenged.concentrate_mass.is_zero(),
            "liberation campaign batch must retain a cleanup feed"
        );
        state = scenario.state;
    }

    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("liberation campaign matter audit failed: {error}"))
            .total(),
        matter_before,
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("liberation campaign final state invalid: {error}"));
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation campaign player disappeared after execution"));
    let condition = |equipment| {
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition().parts_per_million())
            .unwrap_or_else(|| panic!("liberation campaign equipment disappeared"))
    };
    PrimitiveLiberationCampaignLifecycle {
        batch_charge_ticks,
        elapsed_ticks: state.tick().value() - started_at,
        metabolic_cost_nj: survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy())
            .unwrap_or_else(|| panic!("liberation campaign metabolic reserve increased"))
            .nanojoules(),
        hydration_cost_ul: survival_before
            .hydration()
            .checked_sub(survival_after.hydration())
            .unwrap_or_else(|| panic!("liberation campaign hydration reserve increased"))
            .microliters(),
        crusher_condition_ppm: condition(crusher),
        quern_condition_ppm: condition(quern),
        screen_condition_ppm: condition(screen),
        separator_condition_ppm: condition(separator),
        treadle_condition_ppm: condition(treadle),
    }
}

fn state_mass(bootstrap: &PrimitiveLiberationBootstrap, state: &AppState) -> Mass {
    state
        .inventory()
        .get_lot(bootstrap.ore_lot)
        .map(|lot| lot.mass())
        .unwrap_or_else(|| panic!("liberation campaign ore lot disappeared"))
}

pub(super) fn run_primitive_liberation_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let requested_batch_mass =
        Mass::from_milligrams(80_000 + mix64(seed ^ 0x4C49_4245_5241_5445) % 40_001);
    let grinding = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation grinding definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation screening definition disappeared"));
    let batch_mass = resolve_representable_screening_mass(
        screening,
        grinding.output_particle_size_distribution(),
        requested_batch_mass,
    )
    .unwrap_or_else(|error| panic!("primitive liberation batch planning failed: {error}"));
    assert!(
        !batch_mass.is_zero(),
        "primitive liberation generated no representable batch"
    );
    let copper_ppm = 300_000 + (mix64(seed ^ 0x4C49_4245_5243_5550) % 300_001) as u32;
    let clay_share_ppm = (mix64(seed ^ 0x4C49_4245_5243_4C41) % 650_001) as u32;
    let (
        state,
        crusher,
        quern,
        screen,
        separator,
        treadle,
        drive,
        kit_acquisition,
        bootstrap,
        campaign_lifecycle,
    ) = if case.role() == FocusedProbeRole::MaintainedAnchor {
        let (acquired, campaign_bootstraps) = acquisition::acquire_raw_kit(
            registries,
            seed,
            PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES,
            |state| {
                (0..PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES)
                    .map(|_| {
                        bootstrap_liberation_inventory(
                            registries,
                            state,
                            batch_mass,
                            copper_ppm,
                            clay_share_ppm,
                        )
                    })
                    .collect::<Vec<_>>()
            },
        );
        let bootstrap = *campaign_bootstraps
            .first()
            .unwrap_or_else(|| panic!("liberation campaign lost its first batch"));
        let campaign_lifecycle = run_powered_campaign_lifecycle(
            registries,
            acquired.state.clone(),
            &campaign_bootstraps,
            acquired.crusher,
            acquired.quern,
            acquired.screen,
            acquired.separator,
            acquired.treadle,
            acquired.drive,
        );
        (
            acquired.state,
            acquired.crusher,
            acquired.quern,
            acquired.screen,
            acquired.separator,
            acquired.treadle,
            acquired.drive,
            Some(acquired.review),
            bootstrap,
            Some(campaign_lifecycle),
        )
    } else {
        let mut state = AppState::new();
        let crusher = support::assemble_equipment_from_authored_parts(
            registries,
            &mut state,
            EQUIPMENT_STONE_CRUSHER,
        );
        let quern = support::assemble_equipment_from_authored_parts(
            registries,
            &mut state,
            EQUIPMENT_STONE_ROTARY_QUERN,
        );
        let screen = support::assemble_equipment_from_authored_parts(
            registries,
            &mut state,
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
        );
        let separator = support::assemble_equipment_from_authored_parts(
            registries,
            &mut state,
            EQUIPMENT_STONE_SEPARATOR,
        );
        let treadle = support::assemble_equipment_from_authored_parts(
            registries,
            &mut state,
            EQUIPMENT_TIMBER_TREADLE_DRIVE,
        );
        let drive = support::assemble_energy_store_from_authored_parts(
            registries,
            &mut state,
            ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
        );
        let bootstrap = bootstrap_liberation_inventory(
            registries,
            &mut state,
            batch_mass,
            copper_ppm,
            clay_share_ppm,
        );
        initialize_player_survival(registries, &mut state)
            .unwrap_or_else(|error| panic!("primitive liberation survival setup failed: {error}"));
        (
            state, crusher, quern, screen, separator, treadle, drive, None, bootstrap, None,
        )
    };
    let PrimitiveLiberationBootstrap {
        ore,
        crushed,
        ground,
        undersize,
        oversize,
        concentrate,
        tailings,
        fine_tailings,
        exhausted_tailings,
        native_copper,
        manual_crushed,
        manual_native,
        manual_residue,
        ore_lot,
    } = bootstrap;
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("paired primitive drive disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("primitive liberation matter setup failed: {error}"))
        .total();
    let manual_recovery = evaluate_manual_ore_recovery(
        registries,
        &state,
        ManualOreRecoveryPlan {
            ore_source: ore,
            crushed_destination: manual_crushed,
            native_destination: manual_native,
            residue_destination: manual_residue,
            feed_mass: batch_mass,
        },
    );
    assert_eq!(
        manual_recovery.feed_mass, batch_mass,
        "primitive liberation manual comparison must use the exact powered feed mass"
    );

    let mut scenario = PrimitiveLiberationScenario {
        charge_policy: support::ChargePolicy::BatchDemand,
        charges: Vec::new(),
        state,
        batch_mass,
        copper_ppm,
        ore,
        crushed,
        ground,
        undersize,
        oversize,
        concentrate,
        tailings,
        fine_tailings,
        exhausted_tailings,
        native_copper,
        ore_lot,
        crusher,
        quern,
        screen,
        separator,
        treadle,
        drive,
    };
    let mut full_buffer = scenario.clone();
    full_buffer.charge_policy = support::ChargePolicy::FullBuffer;
    let started_at = scenario.state.tick().value();
    let primary = primary::run(registries, &mut scenario);
    let primary_completed_at = scenario.state.tick().value();
    let mut direct_cleanup = scenario.clone();
    let direct_cleaned = cleanup::run(registries, &mut direct_cleanup);
    let scavenged = scavenging::run(registries, &mut scenario, &primary);
    let scavenger_completed_at = scenario.state.tick().value();
    let cleaned = cleanup::run(registries, &mut scenario);
    let baseline_primary = primary::run(registries, &mut full_buffer);
    let baseline_scavenged = scavenging::run(registries, &mut full_buffer, &baseline_primary);
    let baseline_cleaned = cleanup::run(registries, &mut full_buffer);
    assert_eq!(
        primary, baseline_primary,
        "charging policy must preserve primary recovery"
    );
    assert_eq!(
        scavenged, baseline_scavenged,
        "charging policy must preserve scavenger recovery"
    );
    assert_eq!(
        cleaned, baseline_cleaned,
        "charging policy must preserve final concentrate cleanup"
    );
    comparison::review(
        registries,
        seed,
        comparison::LiberationComparison {
            started_at,
            primary_completed_at,
            scavenger_completed_at,
            demand: &scenario,
            full: &full_buffer,
            scavenged: &scavenged,
            cleaned: &cleaned,
            direct_cleanup: &direct_cleanup,
            direct_cleaned: &direct_cleaned,
            manual_recovery: &manual_recovery,
            kit_acquisition: kit_acquisition.as_ref(),
            campaign_lifecycle: campaign_lifecycle.as_ref(),
            planned_batches: PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES,
        },
    );
    let state = &scenario.state;
    assert!(
        state
            .energy()
            .get_store(drive)
            .is_some_and(|record| record.stored() < drive_capacity)
    );
    for equipment in [crusher, quern, screen, separator] {
        assert!(
            state
                .equipment()
                .get_equipment(equipment)
                .is_some_and(|record| record.condition() < Condition::PRISTINE),
            "every primitive liberation machine must incur real operation wear"
        );
    }
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!("primitive liberation matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("primitive liberation trusted-state audit failed: {error}"));
    let final_energy = state
        .energy()
        .get_store(drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("primitive liberation drive disappeared after completion"));
    let recovered_copper_ppm_mg = primary
        .concentrate_copper_ppm_mg
        .checked_add(scavenged.additional_recovered_copper_ppm_mg)
        .unwrap_or_else(|| panic!("primitive liberation recovered-copper audit overflowed"));
    let native_copper_mass = state
        .inventory()
        .get_stockpile(native_copper)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive native-copper stockpile disappeared"));
    assert_eq!(native_copper_mass, cleaned.native_copper_mass);
    // Use exact constituent numerators, not rounded concentrate grades. Keep any fractional
    // milligrams as decimal digits without floating point or truncating represented copper.
    let copper_mg = |numerator: u128| {
        let whole = numerator / 1_000_000;
        let fraction = numerator % 1_000_000;
        if fraction == 0 {
            whole.to_string()
        } else {
            let digits = format!("{fraction:06}");
            format!("{whole}.{}", digits.trim_end_matches('0'))
        }
    };
    let furnace = registries
        .equipment()
        .get_equipment(EQUIPMENT_ELECTRIC_FURNACE)
        .unwrap_or_else(|| panic!("foundry frontier furnace definition disappeared"));
    let mold = registries
        .equipment()
        .get_equipment(EQUIPMENT_CASTING_MOLD)
        .unwrap_or_else(|| panic!("foundry frontier mold definition disappeared"));
    let electrical_buffer = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("foundry frontier electrical buffer definition disappeared"));
    let thermal_sink = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .unwrap_or_else(|| panic!("foundry frontier thermal sink definition disappeared"));
    let manual_electrical_generation = [
        MANUAL_POWER_HAND_CRANK,
        MANUAL_POWER_FOOT_TREADLE,
        MANUAL_POWER_WALKING_WHEEL,
        MANUAL_POWER_TREADLE_DYNAMO,
    ]
    .into_iter()
    .filter_map(|method| registries.labor().get_manual_power(method))
    .any(|method| method.carrier() == EnergyCarrier::Electrical);
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("foundry frontier copper melting definition disappeared"));
    let furnace_heating_power = match furnace
        .capabilities()
        .get_capability(melting.heating_power_capability())
    {
        Some(CapabilityValue::Power(power)) => power,
        Some(other) => panic!(
            "foundry furnace heating capability changed physical kind: {:?}",
            other.kind()
        ),
        None => panic!("foundry furnace lost its authored heating capability"),
    };
    let maximum_manual_electrical_power = [
        MANUAL_POWER_HAND_CRANK,
        MANUAL_POWER_FOOT_TREADLE,
        MANUAL_POWER_WALKING_WHEEL,
        MANUAL_POWER_TREADLE_DYNAMO,
    ]
    .into_iter()
    .filter_map(|method| registries.labor().get_manual_power(method))
    .filter(|method| method.carrier() == EnergyCarrier::Electrical)
    .flat_map(|method| {
        registries
            .equipment()
            .definitions()
            .filter_map(move |equipment| {
                match equipment
                    .capabilities()
                    .get_capability(method.power_capability())
                {
                    Some(CapabilityValue::Power(power)) => Some(power),
                    Some(_) | None => None,
                }
            })
    })
    .max()
    .unwrap_or_else(|| panic!("ordinary manual electrical power has no authored provider"));
    let manual_microwatts = maximum_manual_electrical_power
        .whole_microwatts()
        .unwrap_or_else(|| panic!("manual electrical frontier power is not a whole microwatt"));
    let furnace_microwatts = furnace_heating_power
        .whole_microwatts()
        .unwrap_or_else(|| panic!("furnace heating frontier power is not a whole microwatt"));
    let transfer_ceiling_ratio = furnace_heating_power
        .picowatts()
        .checked_div(maximum_manual_electrical_power.picowatts())
        .unwrap_or_else(|| panic!("manual electrical frontier power unexpectedly vanished"));
    let foundry_frontier = format!(
        "assembly-edge=[furnace:{} mold:{} electrical-buffer:{} thermal-sink:{}] manual-electrical-generation:{} support-required=[furnace:{} mold:{}] energy-scale=[manual-electrical-max:{}uW industrial-furnace-transfer-ceiling:{}uW ceiling-ratio:{}x melting-carrier:{:?} conversion-path:present]",
        furnace.assembly_profile().is_some(),
        mold.assembly_profile().is_some(),
        electrical_buffer.assembly_profile().is_some(),
        thermal_sink.assembly_profile().is_some(),
        manual_electrical_generation,
        furnace.requires_structural_support(),
        mold.requires_structural_support(),
        manual_microwatts,
        furnace_microwatts,
        transfer_ceiling_ratio,
        melting.energy_carrier(),
    );
    reviewln!(
        "LIBERATION FRONTIER CAPABILITY seed=0x{seed:016X} sample={} cleanup-executed=true reason=required-native-copper-conversion route=treadle+paired-flywheel->crusher->quern->timber-riddle->regrind->separator->tailings-regrind->scavenger->concentrate-cleanup input=[{}mg {}ppm-Cu clay-share:{}ppm] concentrate=[first:{}mg/{}ppm final:{}mg/{}ppm] copper-in-concentrate=[first:{}mg final:{}mg scavenger-recovered:{}mg] native-copper={}mg cleanup-residue={}mg exhausted-tailings={}mg stored-work-remaining={}nJ machinery-worn=true matter=conserved",
        focused_probe_role_label(case.role()),
        batch_mass.milligrams(),
        copper_ppm,
        clay_share_ppm,
        primary.concentrate_mass.milligrams(),
        primary.concentrate_grade_ppm,
        scavenged.concentrate_mass.milligrams(),
        scavenged.concentrate_grade_ppm,
        copper_mg(primary.concentrate_copper_ppm_mg),
        copper_mg(recovered_copper_ppm_mg),
        copper_mg(scavenged.additional_recovered_copper_ppm_mg),
        cleaned.native_copper_mass.milligrams(),
        cleaned.cleanup_residue_mass.milligrams(),
        cleaned.exhausted_tailings_mass.milligrams(),
        final_energy.nanojoules(),
    );
    let scavenger_share_ppm = scavenged
        .additional_recovered_copper_ppm_mg
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("primitive liberation scavenger-share audit overflowed"))
        .checked_div(recovered_copper_ppm_mg)
        .unwrap_or(0)
        .min(1_000_000);
    reviewln!(
        "LIBERATION FRONTIER seed=0x{seed:016X} sample={} input=[{}mg {}ppm-Cu] concentrate=[final:{}mg/{}ppm] scavenger=[extra-copper:{}mg share:{}ppm-of-recovered-copper] cleanup=[native-copper:{}mg recovery:{}ppm residue:{}mg] sink=usable-native-copper remaining-frontier=industrial-foundry-scale industrial-foundry-frontier=[{}] reachability-authority=STATUS.md",
        focused_probe_role_label(case.role()),
        batch_mass.milligrams(),
        copper_ppm,
        scavenged.concentrate_mass.milligrams(),
        scavenged.concentrate_grade_ppm,
        scavenged.additional_recovered_copper_ppm_mg / 1_000_000,
        scavenger_share_ppm,
        cleaned.native_copper_mass.milligrams(),
        cleaned.target_recovery_ppm,
        cleaned.cleanup_residue_mass.milligrams(),
        foundry_frontier,
    );
}
