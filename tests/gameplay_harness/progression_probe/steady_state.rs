//! Steady-state overlap, separation, and autonomous processing support for primitive progression.

use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in super::super) enum PrimitiveSteadyStop {
    #[default]
    CycleLimit,
    StockpileOrderComplete,
    TargetSupply,
    ToolCondition,
    CrusherCondition,
    CrankCondition,
}

impl PrimitiveSteadyStop {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::CycleLimit => "probe-cycle-limit",
            Self::StockpileOrderComplete => "stockpile-order-complete",
            Self::TargetSupply => "known-target-supply",
            Self::ToolCondition => "player-tool-condition-lifetime",
            Self::CrusherCondition => "crusher-condition-lifetime",
            Self::CrankCondition => "crank-condition-lifetime",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SteadyStateWork {
    pub(super) cycles: u64,
    pub(super) overlap_setup_equivalent_cycle: Option<u64>,
    pub(super) charge_ticks: u64,
    pub(super) machine_ticks: u64,
    pub(super) useful_overlap_ticks: u64,
    pub(super) player_free_ticks: u64,
    pub(super) mined_mass: Mass,
    pub(super) mining_jobs: u64,
    pub(super) feed_buffer_limited_cycles: u64,
    pub(super) maintenance_preparation_ticks: u64,
    pub(super) maintenance_preparation_overlap_ticks: u64,
    pub(super) stop: PrimitiveSteadyStop,
    pub(super) terminal_crusher_condition_ppm: u32,
}

#[derive(Clone, Copy)]
pub(super) struct SteadyStateCrushingPlan {
    pub(super) ore_storage: deep_hearth::inventory::StockpileId,
    pub(super) crushed_storage: deep_hearth::inventory::StockpileId,
    pub(super) machine: PrimitiveMachine,
    pub(super) concurrent: ConcurrentMiningPlan,
    pub(super) raw: deep_hearth::inventory::StockpileId,
    pub(super) native_storage: deep_hearth::inventory::StockpileId,
    pub(super) shaped: deep_hearth::inventory::StockpileId,
    pub(super) required_productive_ticks: u64,
}

pub(super) fn stage_pick_service_component_while_crushing(
    registries: &Registries,
    state: &mut AppState,
    concurrent: ConcurrentMachineWork,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    pick: EquipmentId,
) -> (u64, u64, u64) {
    let pick_record = state.equipment().get_equipment(pick).unwrap_or_else(|| {
        panic!("primitive progression pick disappeared before staged service prep")
    });
    let profile = registries
        .equipment()
        .get_equipment(pick_record.definition())
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("primitive reinforced pick lost its maintenance profile"));
    let replacement = profile.replacement();
    let required = profile.full_service_replacement_mass();
    let available = state
        .inventory()
        .get_stockpile(shaped)
        .map(|stockpile| stockpile.get_mass(replacement))
        .unwrap_or_else(|| {
            panic!("primitive shaped stockpile disappeared before staged service prep")
        });
    if available >= required {
        return (0, 0, finish_autonomous_crush(registries, state, concurrent));
    }
    let missing = required
        .checked_sub(available)
        .unwrap_or_else(|| unreachable!("staged maintenance component is known to be short"));
    let (craft, batches, source) = manual_craft_plan_for_available_output(
        registries,
        state,
        &[raw, native_storage],
        replacement,
        missing,
        "primitive staged maintenance component",
    );
    let request = select_manual_craft_request(
        registries,
        state,
        craft.process(),
        source,
        batches,
        "primitive staged maintenance component",
    );
    let machine_remaining = state
        .production()
        .get_job(concurrent.job)
        .map(|job| {
            job.completes_at()
                .value()
                .checked_sub(state.tick().value())
                .unwrap_or_else(|| {
                    panic!("primitive crusher completion fell behind staged maintenance prep")
                })
        })
        .unwrap_or(0);
    assert!(
        machine_remaining > 0,
        "staged maintenance preparation requires an active autonomous crusher window"
    );
    let craft_job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(request, shaped),
    )
    .unwrap_or_else(|error| panic!("primitive staged maintenance craft start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive staged maintenance craft commit failed: {error}"));
    let craft_ticks = state
        .production()
        .get_job(craft_job)
        .map(|job| {
            job.completes_at()
                .value()
                .checked_sub(state.tick().value())
                .unwrap_or_else(|| panic!("staged maintenance craft completion precedes start"))
        })
        .unwrap_or_else(|| panic!("staged maintenance craft disappeared after admission"));
    assert!(craft_ticks > 0);
    let mut craft_completion_seen = false;
    for elapsed in 1..=craft_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("primitive staged maintenance tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[craft_job, concurrent.job],
                ..TickEventAllowance::default()
            },
            "staged maintenance work",
        );
        if outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == craft_job)
        {
            assert_eq!(
                elapsed, craft_ticks,
                "staged maintenance craft completed before its admitted schedule"
            );
            craft_completion_seen = true;
        }
    }
    assert!(
        craft_completion_seen,
        "staged maintenance craft produced no completion receipt"
    );
    assert_eq!(state.player_work().active(), None);
    assert!(
        state
            .inventory()
            .get_stockpile(shaped)
            .is_some_and(|stockpile| stockpile.get_mass(replacement) >= required),
        "staged maintenance craft did not produce the required replacement component"
    );
    let overlap_ticks = craft_ticks.min(machine_remaining);
    let idle_ticks = finish_autonomous_crush(registries, state, concurrent);
    assert_eq!(
        overlap_ticks
            .checked_add(idle_ticks)
            .unwrap_or_else(|| panic!("staged maintenance crusher-tail accounting overflowed")),
        machine_remaining,
        "staged maintenance work must partition the original autonomous crusher tail"
    );
    (craft_ticks, overlap_ticks, idle_ticks)
}

