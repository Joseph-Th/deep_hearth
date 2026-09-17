//! Executed charging-policy comparison, kept outside actor decisions.

use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;

use super::{PrimitiveLiberationScenario, scavenging::ScavengingOutcome};

fn charge_ticks(scenario: &PrimitiveLiberationScenario) -> u64 {
    scenario.charges.iter().map(|charge| charge.ticks).sum()
}

fn generated_nj(scenario: &PrimitiveLiberationScenario) -> u128 {
    scenario
        .charges
        .iter()
        .map(|charge| charge.requested.nanojoules())
        .sum()
}

pub(super) fn review(
    registries: &Registries,
    seed: u64,
    started_at: u64,
    primary_completed_at: u64,
    demand: &PrimitiveLiberationScenario,
    full: &PrimitiveLiberationScenario,
    scavenged: &ScavengingOutcome,
) {
    validate_loaded_state(registries, &full.state)
        .expect("full-buffer branch must remain loadable");
    assert_eq!(
        calculate_matter_accounting(&full.state)
            .expect("full-buffer matter audit")
            .total(),
        calculate_matter_accounting(&demand.state)
            .expect("batch-demand matter audit")
            .total(),
    );
    let body_cost = |scenario: &PrimitiveLiberationScenario| -> (u128, u64) {
        (
            scenario
                .charges
                .iter()
                .map(|charge| charge.metabolic_nj)
                .sum(),
            scenario
                .charges
                .iter()
                .map(|charge| charge.hydration_ul)
                .sum(),
        )
    };
    let demand_body = body_cost(demand);
    let full_body = body_cost(full);
    let demand_ticks = charge_ticks(demand);
    let full_ticks = charge_ticks(full);
    let demand_generated = generated_nj(demand);
    let full_generated = generated_nj(full);
    assert!(
        demand_generated < full_generated,
        "this finite pipeline must not fund unused full-buffer work"
    );
    assert!(
        demand_ticks <= full_ticks,
        "demand charging must not increase attention in this bounded pipeline"
    );
    let elapsed = demand.state.tick().value() - started_at;
    let primary_ticks = primary_completed_at - started_at;
    let scavenger_ticks = demand.state.tick().value() - primary_completed_at;
    let remaining = |scenario: &PrimitiveLiberationScenario| {
        scenario
            .state
            .energy()
            .get_store(scenario.drive)
            .expect("comparison drive exists")
            .stored()
            .nanojoules()
    };
    // These are completion costs for the same finite job, not equal-horizon final reserves.
    // The full-buffer arm's remaining work stays visible: it could be useful for a later job.
    reviewln!(
        "LIBERATION COST seed=0x{seed:016X} basis=matched-start-same-finite-pipeline setup=preowned-parts-and-ore primary={}t scavenger={}t total={}t charge=[demand:{}t full:{}t] generated=[demand:{}nJ full:{}nJ] retained=[demand:{}nJ full:{}nJ] scavenger-extra={}mg output=identical acquisition-cost=excluded",
        primary_ticks,
        scavenger_ticks,
        elapsed,
        demand_ticks,
        full_ticks,
        demand_generated,
        full_generated,
        remaining(demand),
        remaining(full),
        scavenged.additional_recovered_copper_ppm_mg / 1_000_000,
    );
    reviewln!(
        "LIBERATION PACING seed=0x{seed:016X} primary={} scavenger={} charge-body=[demand:{}nJ/{}uL full:{}nJ/{}uL] machine-time={}t parallel-work=not-exercised interpretation=extra-recovery-is-not-free-metal",
        super::super::physical_time::format_physical_duration(registries, primary_ticks),
        super::super::physical_time::format_physical_duration(registries, scavenger_ticks),
        demand_body.0,
        demand_body.1,
        full_body.0,
        full_body.1,
        elapsed - demand_ticks,
    );
}
