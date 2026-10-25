//! Single-state settlement workshop portfolio using one shared human-power bank.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_HELVE_HAMMER, EQUIPMENT_TIMBER_SPINDLE_DRILL, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    FORM_DRAWPLATE, FORM_DRILL_BIT, MANUAL_POWER_FOOT_TREADLE, MATERIAL_STONE,
    PROCESS_DRESS_STONE_CHIP_DRILL_BIT, PROCESS_SHAPE_STONE_DRAWPLATE,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    ManualCraftStartRequest, PoweredCraftRequest, resolve_manual_craft,
    validate_start_manual_craft, validate_start_powered_craft,
};
use deep_hearth::energy::{EnergySinkError, EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{
    EquipmentId, player_equipment_maintenance_options, validate_assemble_equipment,
    validate_start_player_equipment_maintenance,
};
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile, project_material_stacks};
use deep_hearth::labor::{ManualPowerError, ManualPowerRequest, validate_start_manual_power};
use deep_hearth::logistics::validate_pickup_stack_from_ground;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::survival::assess_survival;

use super::{PORTFOLIO_BATCHES_PER_FAMILY, POWERED_FAMILIES, PoweredFamily};
use crate::environment::ROOM_TEMPERATURE;
use crate::manual_craft_selection::select_manual_craft_request;
use crate::manual_power_timing::finish_manual_power_work;
use crate::material_selection::select_stockpile_mass;
use crate::powered_craft_planning::{authored_batch, project_powered_craft_sequence};
use crate::production_timing::finish_uninterrupted_production_job;
use crate::tick_observation::{TickEventAllowance, assert_tick_events_within};

mod parallel;
use parallel::{
    execute_parallel_drill_bit_maintenance, prepare_next_machine_during_delegation,
    service_spindle_from_parallel_bit,
};

#[derive(Clone, Copy)]
struct PortfolioFamilyRuntime {
    family: PoweredFamily,
    source: StockpileId,
    destination: StockpileId,
    machine: Option<EquipmentId>,
}

#[derive(Clone, Copy)]
struct ParallelMaintenanceProject {
    source: StockpileId,
    destination: StockpileId,
    completed: bool,
}

#[derive(Clone, Copy)]
struct ParallelMachineProject {
    source: StockpileId,
    components: StockpileId,
    completed: bool,
}

#[derive(Clone, Copy)]
struct PendingPortfolioFamily {
    family: PoweredFamily,
    source: StockpileId,
    destination: StockpileId,
}

#[derive(Clone, Copy, Default)]
struct LivedPortfolioOutcome {
    charge_events: u64,
    charge_attention_ticks: u64,
    parallel_maintenance_attention_ticks: u64,
    parallel_maintenance_overlap_ticks: u64,
    parallel_machine_attention_ticks: u64,
    parallel_machine_overlap_ticks: u64,
    built_next_machine: bool,
    service_attention_ticks: u64,
    serviced_spindle_condition_before_ppm: u32,
    serviced_spindle_condition_after_ppm: u32,
    service_used_parallel_bit: bool,
    spindle_batches_after_service: u64,
    delegated_ticks: u64,
    maximum_leg_batches: u64,
    shared_bank_serialization_proven: bool,
}

fn total_batch_mass(batch_mass: Mass, batches: u64, context: &'static str) -> Mass {
    Mass::from_milligrams(
        batch_mass
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("{context} total batch mass overflowed")),
    )
}

