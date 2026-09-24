//! Observable prospecting, mining execution, and clue interpretation for primitive progression.

use super::super::prospecting_timing::complete_prospecting_work;
use super::*;

pub(in super::super) fn progression_mining_mass(registries: &Registries, seed: u64) -> Mass {
    let maximum = stone_pick_mining_batch_limit(registries).milligrams();
    assert!(
        maximum > 0,
        "primitive progression mining batch must be nonzero"
    );
    let minimum = maximum
        .checked_mul(3)
        .map(|scaled| scaled.div_ceil(4))
        .unwrap_or_else(|| panic!("primitive progression mining-range scaling overflowed"));
    Mass::from_milligrams(minimum + mix64(seed ^ 0x5052_4F47_4D49_4E45) % (maximum - minimum + 1))
}

pub(super) fn mine_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> u64 {
    try_mine_and_claim(registries, state, target, destination, equipment, mass)
        .unwrap_or_else(|error| panic!("primitive progression mining failed: {error}"))
        .ticks
}

pub(super) fn try_mine_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> Result<MiningAttemptOutcome, MiningStartError> {
    let target = resolve_progression_mining_target(state, target);
    try_mine_and_claim_resolved(registries, state, target, destination, equipment, mass)
}

pub(super) fn try_mine_and_claim_resolved(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetResolution,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> Result<MiningAttemptOutcome, MiningStartError> {
    let mining = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        equipment,
        mass,
    )?;
    let mining_job = mining
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression mining commit failed: {error}"));
    let mining_record = state
        .mining()
        .get_job(mining_job)
        .unwrap_or_else(|| panic!("primitive progression mining job disappeared"));
    let mining_ticks = duration(
        mining_record.started_at().value(),
        mining_record.completes_at().value(),
    );
    assert_eq!(
        finish_mining_work(registries, state, mining_job, None, "mining"),
        mining_ticks
    );
    let receipt = validate_claim_mining_output(registries, state, mining_job)
        .unwrap_or_else(|error| panic!("primitive progression mining claim failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("primitive progression mining claim commit failed: {error}")
        });
    Ok(MiningAttemptOutcome {
        ticks: mining_ticks,
        output: receipt.output().mass(),
    })
}

pub(super) fn mine_total_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    total: Mass,
    maximum_batch: Mass,
) -> u64 {
    try_mine_total_and_claim(
        registries,
        state,
        target,
        destination,
        equipment,
        total,
        maximum_batch,
    )
    .unwrap_or_else(|stop| panic!("primitive progression mining stopped: {}", stop.label()))
}

pub(super) fn try_mine_total_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    total: Mass,
    maximum_batch: Mass,
) -> Result<u64, AutonomousWorkStop> {
    assert!(!total.is_zero());
    assert!(!maximum_batch.is_zero());
    let mut remaining = total;
    let mut elapsed = 0_u64;
    while !remaining.is_zero() {
        let batch = Mass::from_milligrams(remaining.milligrams().min(maximum_batch.milligrams()));
        let resolved_target =
            resolve_mining_target(state, target).map_err(autonomous_target_resolution_stop)?;
        let attempt = try_mine_and_claim_resolved(
            registries,
            state,
            resolved_target,
            destination,
            equipment,
            batch,
        )
        .map_err(autonomous_mining_stop)?;
        elapsed = elapsed
            .checked_add(attempt.ticks)
            .unwrap_or_else(|| panic!("primitive progression mining duration overflowed"));
        remaining = remaining.checked_sub(attempt.output).unwrap_or_else(|| {
            unreachable!("mining output cannot exceed requested remaining mass")
        });
        if attempt.output < batch {
            return Err(AutonomousWorkStop::TargetSupply);
        }
    }
    Ok(elapsed)
}

pub(super) fn resolve_progression_mining_target(
    state: &AppState,
    request: MiningTargetRequest,
) -> MiningTargetResolution {
    resolve_mining_target(state, request)
        .unwrap_or_else(|error| panic!("primitive progression mining evidence failed: {error}"))
}