pub(super) fn run_steady_state_crushing(
    registries: &Registries,
    state: &mut AppState,
    plan: SteadyStateCrushingPlan,
) -> SteadyStateWork {
    let SteadyStateCrushingPlan {
        ore_storage,
        crushed_storage,
        machine,
        concurrent,
        raw,
        native_storage,
        shaped,
        required_productive_ticks,
    } = plan;
    let mut totals = SteadyStateWork::default();
    let mut overlap_setup_equivalent_cycle = None;
    let mut maintenance_staged = false;
    for cycle in 1..=MAX_STEADY_STATE_CRUSH_CYCLES {
        let charge_ticks =
            match fill_primitive_accumulator(registries, state, machine, machine.required_energy) {
                Ok(ticks) => ticks,
                Err(
                    ManualPowerError::ConditionDuration(_)
                    | ManualPowerError::ZeroEquipmentPower { .. },
                ) => {
                    totals.stop = PrimitiveSteadyStop::CrankCondition;
                    break;
                }
                Err(error) => panic!("primitive progression steady recharge failed: {error}"),
            };
        totals.charge_ticks = totals
            .charge_ticks
            .checked_add(charge_ticks)
            .unwrap_or_else(|| panic!("primitive steady-state charge duration overflowed"));
        let work = match crush_while_mining(
            registries,
            state,
            ore_storage,
            crushed_storage,
            machine,
            CrushingBatch {
                mass: concurrent.mass,
                expected_energy: machine.required_energy,
            },
            concurrent,
        ) {
            Ok(work) => work,
            Err(ComminutionResolutionError::ConditionDuration(_)) => {
                totals.stop = PrimitiveSteadyStop::CrusherCondition;
                break;
            }
            Err(error) => {
                panic!("primitive progression steady crushing resolution failed: {error}")
            }
        };
        totals.cycles = cycle;
        let player_free = if maintenance_staged {
            finish_autonomous_crush(registries, state, work)
        } else {
            let (preparation_ticks, overlap_ticks, idle_ticks) =
                stage_pick_service_component_while_crushing(
                    registries,
                    state,
                    work,
                    raw,
                    native_storage,
                    shaped,
                    concurrent.pick,
                );
            maintenance_staged = true;
            totals.maintenance_preparation_ticks = preparation_ticks;
            totals.maintenance_preparation_overlap_ticks = overlap_ticks;
            idle_ticks
        };
        let useful_overlap = work
            .crush_ticks
            .checked_sub(player_free)
            .unwrap_or_else(|| panic!("steady-state free time exceeded machine duration"));
        totals.machine_ticks = totals
            .machine_ticks
            .checked_add(work.crush_ticks)
            .unwrap_or_else(|| panic!("primitive steady-state machine duration overflowed"));
        totals.useful_overlap_ticks = totals
            .useful_overlap_ticks
            .checked_add(useful_overlap)
            .unwrap_or_else(|| panic!("primitive steady-state overlap duration overflowed"));
        totals.player_free_ticks = totals
            .player_free_ticks
            .checked_add(player_free)
            .unwrap_or_else(|| panic!("primitive steady-state free duration overflowed"));
        totals.mined_mass = totals
            .mined_mass
            .checked_add(work.mined_mass)
            .unwrap_or_else(|| panic!("primitive steady-state mined mass overflowed"));
        totals.mining_jobs = totals
            .mining_jobs
            .checked_add(work.mining_jobs)
            .unwrap_or_else(|| panic!("primitive steady-state mining-job count overflowed"));
        if matches!(
            work.autonomous_stop,
            AutonomousWorkStop::FeedBufferReady | AutonomousWorkStop::FeedBufferCapacity
        ) {
            totals.feed_buffer_limited_cycles = totals
                .feed_buffer_limited_cycles
                .checked_add(1)
                .unwrap_or_else(|| panic!("primitive steady-state buffer-limit count overflowed"));
        }
        if overlap_setup_equivalent_cycle.is_none()
            && totals.useful_overlap_ticks >= required_productive_ticks
        {
            overlap_setup_equivalent_cycle = Some(cycle);
        }
        if work.mining_jobs == 0 {
            match work.autonomous_stop {
                AutonomousWorkStop::TargetSupply => {
                    totals.stop = PrimitiveSteadyStop::TargetSupply;
                    break;
                }
                AutonomousWorkStop::ToolCondition => {
                    totals.stop = PrimitiveSteadyStop::ToolCondition;
                    break;
                }
                AutonomousWorkStop::MachineCompleted
                | AutonomousWorkStop::FeedBufferReady
                | AutonomousWorkStop::FeedBufferCapacity => {}
            }
        }
        if cycle >= STOCKPILE_WORK_ORDER_CYCLES {
            totals.stop = PrimitiveSteadyStop::StockpileOrderComplete;
            break;
        }
    }
    totals.overlap_setup_equivalent_cycle = overlap_setup_equivalent_cycle;
    totals.terminal_crusher_condition_ppm = state
        .equipment()
        .get_equipment(machine.crusher)
        .unwrap_or_else(|| panic!("primitive crusher disappeared at steady-state endpoint"))
        .condition()
        .parts_per_million();
    assert!(
        totals.cycles > 0,
        "primitive automation must complete useful work before its lifecycle endpoint"
    );
    totals
}

