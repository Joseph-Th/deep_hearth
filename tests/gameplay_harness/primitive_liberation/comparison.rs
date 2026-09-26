//! Executed charging-policy comparison, kept outside actor decisions.

use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;

use super::super::manual_ore_recovery::ManualOreRecoveryReview;
use super::acquisition::RawKitAcquisitionReview;
use super::cleanup::CleanupOutcome;
use super::{
    PrimitiveLiberationCampaignLifecycle, PrimitiveLiberationScenario,
    scavenging::ScavengingOutcome,
};

fn charge_ticks(scenario: &PrimitiveLiberationScenario) -> u64 {
    scenario
        .charges
        .iter()
        .try_fold(0_u64, |total, charge| total.checked_add(charge.ticks))
        .unwrap_or_else(|| panic!("primitive liberation charge attention overflowed"))
}

fn generated_nj(scenario: &PrimitiveLiberationScenario) -> u128 {
    scenario
        .charges
        .iter()
        .try_fold(0_u128, |total, charge| {
            total.checked_add(charge.requested.nanojoules())
        })
        .unwrap_or_else(|| panic!("primitive liberation generated energy overflowed"))
}

pub(super) struct LiberationComparison<'a> {
    pub(super) started_at: u64,
    pub(super) primary_completed_at: u64,
    pub(super) scavenger_completed_at: u64,
    pub(super) demand: &'a PrimitiveLiberationScenario,
    pub(super) full: &'a PrimitiveLiberationScenario,
    pub(super) scavenged: &'a ScavengingOutcome,
    pub(super) cleaned: &'a CleanupOutcome,
    pub(super) direct_cleanup: &'a PrimitiveLiberationScenario,
    pub(super) direct_cleaned: &'a CleanupOutcome,
    pub(super) manual_recovery: &'a ManualOreRecoveryReview,
    pub(super) kit_acquisition: Option<&'a RawKitAcquisitionReview>,
    pub(super) campaign_lifecycle: Option<&'a PrimitiveLiberationCampaignLifecycle>,
    pub(super) planned_batches: u64,
}