pub(super) fn acquire_copper_evidence(
    registries: &Registries,
    state: &mut AppState,
    method: ProspectingMethodId,
    region: VoxelBounds,
) -> u64 {
    acquire_copper_evidence_with_equipment(registries, state, method, region, None)
}

pub(super) fn acquire_copper_evidence_with_equipment(
    registries: &Registries,
    state: &mut AppState,
    method: ProspectingMethodId,
    region: VoxelBounds,
    equipment: Option<EquipmentId>,
) -> u64 {
    let request = match equipment {
        Some(equipment) => {
            FieldProspectingRequest::new_with_equipment(method, region, MATERIAL_COPPER, equipment)
        }
        None => FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
    };
    let start = validate_start_field_prospecting(registries, state, request)
        .unwrap_or_else(|error| panic!("primitive progression prospecting failed: {error}"));
    let work = start.work();
    let duration = work
        .completes_at()
        .value()
        .checked_sub(work.started_at().value())
        .unwrap_or_else(|| panic!("primitive progression prospecting schedule is inverted"));
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression prospecting commit failed: {error}"));
    let completion =
        complete_prospecting_work(registries, state, work, "primitive progression prospecting");
    assert_eq!(completion.method(), method);
    assert_eq!(completion.region(), region);
    assert_eq!(completion.material(), MATERIAL_COPPER);
    assert!(
        state
            .geological_knowledge()
            .get_observation(completion.observation())
            .is_some(),
        "prospecting completion receipt must identify the persisted observation"
    );
    duration
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ObservedCopperClue {
    pub(super) request: MiningTargetRequest,
    pub(super) lower_ppm: u32,
    pub(super) upper_ppm: u32,
}

pub(super) fn observed_copper_bounds(state: &AppState, request: MiningTargetRequest) -> (u32, u32) {
    match assess_geological_knowledge(
        state.geological_knowledge(),
        request.region(),
        request.material(),
    )
    .consistency()
    {
        GeologicalEvidenceConsistency::Compatible {
            lower_ppm,
            upper_ppm,
        } => (lower_ppm, upper_ppm),
        GeologicalEvidenceConsistency::NoEvidence => panic!(
            "primitive progression expected compatible copper evidence for {:?}, found no evidence",
            request.region()
        ),
        GeologicalEvidenceConsistency::SpatiallyIncomparable => panic!(
            "primitive progression expected compatible copper evidence for {:?}, found spatially incomparable evidence",
            request.region()
        ),
        GeologicalEvidenceConsistency::Conflicting {
            highest_lower_ppm,
            lowest_upper_ppm,
        } => panic!(
            "primitive progression expected compatible copper evidence for {:?}, found conflicting bounds {highest_lower_ppm}..{lowest_upper_ppm} ppm",
            request.region()
        ),
    }
}

pub(super) fn observed_resolved_copper_clue(
    state: &AppState,
    request: MiningTargetRequest,
) -> ObservedCopperClue {
    let (lower_ppm, upper_ppm) = observed_copper_bounds(state, request);
    let _ = resolve_mining_target(state, request)
        .unwrap_or_else(|error| panic!("observable copper clue did not resolve: {error}"));
    ObservedCopperClue {
        request,
        lower_ppm,
        upper_ppm,
    }
}

pub(super) fn strongest_observed_copper_clue(
    clues: impl IntoIterator<Item = ObservedCopperClue>,
) -> ObservedCopperClue {
    // Evidence quality is the actor's primary preference. Equal evidence uses the observable clue
    // region as an explicit stable policy rather than inheriting setup, registry, or iterator order.
    clues
        .into_iter()
        .max_by_key(|clue| {
            (
                clue.lower_ppm,
                clue.upper_ppm,
                Reverse(clue.request.region().min()),
                Reverse(clue.request.region().max_exclusive()),
            )
        })
        .unwrap_or_else(|| panic!("primitive progression has no eligible observed copper clue"))
}