#[derive(Clone, Copy)]
pub(super) struct ConcurrentMachineWork {
    pub(super) job: ProductionJobId,
    pub(super) machine_started_at: u64,
    pub(super) crush_ticks: u64,
    pub(super) player_work_ticks: u64,
    pub(super) overlap_ticks: u64,
    pub(super) mined_mass: Mass,
    pub(super) mining_jobs: u64,
    pub(super) autonomous_stop: AutonomousWorkStop,
}

#[derive(Clone, Copy)]
pub(super) struct ConcurrentMiningPlan {
    pub(super) target: MiningTargetRequest,
    pub(super) destination: deep_hearth::inventory::StockpileId,
    pub(super) pick: deep_hearth::equipment::EquipmentId,
    pub(super) mass: Mass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ObservedMaterialSample {
    pub(super) commodity: CommodityKey,
    pub(super) copper_ppm: u32,
}

pub(super) fn observe_material_sample(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    context: &'static str,
) -> ObservedMaterialSample {
    let mut lots = state.inventory().lot_ids(stockpile);
    let first = lots
        .next()
        .unwrap_or_else(|| panic!("primitive progression {context} has no extracted material"));
    let first_record = state
        .inventory()
        .get_lot(first)
        .unwrap_or_else(|| panic!("primitive progression {context} sample disappeared"));
    let commodity = first_record.commodity();
    let composition = first_record.composition();
    for lot in lots {
        let record = state.inventory().get_lot(lot).unwrap_or_else(|| {
            panic!("primitive progression {context} sample fragment disappeared")
        });
        assert_eq!(
            record.commodity(),
            commodity,
            "primitive progression {context} contains physically different commodities and is not one observable sample"
        );
        assert_eq!(
            record.composition(),
            composition,
            "primitive progression {context} contains compositionally different lots and cannot be treated as one assay"
        );
    }
    ObservedMaterialSample {
        commodity,
        copper_ppm: composition.parts_per_million(MATERIAL_COPPER),
    }
}

#[derive(Clone, Copy)]
pub(super) struct PrimitiveSeparationWork {
    pub(super) feed_mass: Mass,
    pub(super) target_mass: Mass,
    pub(super) residue_mass: Mass,
    pub(super) processing_rate: MassFlow,
    pub(super) required_energy: Energy,
    pub(super) charge_ticks: u64,
    pub(super) throughput_ticks: u64,
    pub(super) energy_ticks: u64,
    pub(super) ticks: u64,
}

#[derive(Clone, Copy)]
pub(super) struct PrimitiveSeparationPlan {
    pub(super) crushed_storage: deep_hearth::inventory::StockpileId,
    pub(super) native_storage: deep_hearth::inventory::StockpileId,
    pub(super) residue_storage: deep_hearth::inventory::StockpileId,
    pub(super) machine: PrimitiveMachine,
    pub(super) feed_mass: Mass,
    pub(super) expected_target: Mass,
}

pub(super) fn separate_native_copper(
    registries: &Registries,
    state: &mut AppState,
    plan: PrimitiveSeparationPlan,
) -> PrimitiveSeparationWork {
    let PrimitiveSeparationPlan {
        crushed_storage,
        native_storage,
        residue_storage,
        machine,
        feed_mass,
        expected_target,
    } = plan;
    let charge_ticks = fill_primitive_accumulator(
        registries,
        state,
        machine,
        machine.separation_required_energy,
    )
    .unwrap_or_else(|error| panic!("primitive separation recharge failed: {error}"));
    let selections = select_stockpile_mass(
        state,
        crushed_storage,
        feed_mass,
        "primitive separation feed",
    );
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let target_before = state
        .inventory()
        .get_stockpile(native_storage)
        .map(|stockpile| stockpile.get_mass(native))
        .unwrap_or_else(|| panic!("primitive native-copper storage disappeared before separation"));
    let resolved = resolve_constituent_separation_process(
        registries,
        state,
        ConstituentSeparationRequest::new(
            PROCESS_SEPARATE_NATIVE_COPPER,
            crushed_storage,
            selections.as_slice(),
            machine.separator,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive progression separation resolution failed: {error}"));
    assert!(
        resolved.required_energy() <= machine.separation_required_energy,
        "selected progression feed must not exceed the conservative separation-energy allowance used to charge the primitive flywheel"
    );
    assert_eq!(resolved.target_mass(), expected_target);
    let ticks = resolved.process_resolution().duration().value();
    let job = validate_start_process_routed(
        registries,
        state,
        resolved.process_resolution(),
        crushed_storage,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                native_storage,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                residue_storage,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive progression separation start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression separation commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, "primitive powered separation");
    let target_after = state
        .inventory()
        .get_stockpile(native_storage)
        .map(|stockpile| stockpile.get_mass(native))
        .unwrap_or_else(|| panic!("primitive native-copper storage disappeared after separation"));
    assert_eq!(
        target_after.checked_sub(target_before),
        Some(expected_target),
        "processed ore must provide the exact copper parcel used for the second upgrade"
    );
    PrimitiveSeparationWork {
        feed_mass,
        target_mass: resolved.target_mass(),
        residue_mass: resolved.residue_mass(),
        processing_rate: resolved.processing_rate(),
        required_energy: resolved.required_energy(),
        charge_ticks,
        throughput_ticks: resolved.throughput_duration().value(),
        energy_ticks: resolved.energy_duration().value(),
        ticks,
    }
}

#[derive(Clone, Copy)]
pub(super) struct MatureReinvestmentPlan {
    pub(super) raw: deep_hearth::inventory::StockpileId,
    pub(super) shaped: deep_hearth::inventory::StockpileId,
    pub(super) ore_storage: deep_hearth::inventory::StockpileId,
    pub(super) crushed_storage: deep_hearth::inventory::StockpileId,
    pub(super) native_storage: deep_hearth::inventory::StockpileId,
    pub(super) residue_storage: deep_hearth::inventory::StockpileId,
    pub(super) machine: PrimitiveMachine,
    pub(super) pick: deep_hearth::equipment::EquipmentId,
    pub(super) mining_target: MiningTargetRequest,
    pub(super) primary_batch_mass: Mass,
    pub(super) separation_feed_mass: Mass,
    pub(super) reinforcement_mass: Mass,
}

pub(super) fn crush_mass_for_exact_energy(registries: &Registries, energy: Energy) -> Mass {
    let process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive reinvestment crusher process disappeared"));
    let mass = calculate_mass_specific_energy_capacity(energy, process.specific_energy());
    assert_eq!(
        calculate_mass_specific_energy(mass, process.specific_energy()),
        energy,
        "primitive reinvestment stored work must map to an exact crusher feed mass"
    );
    mass
}

pub(super) fn resolve_crush_ticks(
    registries: &Registries,
    state: &AppState,
    source: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    mass: Mass,
    expected_energy: Energy,
    context: &'static str,
) -> u64 {
    let selection = select_stockpile_mass(state, source, mass, context);
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            source,
            selection.as_slice(),
            machine.crusher,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), expected_energy);
    resolved.process_resolution().duration().value()
}

#[derive(Clone, Copy)]
pub(super) struct UninterruptedCrushPlan {
    pub(super) source: deep_hearth::inventory::StockpileId,
    pub(super) destination: deep_hearth::inventory::StockpileId,
    pub(super) machine: PrimitiveMachine,
    pub(super) mass: Mass,
    pub(super) expected_energy: Energy,
    pub(super) context: &'static str,
}

pub(super) fn run_uninterrupted_crush(
    registries: &Registries,
    state: &mut AppState,
    plan: UninterruptedCrushPlan,
) -> u64 {
    let UninterruptedCrushPlan {
        source,
        destination,
        machine,
        mass,
        expected_energy,
        context,
    } = plan;
    let selection = select_stockpile_mass(state, source, mass, context);
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            source,
            selection.as_slice(),
            machine.crusher,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), expected_energy);
    let ticks = resolved.process_resolution().duration().value();
    let job = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, context);
    ticks
}

