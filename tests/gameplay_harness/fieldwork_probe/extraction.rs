//! Claim-driven continuation: hidden reserves never determine requests or stopping policy.

use deep_hearth::content::{MATERIAL_COPPER, MINING_METHOD_HAND_PICK};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::maintenance::Condition;
use deep_hearth::mining::{
    MiningClaimReceipt, MiningTargetRequest, MiningTargetResolution, MiningTargetResolutionError,
    ValidatedMiningStart, resolve_mining_target, validate_claim_mining_output,
    validate_start_mining,
};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FieldworkStop {
    OrderComplete,
    ShortClaim,
    TargetNoLongerResolved,
}

impl FieldworkStop {
    pub(super) fn outcome(self) -> &'static str {
        match self {
            Self::OrderComplete => "completed",
            Self::ShortClaim | Self::TargetNoLongerResolved => "known-target-supply",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::OrderComplete => "order-complete",
            Self::ShortClaim => "short-claim",
            Self::TargetNoLongerResolved => "target-no-longer-resolved",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct FieldworkExtraction {
    pub(super) extracted: Mass,
    pub(super) ticks: u64,
    pub(super) batches: u64,
    pub(super) first_ore_ticks: u64,
    pub(super) output_grade_ppm: u32,
    pub(super) condition_before: Condition,
    pub(super) condition_after: Condition,
    pub(super) stop: FieldworkStop,
    pub(super) adaptation: &'static str,
}

struct BatchClaim {
    receipt: MiningClaimReceipt,
    ticks: u64,
    condition_before: Condition,
    condition_after: Condition,
}

fn complete_batch(
    registries: &Registries,
    state: &mut AppState,
    start: ValidatedMiningStart,
) -> BatchClaim {
    let job = start
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork mining start commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("fieldwork mining job disappeared after start"));
    let ticks = record.completes_at().value() - record.started_at().value();
    let condition_before = record.equipment_condition_before();
    let condition_after = record.equipment_condition_after();
    // Only the schedule and wear are observed before the claim, never the reserved output.
    for elapsed in 1..=ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("fieldwork mining tick failed: {error}"));
        assert_eq!(
            outcome.ready_mining_jobs().contains(&job),
            elapsed == ticks,
            "fieldwork readiness diverged from its authoritative schedule"
        );
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                mining_jobs: &[job],
                ..TickEventAllowance::default()
            },
            "fieldwork mining",
        );
    }
    let receipt = validate_claim_mining_output(registries, state, job)
        .unwrap_or_else(|error| panic!("fieldwork mining claim validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork mining claim commit failed: {error}"));
    BatchClaim {
        receipt,
        ticks,
        condition_before,
        condition_after,
    }
}

impl FieldworkExtraction {
    fn record_claim(&mut self, claim: BatchClaim, requested_batch: Mass) {
        let landed = claim.receipt.output().mass();
        assert!(landed <= requested_batch);
        self.extracted = self
            .extracted
            .checked_add(landed)
            .unwrap_or_else(|| panic!("fieldwork cumulative output overflowed"));
        self.ticks = self
            .ticks
            .checked_add(claim.ticks)
            .unwrap_or_else(|| panic!("fieldwork cumulative extraction time overflowed"));
        self.batches += 1;
        self.condition_after = claim.condition_after;
        if landed < requested_batch {
            self.stop = FieldworkStop::ShortClaim;
        }
    }
}

pub(super) struct FieldworkExtractionOrder {
    pub(super) target: MiningTargetResolution,
    pub(super) destination: StockpileId,
    pub(super) equipment: EquipmentId,
    pub(super) requested: Mass,
    pub(super) batch_limit: Mass,
}

pub(super) fn execute_fieldwork_extraction(
    registries: &Registries,
    state: &mut AppState,
    order: FieldworkExtractionOrder,
) -> FieldworkExtraction {
    let FieldworkExtractionOrder {
        target,
        destination,
        equipment,
        requested,
        batch_limit,
    } = order;
    let first_batch = requested.min(batch_limit);
    let adaptation = if first_batch < requested {
        "preparation-plus-order+batch-limit"
    } else {
        "preparation-plus-order"
    };
    let start = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        equipment,
        first_batch,
    )
    .unwrap_or_else(|error| panic!("fieldwork selected-tool mining failed: {error}"));
    let first = complete_batch(registries, state, start);
    let mut result = FieldworkExtraction {
        extracted: Mass::ZERO,
        ticks: 0,
        batches: 0,
        first_ore_ticks: first.ticks,
        output_grade_ppm: first
            .receipt
            .output()
            .composition()
            .parts_per_million(MATERIAL_COPPER),
        condition_before: first.condition_before,
        condition_after: first.condition_after,
        stop: FieldworkStop::OrderComplete,
        adaptation,
    };
    result.record_claim(first, first_batch);
    // A short first or later claim ends the episode immediately. Exact exhaustion becomes
    // observable only when unfinished demand causes the next canonical target refresh.
    while result.stop != FieldworkStop::ShortClaim && result.extracted < requested {
        let refreshed = match resolve_mining_target(
            state,
            MiningTargetRequest::new(target.region(), MATERIAL_COPPER),
        ) {
            Ok(target) => target,
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. }) => {
                result.stop = FieldworkStop::TargetNoLongerResolved;
                break;
            }
            Err(error) => panic!("fieldwork follow-up target failed: {error}"),
        };
        let batch = requested
            .checked_sub(result.extracted)
            .unwrap_or_else(|| unreachable!("loop requires unfinished extraction"))
            .min(first_batch);
        let start = validate_start_mining(
            registries,
            state,
            MINING_METHOD_HAND_PICK,
            refreshed,
            destination,
            equipment,
            batch,
        )
        .unwrap_or_else(|error| panic!("fieldwork follow-up admission failed: {error}"));
        let claim = complete_batch(registries, state, start);
        result.record_claim(claim, batch);
    }
    assert_eq!(
        result.stop == FieldworkStop::OrderComplete,
        result.extracted == requested
    );
    result
}
