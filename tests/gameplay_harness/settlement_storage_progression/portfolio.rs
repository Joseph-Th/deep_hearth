//! Single-state settlement workshop portfolio using one shared human-power bank.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_HELVE_HAMMER, EQUIPMENT_TIMBER_SPINDLE_DRILL, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    MANUAL_POWER_FOOT_TREADLE,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{PoweredCraftRequest, validate_start_powered_craft};
use deep_hearth::energy::{EnergySinkError, EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::labor::{ManualPowerError, ManualPowerRequest, validate_start_manual_power};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::assess_survival;

use super::{PORTFOLIO_BATCHES_PER_FAMILY, POWERED_FAMILIES, PoweredFamily};
use crate::environment::ROOM_TEMPERATURE;
use crate::manual_power_timing::finish_manual_power_work;
use crate::material_selection::select_stockpile_mass;
use crate::powered_craft_planning::{authored_batch, project_powered_craft_sequence};
use crate::production_timing::finish_uninterrupted_production_job;

#[derive(Clone, Copy)]
struct PortfolioFamilyRuntime {
    family: PoweredFamily,
    source: StockpileId,
    destination: StockpileId,
    machine: EquipmentId,
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
) -> (AppState, StockpileId, Vec<PendingPortfolioFamily>) {
    let mut state = AppState::new();
    let equipment_definitions = [
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
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
    (state, component_package, pending_families)
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
        let machine =
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
    outcome: &mut LivedPortfolioOutcome,
) {
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
            .get_equipment(runtime.machine)
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
                runtime.machine,
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
        finish_uninterrupted_production_job(registries, state, job, runtime.family.label);
        assert_eq!(
            state
                .equipment()
                .get_equipment(runtime.machine)
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
        "SETTLEMENT PORTFOLIO EXPERIENCE continuity=single-state provider=treadle shared-bank=timber-frame-flywheel shared-bank-concurrency=serialized-by-store-occupancy families={} batches-per-family={} productive-batches={} charge-events={} charge-attention={}t delegated={}t max-packed={} elapsed={}t survival=[energy:{}nJ hydration:{}uL] provider-condition={}ppm matter=conserved",
        POWERED_FAMILIES.len(),
        PORTFOLIO_BATCHES_PER_FAMILY,
        PORTFOLIO_BATCHES_PER_FAMILY * POWERED_FAMILIES.len() as u64,
        outcome.charge_events,
        outcome.charge_attention_ticks,
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
    let (mut state, component_package, pending_families) = seed_portfolio_world(registries);
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
    for runtime in families {
        execute_family(
            registries,
            &mut state,
            provider,
            drive,
            runtime,
            &mut outcome,
        );
    }

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