pub(super) fn review(registries: &Registries, seed: u64, comparison: LiberationComparison<'_>) {
    let LiberationComparison {
        started_at,
        primary_completed_at,
        scavenger_completed_at,
        demand,
        full,
        scavenged,
        cleaned,
        direct_cleanup,
        direct_cleaned,
        manual_recovery,
        kit_acquisition,
        campaign_lifecycle,
        planned_batches,
    } = comparison;
    validate_loaded_state(registries, &full.state)
        .unwrap_or_else(|error| panic!("full-buffer branch must remain loadable: {error}"));
    assert_eq!(
        calculate_matter_accounting(&full.state)
            .unwrap_or_else(|error| panic!("full-buffer matter audit: {error}"))
            .total(),
        calculate_matter_accounting(&demand.state)
            .unwrap_or_else(|error| panic!("batch-demand matter audit: {error}"))
            .total(),
    );
    let body_cost = |scenario: &PrimitiveLiberationScenario| -> (u128, u64) {
        scenario
            .charges
            .iter()
            .try_fold((0_u128, 0_u64), |(metabolic, hydration), charge| {
                Some((
                    metabolic.checked_add(charge.metabolic_nj)?,
                    hydration.checked_add(charge.hydration_ul)?,
                ))
            })
            .unwrap_or_else(|| panic!("primitive liberation charge body cost overflowed"))
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
    let scavenger_ticks = scavenger_completed_at - primary_completed_at;
    let cleanup_ticks = demand.state.tick().value() - scavenger_completed_at;
    let direct_cleanup_ticks = direct_cleanup.state.tick().value() - primary_completed_at;
    let scavenger_marginal_ticks = demand
        .state
        .tick()
        .value()
        .checked_sub(direct_cleanup.state.tick().value())
        .unwrap_or_else(|| {
            panic!("scavenging branch completed before direct-cleanup counterfactual")
        });
    let scavenger_marginal_native = cleaned
        .native_copper_mass
        .checked_sub(direct_cleaned.native_copper_mass)
        .unwrap_or_else(|| panic!("scavenging reduced recovered native copper"));
    assert!(scavenger_marginal_native > deep_hearth::core::quantity::Mass::ZERO);
    let remaining = |scenario: &PrimitiveLiberationScenario| {
        scenario
            .state
            .energy()
            .get_store(scenario.drive)
            .unwrap_or_else(|| panic!("comparison drive exists"))
            .stored()
            .nanojoules()
    };
    assert_eq!(
        manual_recovery.feed_mass, demand.batch_mass,
        "manual and powered liberation route evidence must use the same ore mass"
    );
    // These are completion costs for the same finite job, not equal-horizon final reserves.
    // The full-buffer arm's remaining work stays visible: it could be useful for a later job.
    reviewln!(
        "LIBERATION COST seed=0x{seed:016X} basis=matched-post-setup-same-finite-pipeline primary={}t scavenger={}t cleanup={}t direct-cleanup={}t total={}t charge=[demand:{}t full:{}t] generated=[demand:{}nJ full:{}nJ] retained=[demand:{}nJ full:{}nJ] scavenger-extra={}mg native-copper={}mg direct-native={}mg scavenger-marginal=[attention:{}t native:{}mg] output=identical-within-branch base-kit-cost=reported-separately",
        primary_ticks,
        scavenger_ticks,
        cleanup_ticks,
        direct_cleanup_ticks,
        elapsed,
        demand_ticks,
        full_ticks,
        demand_generated,
        full_generated,
        remaining(demand),
        remaining(full),
        scavenged.additional_recovered_copper_ppm_mg / 1_000_000,
        cleaned.native_copper_mass.milligrams(),
        direct_cleaned.native_copper_mass.milligrams(),
        scavenger_marginal_ticks,
        scavenger_marginal_native.milligrams(),
    );
    let kit = kit_acquisition.map_or_else(
        || "not-executed-this-sample".to_owned(),
        |review| {
            format!(
                "executed attention:{}t body:{}nJ/{}uL",
                review.attention_ticks, review.metabolic_cost_nj, review.hydration_cost_ul
            )
        },
    );
    let continuity = if kit_acquisition.is_some() {
        "live-kit-used"
    } else {
        "controlled-preassembled-kit"
    };
    let campaign = kit_acquisition.map_or_else(
        || format!(
            "planned:{planned_batches}batches kit-payback:not-applicable economics:not-applicable justified:not-applicable"
        ),
        |review| {
            let lifecycle = campaign_lifecycle.unwrap_or_else(|| {
                panic!("executed primitive kit requires an executed campaign lifecycle")
            });
            assert_eq!(
                lifecycle.batch_charge_ticks.len() as u64,
                planned_batches,
                "executed liberation campaign must cover its complete disclosed horizon"
            );
            let manual_campaign_attention = manual_recovery
                .attention_ticks
                .checked_mul(planned_batches)
                .unwrap_or_else(|| panic!("manual liberation campaign attention overflowed"));
            let mut cumulative_powered_attention = review.attention_ticks;
            let mut payback = None;
            for (index, charge_ticks) in lifecycle.batch_charge_ticks.iter().copied().enumerate() {
                cumulative_powered_attention = cumulative_powered_attention
                    .checked_add(charge_ticks)
                    .unwrap_or_else(|| panic!("powered liberation campaign attention overflowed"));
                let batches = u64::try_from(index + 1)
                    .unwrap_or_else(|_| panic!("liberation campaign batch index overflowed"));
                let manual_attention = manual_recovery
                    .attention_ticks
                    .checked_mul(batches)
                    .unwrap_or_else(|| panic!("manual liberation payback attention overflowed"));
                if payback.is_none() && cumulative_powered_attention <= manual_attention {
                    payback = Some(batches);
                }
            }
            let payback = payback.unwrap_or_else(|| {
                panic!("primitive processing kit did not repay within the disclosed campaign")
            });
            assert!(
                payback <= planned_batches,
                "primitive processing kit pays back in {payback} batches but only {planned_batches} were disclosed before build"
            );
            let powered_campaign_attention = cumulative_powered_attention;
            assert!(
                powered_campaign_attention <= manual_campaign_attention,
                "disclosed primitive campaign must not spend more player attention after its claimed payback"
            );
            let planned_batches_u128 = u128::from(planned_batches);
            let manual_campaign_metabolic = manual_recovery
                .metabolic_cost_nj
                .checked_mul(planned_batches_u128)
                .unwrap_or_else(|| panic!("manual liberation campaign metabolism overflowed"));
            let powered_campaign_metabolic = review
                .metabolic_cost_nj
                .checked_add(lifecycle.metabolic_cost_nj)
                .unwrap_or_else(|| panic!("powered liberation campaign metabolism overflowed"));
            let manual_campaign_hydration = manual_recovery
                .hydration_cost_ul
                .checked_mul(planned_batches)
                .unwrap_or_else(|| panic!("manual liberation campaign hydration overflowed"));
            let powered_campaign_hydration = review
                .hydration_cost_ul
                .checked_add(lifecycle.hydration_cost_ul)
                .unwrap_or_else(|| panic!("powered liberation campaign hydration overflowed"));
            format!(
                concat!(
                    "planned:{planned_batches}batches executed:{executed_batches} kit-payback:{payback}batches ",
                    "attention:manual:{manual_campaign_attention}t/powered:{powered_campaign_attention}t ",
                    "body:manual:{manual_campaign_metabolic}nJ/{manual_campaign_hydration}uL ",
                    "powered:{powered_campaign_metabolic}nJ/{powered_campaign_hydration}uL ",
                    "elapsed:{elapsed_ticks}t final-condition=[crusher:{crusher} quern:{quern} screen:{screen} separator:{separator} treadle:{treadle}] justified:true"
                ),
                planned_batches = planned_batches,
                executed_batches = lifecycle.batch_charge_ticks.len(),
                payback = payback,
                manual_campaign_attention = manual_campaign_attention,
                powered_campaign_attention = powered_campaign_attention,
                manual_campaign_metabolic = manual_campaign_metabolic,
                manual_campaign_hydration = manual_campaign_hydration,
                powered_campaign_metabolic = powered_campaign_metabolic,
                powered_campaign_hydration = powered_campaign_hydration,
                elapsed_ticks = lifecycle.elapsed_ticks,
                crusher = lifecycle.crusher_condition_ppm,
                quern = lifecycle.quern_condition_ppm,
                screen = lifecycle.screen_condition_ppm,
                separator = lifecycle.separator_condition_ppm,
                treadle = lifecycle.treadle_condition_ppm,
            )
        },
    );
    reviewln!(
        "LIBERATION ROUTE TRADEOFF seed=0x{seed:016X} basis=matched-ore-mass feed={}mg manual=[attention:{}t native:{}mg recovery:{}ppm body:{}nJ/{}uL] powered=[elapsed:{}t charge-attention:{}t native:{}mg] campaign=[{campaign}] sizing=timber-riddle copper-input=none next-screen-upgrade=proved-by-progression-continuation base-kit=[{kit}] continuity={continuity} interpretation=manual-is-low-infrastructure-fallback;powered-route-buys-recovery-and-reusable-throughput",
        manual_recovery.feed_mass.milligrams(),
        manual_recovery.attention_ticks,
        manual_recovery.recovered_native.milligrams(),
        manual_recovery.manual_recovery_ppm,
        manual_recovery.metabolic_cost_nj,
        manual_recovery.hydration_cost_ul,
        elapsed,
        demand_ticks,
        cleaned.native_copper_mass.milligrams(),
    );
    reviewln!(
        "LIBERATION PACING seed=0x{seed:016X} primary={} scavenger={} cleanup={} charge-body=[demand:{}nJ/{}uL full:{}nJ/{}uL] machine-time={}t parallel-work=covered-by-progression-probe interpretation=recovered-native-copper-pays-extra-dressing-cost",
        super::super::physical_time::format_physical_duration(registries, primary_ticks),
        super::super::physical_time::format_physical_duration(registries, scavenger_ticks),
        super::super::physical_time::format_physical_duration(registries, cleanup_ticks),
        demand_body.0,
        demand_body.1,
        full_body.0,
        full_body.1,
        elapsed - demand_ticks,
    );
}