fn seed_portfolio_world(
    registries: &Registries,
) -> (
    AppState,
    StockpileId,
    Vec<PendingPortfolioFamily>,
    ParallelMaintenanceProject,
    ParallelMachineProject,
) {
    let mut state = AppState::new();
    let equipment_definitions = [
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
    ];
    let component_package = crate::settlement_fixture::seed_inherited_workshop_package(
        registries,
        &mut state,
        &equipment_definitions,
        &[ENERGY_TIMBER_FRAME_FLYWHEEL_BANK],
        &[],
        "settlement lived shared-power portfolio",
    );

    let mut family_stockpiles = Vec::with_capacity(POWERED_FAMILIES.len() * 2);
    let mut pending_families = Vec::with_capacity(POWERED_FAMILIES.len());
    for family in POWERED_FAMILIES {
        let powered = registries
            .crafting()
            .get_powered(family.process)
            .unwrap_or_else(|| panic!("{} powered craft disappeared", family.label));
        let transform = registries
            .crafting()
            .get_manual(powered.transform())
            .unwrap_or_else(|| panic!("{} manual transform disappeared", family.label));
        let batch = authored_batch(registries, family.process, family.label);
        let project_mass =
            total_batch_mass(batch.input_mass, PORTFOLIO_BATCHES_PER_FAMILY, family.label);
        let source = seed_stockpile(
            &mut state,
            project_mass,
            StockpileStorageProfile::unbounded_solid_only(),
        );
        let destination = seed_stockpile(
            &mut state,
            project_mass,
            StockpileStorageProfile::unbounded_solid_only(),
        );
        let _ = seed_lot(
            registries,
            &mut state,
            source,
            transform.input(),
            project_mass,
            ROOM_TEMPERATURE,
        );
        family_stockpiles.extend([source, destination]);
        pending_families.push(PendingPortfolioFamily {
            family,
            source,
            destination,
        });
    }

    let maintenance = registries
        .crafting()
        .get_manual(PROCESS_DRESS_STONE_CHIP_DRILL_BIT)
        .unwrap_or_else(|| panic!("settlement portfolio drill-bit maintenance craft disappeared"));
    let maintenance_source = seed_stockpile(
        &mut state,
        maintenance.input_mass(),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let _ = seed_lot(
        registries,
        &mut state,
        maintenance_source,
        maintenance.input(),
        maintenance.input_mass(),
        ROOM_TEMPERATURE,
    );
    let maintenance_destination = seed_stockpile(
        &mut state,
        maintenance.input_mass(),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    family_stockpiles.extend([maintenance_source, maintenance_destination]);

    // The next production machine must be earned while the first one works, not included in an
    // already-complete starting workshop. Supply its other physical parts but leave its stone die
    // as raw feedstock requiring real craft attention and conserved process waste.
    let drawplate = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_STONE_DRAWPLATE)
        .unwrap_or_else(|| panic!("workshop drawplate fabrication definition disappeared"));
    let drawbench = registries
        .equipment()
        .get_equipment(EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH)
        .unwrap_or_else(|| panic!("workshop next-machine definition disappeared"));
    let assembly = drawbench
        .assembly_profile()
        .unwrap_or_else(|| panic!("workshop next machine lost its material assembly"));
    let drawplate_commodity = CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE);
    let assembled_mass = assembly
        .inputs()
        .iter()
        .map(|input| input.mass())
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("workshop next-machine material capacity overflowed"));
    let machine_parts = seed_stockpile(
        &mut state,
        assembled_mass
            .checked_add(drawplate.input_mass())
            .unwrap_or_else(|| panic!("workshop part stockpile capacity overflowed")),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in assembly.inputs() {
        if input.commodity() == drawplate_commodity {
            continue;
        }
        let _ = seed_lot(
            registries,
            &mut state,
            machine_parts,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let machine_raw = seed_stockpile(
        &mut state,
        drawplate.input_mass(),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let _ = seed_lot(
        registries,
        &mut state,
        machine_raw,
        drawplate.input(),
        drawplate.input_mass(),
        ROOM_TEMPERATURE,
    );
    family_stockpiles.extend([machine_parts, machine_raw]);

    let mut admitted_stockpiles = Vec::with_capacity(1 + family_stockpiles.len());
    admitted_stockpiles.push(component_package);
    admitted_stockpiles.extend(family_stockpiles);
    crate::world_admission::admit_stationary_player(
        registries,
        &mut state,
        &admitted_stockpiles,
        &[],
        "settlement lived shared-power portfolio",
    );
    (
        state,
        component_package,
        pending_families,
        ParallelMaintenanceProject {
            source: maintenance_source,
            destination: maintenance_destination,
            completed: false,
        },
        ParallelMachineProject {
            source: machine_raw,
            components: machine_parts,
            completed: false,
        },
    )
}

fn assemble_portfolio_infrastructure(
    registries: &Registries,
    state: &mut AppState,
    component_package: StockpileId,
    pending_families: Vec<PendingPortfolioFamily>,
) -> (EquipmentId, EnergyStoreId, Vec<PortfolioFamilyRuntime>) {
    let provider = validate_assemble_equipment(
        registries,
        &state,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        component_package,
    )
    .unwrap_or_else(|error| panic!("settlement portfolio treadle assembly failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("settlement portfolio treadle commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        registries,
        &state,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        component_package,
    )
    .unwrap_or_else(|error| panic!("settlement portfolio bank assembly failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("settlement portfolio bank commit failed: {error}"));

    let mut families = Vec::with_capacity(POWERED_FAMILIES.len());
    for pending in pending_families {
        let family = pending.family;
        let machine = (family.equipment != EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH).then(|| {
            validate_assemble_equipment(registries, &state, family.equipment, component_package)
                .unwrap_or_else(|error| {
                    panic!(
                        "settlement portfolio {} assembly failed: {error}",
                        family.label
                    )
                })
                .commit(state)
                .unwrap_or_else(|error| {
                    panic!(
                        "settlement portfolio {} commit failed: {error}",
                        family.label
                    )
                })
        });
        families.push(PortfolioFamilyRuntime {
            family,
            source: pending.source,
            destination: pending.destination,
            machine,
        });
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(component_package)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "settlement lived portfolio must embody its complete disclosed infrastructure package"
    );
    (provider, drive, families)
}

fn prove_shared_bank_serialization(
    registries: &Registries,
    state: &AppState,
    provider: EquipmentId,
    drive: EnergyStoreId,
    blocking_job: deep_hearth::production::ProductionJobId,
) {
    match validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(
            MANUAL_POWER_FOOT_TREADLE,
            provider,
            drive,
            Energy::from_nanojoules(1),
        ),
    ) {
        Err(ManualPowerError::EnergySink(EnergySinkError::StoreBusy { store, job, .. })) => {
            assert_eq!(store, drive);
            assert_eq!(job, blocking_job);
        }
        Err(error) => {
            panic!("settlement shared bank concurrency probe failed for the wrong reason: {error}")
        }
        Ok(_) => {
            panic!("settlement shared bank accepted recharge while still reserved by production")
        }
    }
}

fn execute_family(
    registries: &Registries,
    state: &mut AppState,
    provider: EquipmentId,
    drive: EnergyStoreId,
    runtime: PortfolioFamilyRuntime,
    parallel_maintenance: &mut ParallelMaintenanceProject,
    parallel_machine: &mut ParallelMachineProject,
    outcome: &mut LivedPortfolioOutcome,
) {
    let machine = runtime
        .machine
        .unwrap_or_else(|| panic!("production equipment must be built before its family starts"));
    let batch = authored_batch(registries, runtime.family.process, runtime.family.label);
    let mut completed_batches = 0_u64;
    while completed_batches < PORTFOLIO_BATCHES_PER_FAMILY {
        assert_eq!(
            state.energy().get_store(drive).map(|store| store.stored()),
            Some(Energy::ZERO),
            "settlement shared bank must begin each productive leg empty"
        );
        let machine_condition = state
            .equipment()
            .get_equipment(machine)
            .map(|record| record.condition())
            .unwrap_or_else(|| panic!("{} machine disappeared", runtime.family.label));
        let remaining = PORTFOLIO_BATCHES_PER_FAMILY
            .checked_sub(completed_batches)
            .unwrap_or_else(|| unreachable!("settlement portfolio completed too many batches"));
        let sequence = project_powered_craft_sequence(
            registries,
            &state,
            runtime.family.process,
            runtime.family.equipment,
            machine_condition,
            drive,
            batch,
            remaining,
            runtime.family.label,
        );
        let leg = *sequence
            .legs
            .first()
            .unwrap_or_else(|| panic!("{} lost a feasible shared-bank leg", runtime.family.label));

        let power = validate_start_manual_power(
            registries,
            &state,
            ManualPowerRequest::new(MANUAL_POWER_FOOT_TREADLE, provider, drive, leg.work),
        )
        .unwrap_or_else(|error| {
            panic!(
                "{} shared-bank charge failed: {error}",
                runtime.family.label
            )
        })
        .commit(state)
        .unwrap_or_else(|error| {
            panic!(
                "{} shared-bank charge commit failed: {error}",
                runtime.family.label
            )
        });
        outcome.charge_attention_ticks = outcome
            .charge_attention_ticks
            .checked_add(finish_manual_power_work(
                registries,
                state,
                power,
                runtime.family.label,
            ))
            .unwrap_or_else(|| panic!("settlement portfolio charge attention overflowed"));
        outcome.charge_events = outcome
            .charge_events
            .checked_add(1)
            .unwrap_or_else(|| panic!("settlement portfolio charge count overflowed"));

        let selections =
            select_stockpile_mass(&state, runtime.source, leg.input_mass, runtime.family.label);
        let job = validate_start_powered_craft(
            registries,
            &state,
            PoweredCraftRequest::new(
                runtime.family.process,
                runtime.source,
                selections,
                machine,
                drive,
            ),
            runtime.destination,
        )
        .unwrap_or_else(|error| {
            panic!(
                "{} shared-bank work start failed: {error}",
                runtime.family.label
            )
        })
        .commit(state)
        .unwrap_or_else(|error| {
            panic!(
                "{} shared-bank work commit failed: {error}",
                runtime.family.label
            )
        });
        let delegated = state
            .production()
            .get_job(job)
            .map(|record| record.active_duration().value())
            .unwrap_or_else(|| panic!("{} shared-bank job disappeared", runtime.family.label));
        assert_eq!(delegated, leg.delegated_ticks);
        if !outcome.shared_bank_serialization_proven {
            prove_shared_bank_serialization(registries, state, provider, drive, job);
            outcome.shared_bank_serialization_proven = true;
        }
        outcome.delegated_ticks = outcome
            .delegated_ticks
            .checked_add(delegated)
            .unwrap_or_else(|| panic!("settlement portfolio delegated time overflowed"));
        if runtime.family.equipment == EQUIPMENT_TIMBER_SPINDLE_DRILL
            && !parallel_maintenance.completed
        {
            let overlap = execute_parallel_drill_bit_maintenance(
                registries,
                state,
                job,
                delegated,
                parallel_maintenance,
            );
            outcome.parallel_maintenance_attention_ticks = overlap;
            outcome.parallel_maintenance_overlap_ticks = overlap;
            let next_machine_attention =
                prepare_next_machine_during_delegation(registries, state, job, parallel_machine);
            outcome.parallel_machine_attention_ticks = next_machine_attention;
            outcome.parallel_machine_overlap_ticks = next_machine_attention;
        }
        finish_uninterrupted_production_job(registries, state, job, runtime.family.label);
        assert_eq!(
            state
                .equipment()
                .get_equipment(machine)
                .map(|record| record.condition()),
            Some(leg.condition_after),
            "{} wear diverged from the live packed-leg projection",
            runtime.family.label
        );
        assert_eq!(
            state.energy().get_store(drive).map(|store| store.stored()),
            Some(Energy::ZERO),
            "{} productive leg must consume its full shared-bank charge",
            runtime.family.label
        );
        completed_batches = completed_batches
            .checked_add(leg.batches)
            .unwrap_or_else(|| panic!("settlement portfolio batch count overflowed"));
        if runtime.family.equipment == EQUIPMENT_TIMBER_SPINDLE_DRILL
            && parallel_maintenance.completed
            && !outcome.service_used_parallel_bit
        {
            outcome.spindle_batches_after_service = PORTFOLIO_BATCHES_PER_FAMILY
                .checked_sub(completed_batches)
                .unwrap_or_else(|| unreachable!("completed spindle batches are bounded"));
            // Only evaluate the premature repair after the last spindle batch: all useful
            // production in this order is already finished. Retain the prepared spare in the
            // actual actor state and test repair in a matched, fully material-backed branch.
            assert_eq!(outcome.spindle_batches_after_service, 0);
            let mut matched = state.clone();
            service_spindle_from_parallel_bit(
                registries,
                &mut matched,
                machine,
                parallel_maintenance,
                outcome,
            );
            assert_eq!(
                calculate_matter_accounting(&matched).map(|total| total.total()),
                calculate_matter_accounting(state).map(|total| total.total()),
                "matched service must preserve matter and cannot modify original player state",
            );
        }
        outcome.maximum_leg_batches = outcome.maximum_leg_batches.max(leg.batches);
    }
}

fn report_lived_portfolio(
    registries: &Registries,
    state: &AppState,
    provider: EquipmentId,
    outcome: LivedPortfolioOutcome,
    started_at: u64,
    survival_before: deep_hearth::survival::SurvivalAssessment,
) {
    let survival_after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("settlement lived portfolio lost player survival after work"));
    let provider_condition = state
        .equipment()
        .get_equipment(provider)
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("settlement lived portfolio treadle disappeared"));
    reviewln!(
        "SETTLEMENT PORTFOLIO EXPERIENCE continuity=single-state provider=treadle shared-bank=timber-frame-flywheel shared-bank-concurrency=serialized-by-store-occupancy families={} batches-per-family={} productive-batches={} charge-events={} charge-attention={}t parallel-maintenance=[component:stone-drill-bit attention:{}t overlap:{}t completed:{}] next-machine=[component:drawplate attention:{}t overlap:{}t built:{} used-for:wire-drawbench] service-option=[choice:defer-spare matched-bit-used:{} spindle-condition:{}->{}ppm matched-service:{}t pending-batches:{}] delegated={}t max-packed={} elapsed={}t survival=[energy:{}nJ hydration:{}uL] provider-condition={}ppm matter=conserved",
        POWERED_FAMILIES.len(),
        PORTFOLIO_BATCHES_PER_FAMILY,
        PORTFOLIO_BATCHES_PER_FAMILY * POWERED_FAMILIES.len() as u64,
        outcome.charge_events,
        outcome.charge_attention_ticks,
        outcome.parallel_maintenance_attention_ticks,
        outcome.parallel_maintenance_overlap_ticks,
        outcome.parallel_maintenance_attention_ticks > 0,
        outcome.parallel_machine_attention_ticks,
        outcome.parallel_machine_overlap_ticks,
        outcome.built_next_machine,
        outcome.service_used_parallel_bit,
        outcome.serviced_spindle_condition_before_ppm,
        outcome.serviced_spindle_condition_after_ppm,
        outcome.service_attention_ticks,
        outcome.spindle_batches_after_service,
        outcome.delegated_ticks,
        outcome.maximum_leg_batches,
        state
            .tick()
            .value()
            .checked_sub(started_at)
            .unwrap_or_else(|| unreachable!("settlement lived portfolio cannot run backward")),
        survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy())
            .unwrap_or_else(|| panic!("settlement lived portfolio metabolic reserve increased"))
            .nanojoules(),
        survival_before
            .hydration()
            .checked_sub(survival_after.hydration())
            .unwrap_or_else(|| panic!("settlement lived portfolio hydration reserve increased"))
            .microliters(),
        provider_condition,
    );
}

pub(super) fn execute_lived_portfolio(registries: &Registries) -> u64 {
    let (
        mut state,
        component_package,
        pending_families,
        mut parallel_maintenance,
        mut parallel_machine,
    ) = seed_portfolio_world(registries);
    let (provider, drive, families) = assemble_portfolio_infrastructure(
        registries,
        &mut state,
        component_package,
        pending_families,
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("settlement lived portfolio matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("settlement lived portfolio lost player survival before work"));
    let started_at = state.tick().value();
    let mut outcome = LivedPortfolioOutcome::default();
    for mut runtime in families {
        if runtime.family.equipment == EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH {
            assert!(
                parallel_machine.completed,
                "the second powered machine must await the player's parallel-prepared drawplate"
            );
            assert!(
                runtime.machine.is_none(),
                "unprepared future machinery must not be inherited as ready-to-run"
            );
            let machine = validate_assemble_equipment(
                registries,
                &state,
                EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
                parallel_machine.components,
            )
            .unwrap_or_else(|error| {
                panic!("parallel-prepared drawbench assembly rejected: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("parallel-prepared drawbench assembly failed: {error}"));
            runtime.machine = Some(machine);
            outcome.built_next_machine = true;
            assert_eq!(
                state
                    .inventory()
                    .get_stockpile(parallel_machine.components)
                    .map(|pile| pile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE))),
                Some(Mass::ZERO),
                "new drawbench must consume the parallel-prepared stone die, not leave a duplicate part",
            );
        }
        execute_family(
            registries,
            &mut state,
            provider,
            drive,
            runtime,
            &mut parallel_maintenance,
            &mut parallel_machine,
            &mut outcome,
        );
    }

    assert!(
        parallel_maintenance.completed
            && outcome.service_used_parallel_bit
            && parallel_machine.completed
            && outcome.built_next_machine,
        "settlement lived portfolio must prove the material-backed service alternative without executing premature repair"
    );

    assert_eq!(
        outcome.charge_events,
        super::project_portfolio_cadence(registries, &state, drive, "lived bank").charge_events,
        "lived shared-bank cadence diverged from the final-state portfolio projection"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "settlement lived portfolio matter audit failed: {error}"
            ))
            .total(),
        matter_before,
        "settlement lived portfolio must conserve matter"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("settlement lived portfolio final state invalid: {error}"));
    assert!(outcome.shared_bank_serialization_proven);
    report_lived_portfolio(
        registries,
        &state,
        provider,
        outcome,
        started_at,
        survival_before,
    );
    outcome.charge_events
}