pub(super) fn charge_exact_reinvestment_energy(
    registries: &Registries,
    state: &mut AppState,
    machine: PrimitiveMachine,
    energy: Energy,
) -> u64 {
    let start = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            machine.crank,
            machine.drive,
            energy,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment accumulator charge failed: {error}"));
    let work = start.work();
    let ticks = duration(work.started_at().value(), work.completes_at().value());
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive reinvestment charge commit failed: {error}"));
    assert_eq!(
        finish_manual_power_work(registries, state, work, "primitive reinvestment charge"),
        ticks
    );
    ticks
}

#[derive(Clone, Copy)]
pub(super) struct CrushingBatch {
    pub(super) mass: Mass,
    pub(super) expected_energy: Energy,
}

pub(super) fn crush_while_mining(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    batch: CrushingBatch,
    concurrent: ConcurrentMiningPlan,
) -> Result<ConcurrentMachineWork, ComminutionResolutionError> {
    let CrushingBatch {
        mass: crush_mass,
        expected_energy,
    } = batch;
    let machine_started_at = state.tick().value();
    let selection = select_stockpile_mass(state, ore_storage, crush_mass, "primitive crusher feed");
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ore_storage,
            selection.as_slice(),
            machine.crusher,
            machine.drive,
        ),
    )?;
    assert_eq!(resolved.required_energy(), expected_energy);
    let crush_ticks = resolved.process_resolution().duration().value();
    let crush_job = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        ore_storage,
        crushed_storage,
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression crushing commit failed: {error}"));

    let mut player_work_ticks = 0_u64;
    let mut overlap_ticks = 0_u64;
    let mut mined_mass = Mass::ZERO;
    let mut mining_jobs = 0_u64;
    let autonomous_stop = loop {
        let Some(machine_job) = state.production().get_job(crush_job) else {
            break AutonomousWorkStop::MachineCompleted;
        };
        let machine_ticks_remaining = machine_job
            .completes_at()
            .value()
            .checked_sub(state.tick().value())
            .unwrap_or_else(|| {
                panic!("primitive crusher completion fell behind authoritative time")
            });
        // Keep two upcoming batches on hand, not a stockpile sized to occupy every
        // idle tick. This is actor inventory policy, not a production capacity rule.
        let feed_buffer = multiply_mass(
            concurrent.mass.max(crush_mass).max(machine.reserve_mass),
            2,
            "concurrent feed buffer",
        );
        let stored_feed = state
            .inventory()
            .get_stockpile(concurrent.destination)
            .unwrap_or_else(|| panic!("primitive concurrent feed storage disappeared"))
            .stored_mass();
        if stored_feed >= feed_buffer {
            break AutonomousWorkStop::FeedBufferReady;
        }
        let concurrent_target = match resolve_mining_target(state, concurrent.target) {
            Ok(target) => target,
            Err(error) => break autonomous_target_resolution_stop(error),
        };
        let concurrent_mining = match validate_start_mining(
            registries,
            state,
            MINING_METHOD_HAND_PICK,
            concurrent_target,
            concurrent.destination,
            concurrent.pick,
            concurrent.mass,
        ) {
            Ok(start) => start,
            Err(error) => break autonomous_mining_stop(error),
        };
        let concurrent_mining_job = concurrent_mining.commit(state).unwrap_or_else(|error| {
            panic!("primitive progression concurrent mining commit failed: {error}")
        });
        let work_ticks = state
            .mining()
            .get_job(concurrent_mining_job)
            .map(|record| duration(record.started_at().value(), record.completes_at().value()))
            .unwrap_or_else(|| panic!("primitive progression concurrent mining job disappeared"));
        if mining_jobs == 0 {
            assert!(
                state.production().get_job(crush_job).is_some()
                    && state.mining().get_job(concurrent_mining_job).is_some()
                    && state.player_work().active().is_some(),
                "autonomous crushing and player mining must coexist after both canonical starts"
            );
        }
        overlap_ticks = overlap_ticks
            .checked_add(machine_ticks_remaining.min(work_ticks))
            .unwrap_or_else(|| panic!("primitive concurrent overlap duration overflowed"));
        player_work_ticks = player_work_ticks
            .checked_add(work_ticks)
            .unwrap_or_else(|| panic!("primitive concurrent player-work duration overflowed"));
        mining_jobs = mining_jobs
            .checked_add(1)
            .unwrap_or_else(|| panic!("primitive concurrent mining-job count overflowed"));
        assert_eq!(
            finish_mining_work(
                registries,
                state,
                concurrent_mining_job,
                Some(crush_job),
                "concurrent mining",
            ),
            work_ticks
        );
        assert!(
            state.mining().get_job(concurrent_mining_job).is_some(),
            "completed mining output must remain claimable after concurrent machine work"
        );
        let receipt = validate_claim_mining_output(registries, state, concurrent_mining_job)
            .unwrap_or_else(|error| {
                panic!("primitive progression concurrent mining claim failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("primitive progression concurrent mining claim commit failed: {error}")
            });
        let recovered = receipt.output().mass();
        mined_mass = mined_mass
            .checked_add(recovered)
            .unwrap_or_else(|| panic!("primitive concurrent mined mass overflowed"));
        if recovered < concurrent.mass {
            break AutonomousWorkStop::TargetSupply;
        }
    };
    Ok(ConcurrentMachineWork {
        job: crush_job,
        machine_started_at,
        crush_ticks,
        player_work_ticks,
        overlap_ticks,
        mined_mass,
        mining_jobs,
        autonomous_stop,
    })
}

pub(super) fn finish_autonomous_crush(
    registries: &Registries,
    state: &mut AppState,
    concurrent: ConcurrentMachineWork,
) -> u64 {
    let Some(job) = state.production().get_job(concurrent.job) else {
        return 0;
    };
    assert!(
        !job.is_suspended(),
        "primitive progression has no world mutation that should suspend its autonomous crusher"
    );
    let player_free_ticks = job
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("primitive crusher completion fell behind authoritative time"));
    let mut completion_seen = false;
    for elapsed in 1..=player_free_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("primitive autonomous crusher tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[concurrent.job],
                ..TickEventAllowance::default()
            },
            "primitive autonomous crusher",
        );
        if outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == concurrent.job)
        {
            assert_eq!(
                elapsed, player_free_ticks,
                "primitive autonomous crusher completed before its authoritative schedule"
            );
            completion_seen = true;
        }
    }
    assert!(
        completion_seen,
        "primitive autonomous crusher produced no completion receipt at its authoritative schedule"
    );
    player_free_ticks
}
