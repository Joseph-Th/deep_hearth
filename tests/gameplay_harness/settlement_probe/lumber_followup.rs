//! Same-state settlement lumber reassessment after a newly disclosed repeat order.

use std::num::NonZeroU64;

use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_BOARD, FORM_CHIP, MANUAL_POWER_HAND_CRANK, MATERIAL_WOOD,
    PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    PoweredCraftRequest, project_manual_craft_equipment, project_powered_craft_work,
    validate_start_powered_craft,
};
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId, validate_upgrade_equipment};
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

use super::{LumberInvestmentChoice, SetupPlan, execute_setup, setup_plans};
use crate::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use crate::manual_craft_execution::execute_manual_craft;
use crate::manual_craft_selection::select_manual_craft_request;
use crate::manual_power_timing::finish_manual_power_work;
use crate::material_selection::select_stockpile_mass;
use crate::powered_craft_planning::{
    AuthoredPoweredCraftBatch, PoweredCraftSequenceProjection, project_powered_craft_sequence,
};
use crate::production_timing::finish_uninterrupted_production_job;
use crate::settlement_power_planning::project_manual_power_workload;

#[derive(Clone, Copy)]
pub(super) struct PoweredLumberOrder {
    pub(super) source: StockpileId,
    pub(super) output: StockpileId,
    pub(super) sawmill: EquipmentId,
    pub(super) crank: EquipmentId,
    pub(super) drive: EnergyStoreId,
    pub(super) batch: AuthoredPoweredCraftBatch,
    pub(super) batches: u64,
    pub(super) context: &'static str,
}

pub(super) struct LumberFollowupInputs<'a> {
    pub(super) registries: &'a Registries,
    pub(super) state: &'a mut AppState,
    pub(super) investment_policy: CapitalInvestmentPolicy,
    pub(super) batch: AuthoredPoweredCraftBatch,
    pub(super) manual_process: ProcessId,
    pub(super) work_source: StockpileId,
    pub(super) output: StockpileId,
    pub(super) upgrade_raw: StockpileId,
    pub(super) upgrade_parts: StockpileId,
    pub(super) frame_saw: EquipmentId,
    pub(super) crank: EquipmentId,
    pub(super) drive: EnergyStoreId,
    pub(super) demand_batches: u64,
    pub(super) choice: LumberInvestmentChoice,
    pub(super) board_mass_before: Mass,
    pub(super) chip_mass_before: Mass,
}

pub(super) struct LumberFollowupResult {
    pub(super) active_attention: u64,
    pub(super) delegated_ticks: u64,
    pub(super) charge_events: u64,
    pub(super) route: &'static str,
    pub(super) reinvested: bool,
    pub(super) completed_batches: u64,
    pub(super) reassessment: String,
    pub(super) elapsed_ticks: u64,
    pub(super) board_mass: Mass,
    pub(super) chip_mass: Mass,
    pub(super) terminal: &'static str,
    pub(super) final_upgraded: bool,
}

pub(super) fn project_setup_equipment_condition(
    registries: &Registries,
    plans: &[SetupPlan],
    equipment_id: EquipmentId,
    equipment_definition: EquipmentDefinitionId,
    mut condition: Condition,
) -> Condition {
    for plan in plans
        .iter()
        .filter(|plan| plan.equipment == Some(equipment_id))
    {
        let batches = NonZeroU64::new(plan.batches)
            .unwrap_or_else(|| unreachable!("settlement setup plans have nonzero batches"));
        condition = project_manual_craft_equipment(
            registries,
            plan.process,
            batches,
            equipment_definition,
            condition,
        )
        .unwrap_or_else(|error| panic!("settlement setup condition projection failed: {error}"))
        .condition_after();
    }
    condition
}

pub(super) fn maximum_feasible_batches(upper: u64, mut feasible: impl FnMut(u64) -> bool) -> u64 {
    let mut low = 0_u64;
    let mut high = upper;
    while low < high {
        let midpoint = low + (high - low).div_ceil(2);
        if feasible(midpoint) {
            low = midpoint;
        } else {
            high = midpoint - 1;
        }
    }
    low
}

pub(super) fn project_future_powered_lumber_sequence(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentDefinitionId,
    condition: Condition,
    drive: EnergyStoreId,
    batch: AuthoredPoweredCraftBatch,
    requested_batches: u64,
) -> PoweredCraftSequenceProjection {
    project_powered_craft_sequence(
        registries,
        state,
        PROCESS_POWER_SAW_WOOD_BOARDS,
        equipment,
        condition,
        drive,
        batch,
        requested_batches,
        "settlement powered lumber",
    )
}

fn project_owned_powered_lumber_sequence(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    batch: AuthoredPoweredCraftBatch,
    requested_batches: u64,
    drive: EnergyStoreId,
) -> PoweredCraftSequenceProjection {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("settlement owned sawmill disappeared before projection"));
    let projection = project_future_powered_lumber_sequence(
        registries,
        state,
        record.definition(),
        record.condition(),
        drive,
        batch,
        requested_batches,
    );
    if let Some(first) = projection.legs.first() {
        let actual = project_powered_craft_work(
            registries,
            state,
            PROCESS_POWER_SAW_WOOD_BOARDS,
            first.input_mass,
            equipment,
            drive,
        )
        .unwrap_or_else(|error| {
            panic!("settlement owned powered-lumber projection failed: {error}")
        });
        assert_eq!(actual.required_energy(), first.work);
        assert_eq!(actual.duration().value(), first.delegated_ticks);
        assert_eq!(actual.condition_after(), first.condition_after);
    }
    projection
}

pub(super) fn execute_powered_lumber_order(
    registries: &Registries,
    state: &mut AppState,
    order: PoweredLumberOrder,
) -> (u64, u64, u64) {
    let PoweredLumberOrder {
        source,
        output,
        sawmill,
        crank,
        drive,
        batch,
        batches,
        context,
    } = order;
    let mut attention = 0_u64;
    let mut delegated = 0_u64;
    let mut completed_batches = 0_u64;
    let mut charge_events = 0_u64;
    while completed_batches < batches {
        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(|record| record.stored()),
            Some(Energy::ZERO),
            "{context} must start each packed powered-lumber leg with an empty work buffer"
        );
        let sawmill_record = state
            .equipment()
            .get_equipment(sawmill)
            .unwrap_or_else(|| panic!("{context} sawmill disappeared before leg planning"));
        let remaining = batches
            .checked_sub(completed_batches)
            .unwrap_or_else(|| unreachable!("completed lumber batches are bounded by the order"));
        let sequence = project_powered_craft_sequence(
            registries,
            state,
            PROCESS_POWER_SAW_WOOD_BOARDS,
            sawmill_record.definition(),
            sawmill_record.condition(),
            drive,
            batch,
            remaining,
            context,
        );
        let leg = *sequence
            .legs
            .first()
            .unwrap_or_else(|| panic!("{context} has no feasible powered-lumber leg"));
        let power = validate_start_manual_power(
            registries,
            state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, leg.work),
        )
        .unwrap_or_else(|error| panic!("{context} charging failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("{context} charge commit failed: {error}"));
        attention = attention
            .checked_add(finish_manual_power_work(registries, state, power, context))
            .unwrap_or_else(|| panic!("{context} active attention overflowed"));
        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(|record| record.stored()),
            Some(leg.work),
            "{context} packed charge must leave exactly the work required by its productive leg"
        );
        let selections = select_stockpile_mass(state, source, leg.input_mass, context);
        let job = validate_start_powered_craft(
            registries,
            state,
            PoweredCraftRequest::new(
                PROCESS_POWER_SAW_WOOD_BOARDS,
                source,
                selections,
                sawmill,
                drive,
            ),
            output,
        )
        .unwrap_or_else(|error| panic!("{context} powered start failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("{context} powered commit failed: {error}"));
        let duration = state
            .production()
            .get_job(job)
            .map(|record| record.active_duration().value())
            .unwrap_or_else(|| panic!("{context} powered job disappeared"));
        delegated = delegated
            .checked_add(duration)
            .unwrap_or_else(|| panic!("{context} delegated time overflowed"));
        finish_uninterrupted_production_job(registries, state, job, context);
        assert_eq!(duration, leg.delegated_ticks);
        assert_eq!(
            state
                .equipment()
                .get_equipment(sawmill)
                .map(|record| record.condition()),
            Some(leg.condition_after),
            "{context} sawmill wear diverged from the packed-leg projection"
        );
        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(|record| record.stored()),
            Some(Energy::ZERO),
            "{context} productive leg must consume its complete packed work charge"
        );
        completed_batches = completed_batches
            .checked_add(leg.batches)
            .unwrap_or_else(|| panic!("{context} completed batch count overflowed"));
        charge_events = charge_events
            .checked_add(1)
            .unwrap_or_else(|| panic!("{context} charge-event count overflowed"));
    }
    assert_eq!(completed_batches, batches);
    (attention, delegated, charge_events)
}

pub(super) fn run_lumber_followup(inputs: LumberFollowupInputs<'_>) -> LumberFollowupResult {
    let LumberFollowupInputs {
        registries,
        state,
        investment_policy,
        batch,
        manual_process,
        work_source,
        output,
        upgrade_raw,
        upgrade_parts,
        frame_saw,
        crank,
        drive,
        demand_batches,
        choice,
        board_mass_before,
        chip_mass_before,
    } = inputs;
    let started_at = state.tick().value();
    let (
        active_attention,
        delegated_ticks,
        charge_events,
        route,
        reinvested,
        completed_batches,
        reassessment,
        final_upgraded,
    ) = match choice {
        LumberInvestmentChoice::FrameSaw => {
            let frame_condition = state
                .equipment()
                .get_equipment(frame_saw)
                .map(|record| record.condition())
                .unwrap_or_else(|| {
                    panic!("settlement frame saw disappeared before follow-up reassessment")
                });
            let manual_capacity = maximum_feasible_batches(demand_batches, |batches| {
                let batches = NonZeroU64::new(batches)
                    .unwrap_or_else(|| unreachable!("positive follow-up batch count is nonzero"));
                project_manual_craft_equipment(
                    registries,
                    manual_process,
                    batches,
                    EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
                    frame_condition,
                )
                .is_ok()
            });
            let manual_attention = NonZeroU64::new(manual_capacity).map(|batches| {
                project_manual_craft_equipment(
                    registries,
                    manual_process,
                    batches,
                    EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
                    frame_condition,
                )
                .unwrap_or_else(|error| {
                    panic!("settlement feasible manual follow-up projection failed: {error}")
                })
                .duration()
                .value()
            });

            let (plans, setup_attention) = setup_plans(registries, state, upgrade_raw, frame_saw);
            let sawmill_start_condition = project_setup_equipment_condition(
                registries,
                &plans,
                frame_saw,
                EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
                frame_condition,
            );
            let powered_projection = project_future_powered_lumber_sequence(
                registries,
                state,
                EQUIPMENT_TIMBER_SASH_SAWMILL,
                sawmill_start_condition,
                drive,
                batch,
                demand_batches,
            );
            let powered_capacity = powered_projection.batches;
            let crank_condition = state
                .equipment()
                .get_equipment(crank)
                .map(|record| record.condition())
                .unwrap_or_else(|| {
                    panic!("settlement hand crank disappeared before follow-up reassessment")
                });
            let machine_attention = (powered_capacity > 0).then(|| {
                setup_attention
                    .checked_add(
                        project_manual_power_workload(
                            registries,
                            MANUAL_POWER_HAND_CRANK,
                            EQUIPMENT_STONE_HAND_CRANK,
                            crank_condition,
                            ENERGY_STONE_FLYWHEEL_DRIVE,
                            powered_projection.charge_energies(),
                            "settlement follow-up sawmill workload",
                        )
                        .attention_ticks,
                    )
                    .unwrap_or_else(|| panic!("settlement follow-up machine attention overflowed"))
            });
            let minimum_return = investment_policy.minimum_attention_return(0, setup_attention);
            let (should_upgrade, decision_basis) = if powered_capacity > manual_capacity {
                (true, "capacity")
            } else if powered_capacity < manual_capacity || powered_capacity == 0 {
                (false, "capacity")
            } else {
                (
                    clears_attention_return(
                        manual_attention.unwrap_or_else(|| {
                            unreachable!("positive equal capacities have manual attention")
                        }),
                        machine_attention.unwrap_or_else(|| {
                            unreachable!("positive equal capacities have machine attention")
                        }),
                        minimum_return,
                    ),
                    "attention-return",
                )
            };

            if should_upgrade {
                let executed_setup =
                    execute_setup(registries, state, upgrade_raw, upgrade_parts, &plans);
                assert_eq!(
                    executed_setup, setup_attention,
                    "settlement follow-up setup projection must match execution"
                );
                let sawmill = validate_upgrade_equipment(
                    registries,
                    state,
                    frame_saw,
                    EQUIPMENT_TIMBER_SASH_SAWMILL,
                    upgrade_parts,
                )
                .unwrap_or_else(|error| {
                    panic!("settlement follow-up sash-sawmill upgrade failed: {error}")
                })
                .commit(state)
                .unwrap_or_else(|error| {
                    panic!("settlement follow-up sash-sawmill commit failed: {error}")
                });
                assert_eq!(
                    sawmill, frame_saw,
                    "settlement follow-up upgrade must preserve equipment identity"
                );
                let (attention, delegated, executed_charge_events) = execute_powered_lumber_order(
                    registries,
                    state,
                    PoweredLumberOrder {
                        source: work_source,
                        output,
                        sawmill,
                        crank,
                        drive,
                        batch,
                        batches: powered_capacity,
                        context: "settlement follow-up sawmill order",
                    },
                );
                let total_attention = attention
                    .checked_add(setup_attention)
                    .unwrap_or_else(|| panic!("settlement follow-up attention overflowed"));
                assert_eq!(
                    Some(total_attention),
                    machine_attention,
                    "settlement follow-up machine attention projection must match execution"
                );
                assert_eq!(
                    delegated, powered_projection.delegated_ticks,
                    "settlement follow-up powered duration projection must match execution"
                );
                assert_eq!(
                    executed_charge_events,
                    powered_projection.charge_events(),
                    "settlement follow-up packed charge plan must match execution"
                );
                (
                    total_attention,
                    delegated,
                    executed_charge_events,
                    "sash-sawmill",
                    true,
                    powered_capacity,
                    format!(
                        "basis:{decision_basis} capacity=[manual:{manual_capacity} powered:{powered_capacity}] power-cycle=[charges:{} max-batches-per-charge:{}] attention=[manual:{} powered:{total_attention}t] minimum-return:{minimum_return}t result:upgrade",
                        powered_projection.charge_events(),
                        powered_projection.maximum_leg_batches(),
                        manual_attention
                            .map(|ticks| format!("{ticks}t"))
                            .unwrap_or_else(|| "unavailable".to_owned())
                    ),
                    true,
                )
            } else if manual_capacity > 0 {
                let request = select_manual_craft_request(
                    registries,
                    state,
                    manual_process,
                    work_source,
                    manual_capacity,
                    "settlement follow-up frame-saw order",
                )
                .with_equipment(frame_saw);
                let attention = execute_manual_craft(
                    registries,
                    state,
                    request,
                    output,
                    "settlement follow-up frame-saw order",
                )
                .value();
                assert_eq!(
                    Some(attention),
                    manual_attention,
                    "settlement follow-up manual projection must match execution"
                );
                (
                    attention,
                    0,
                    0,
                    "frame-saw",
                    false,
                    manual_capacity,
                    format!(
                        "basis:{decision_basis} capacity=[manual:{manual_capacity} powered:{powered_capacity}] attention=[manual:{attention}t powered:{}] minimum-return:{minimum_return}t result:keep-frame-saw",
                        machine_attention
                            .map(|ticks| format!("{ticks}t"))
                            .unwrap_or_else(|| "unavailable".to_owned())
                    ),
                    false,
                )
            } else {
                (
                    0,
                    0,
                    0,
                    "blocked",
                    false,
                    0,
                    format!(
                        "capacity=[manual:0 powered:{powered_capacity}] attention=[manual:unavailable powered:{}] result:wear-blocked",
                        machine_attention
                            .map(|ticks| format!("{ticks}t"))
                            .unwrap_or_else(|| "unavailable".to_owned())
                    ),
                    false,
                )
            }
        }
        LumberInvestmentChoice::SashSawmill => {
            let projection = project_owned_powered_lumber_sequence(
                registries,
                state,
                frame_saw,
                batch,
                demand_batches,
                drive,
            );
            if projection.batches == 0 {
                (
                    0,
                    0,
                    0,
                    "blocked",
                    false,
                    0,
                    "owned-sawmill:wear-blocked".to_owned(),
                    true,
                )
            } else {
                let (attention, delegated, executed_charge_events) = execute_powered_lumber_order(
                    registries,
                    state,
                    PoweredLumberOrder {
                        source: work_source,
                        output,
                        sawmill: frame_saw,
                        crank,
                        drive,
                        batch,
                        batches: projection.batches,
                        context: "settlement follow-up sawmill order",
                    },
                );
                assert_eq!(
                    delegated, projection.delegated_ticks,
                    "settlement owned-sawmill follow-up projection must match execution"
                );
                assert_eq!(
                    executed_charge_events,
                    projection.charge_events(),
                    "settlement owned-sawmill packed charge plan must match execution"
                );
                (
                    attention,
                    delegated,
                    executed_charge_events,
                    "sash-sawmill",
                    false,
                    projection.batches,
                    format!(
                        "owned-sawmill:reuse capacity:{}/{} power-cycle=[charges:{} max-batches-per-charge:{}]",
                        projection.batches,
                        demand_batches,
                        projection.charge_events(),
                        projection.maximum_leg_batches(),
                    ),
                    true,
                )
            }
        }
    };
    let elapsed_ticks = state.tick().value() - started_at;
    let output_record = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("settlement output disappeared after follow-up"));
    let board_mass = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD));
    let chip_mass = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP));
    if completed_batches == 0 {
        assert_eq!(
            (board_mass, chip_mass),
            (board_mass_before, chip_mass_before),
            "blocked settlement follow-up must not create lumber output"
        );
    } else {
        assert!(
            board_mass > board_mass_before && chip_mass > chip_mass_before,
            "productive settlement follow-up must add both authored lumber outputs"
        );
    }
    let terminal = if completed_batches == demand_batches {
        "complete"
    } else if completed_batches == 0 {
        "wear-blocked"
    } else {
        "wear-limited"
    };
    LumberFollowupResult {
        active_attention,
        delegated_ticks,
        charge_events,
        route,
        reinvested,
        completed_batches,
        reassessment,
        elapsed_ticks,
        board_mass,
        chip_mass,
        terminal,
        final_upgraded,
    }
}
