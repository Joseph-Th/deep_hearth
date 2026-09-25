//! Adaptive multi-site recovery after the initial localized deposit cannot satisfy player demand.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::EquipmentId;

use super::campaign::decide_fieldwork_survey_strategy;
use super::extraction::{FieldworkExtractionOrder, execute_fieldwork_extraction};
use super::planning::fieldwork_mining_limits;
use super::preparation::upgrade_sampling_hammer;
use super::retooling::{
    FieldworkOreRecoveryReason, FieldworkOwnedOreRecovery, FieldworkSiteToolRequest,
    prepare_fieldwork_tool_for_site,
};
use super::review::FieldworkEpisodeReview;
use super::survey::{FOLLOWUP_CHANNEL_STARTS, FieldworkSurveyStrategy, localize_target};

pub(super) struct InitialShortfallRecovery {
    pub(super) strategy: FieldworkSurveyStrategy,
    pub(super) upgrade_ticks: u64,
    pub(super) projected_point_search_ticks: u64,
    pub(super) projected_indexed_search_ticks: Option<u64>,
    pub(super) baseline_search_ticks: u64,
    pub(super) baseline_fulfilled: Mass,
    pub(super) realized_search_attention_delta: i128,
    pub(super) realized_total_attention_delta: i128,
    pub(super) sites_visited: u64,
    pub(super) search_ticks: u64,
    pub(super) tool_preparation_ticks: u64,
    pub(super) extraction_ticks: u64,
    pub(super) tool_builds: u64,
    pub(super) tool_switches: u64,
    pub(super) blocked_sites: u64,
    pub(super) hardness_tier_changes: u64,
    pub(super) salvage_retools: u64,
    pub(super) ore_recovery_events: u64,
    pub(super) ore_recovery_required_access: u64,
    pub(super) ore_recovery_payback: u64,
    pub(super) ore_recovery_ticks: u64,
    pub(super) ore_feed_mass: Mass,
    pub(super) recovered_native: Mass,
    pub(super) additional_extracted: Mass,
    pub(super) fulfilled: Mass,
    pub(super) remaining: Mass,
    pub(super) terminal: &'static str,
}

#[derive(Clone, Copy)]
struct InitialShortfallRun {
    sites_visited: u64,
    search_ticks: u64,
    tool_preparation_ticks: u64,
    extraction_ticks: u64,
    tool_builds: u64,
    tool_switches: u64,
    blocked_sites: u64,
    hardness_tier_changes: u64,
    salvage_retools: u64,
    ore_recovery_events: u64,
    ore_recovery_required_access: u64,
    ore_recovery_payback: u64,
    ore_recovery_ticks: u64,
    ore_feed_mass: Mass,
    recovered_native: Mass,
    additional_extracted: Mass,
    fulfilled: Mass,
    remaining: Mass,
}

struct RecoveryProgress {
    remaining: Mass,
    sites_visited: u64,
    search_ticks: u64,
    tool_preparation_ticks: u64,
    extraction_ticks: u64,
    tool_builds: u64,
    tool_switches: u64,
    blocked_sites: u64,
    hardness_tier_changes: u64,
    salvage_retools: u64,
    ore_recovery_events: u64,
    ore_recovery_required_access: u64,
    ore_recovery_payback: u64,
    ore_recovery_ticks: u64,
    ore_feed_mass: Mass,
    recovered_native: Mass,
    additional_extracted: Mass,
    owned_equipment: Vec<EquipmentId>,
    current_tool_label: &'static str,
    previous_hardness_tier: u8,
}

impl RecoveryProgress {
    fn new(review: &FieldworkEpisodeReview<'_>) -> Self {
        let remaining = review
            .requested
            .checked_sub(review.extraction.extracted)
            .unwrap_or_else(|| {
                unreachable!("initial fieldwork extraction cannot exceed its order")
            });
        assert!(
            !remaining.is_zero(),
            "initial shortfall recovery requires unfinished player demand"
        );
        Self {
            remaining,
            sites_visited: 0,
            search_ticks: 0,
            tool_preparation_ticks: 0,
            extraction_ticks: 0,
            tool_builds: 0,
            tool_switches: 0,
            blocked_sites: 0,
            hardness_tier_changes: 0,
            salvage_retools: 0,
            ore_recovery_events: 0,
            ore_recovery_required_access: 0,
            ore_recovery_payback: 0,
            ore_recovery_ticks: 0,
            ore_feed_mass: Mass::ZERO,
            recovered_native: Mass::ZERO,
            additional_extracted: Mass::ZERO,
            owned_equipment: vec![review.mining_equipment],
            current_tool_label: review.estimate.tool.label,
            previous_hardness_tier: hardness_tier(
                review.registries,
                review.observed_hardness.upper(),
            ),
        }
    }

    fn record_tool_choice(&mut self, tool: &super::retooling::FieldworkSiteToolChoice) {
        if let Some(salvaged) = tool.salvaged_equipment {
            self.salvage_retools += 1;
            self.owned_equipment
                .retain(|&equipment| equipment != salvaged);
        }
        if tool.ore_recovery_ticks != 0 {
            self.ore_recovery_events += 1;
            match tool.ore_recovery_reason {
                Some(FieldworkOreRecoveryReason::RequiredAccess) => {
                    self.ore_recovery_required_access += 1;
                }
                Some(FieldworkOreRecoveryReason::Payback) => {
                    self.ore_recovery_payback += 1;
                }
                None => panic!("fieldwork ore-funded tool choice lost its decision reason"),
            }
            self.ore_recovery_ticks = self
                .ore_recovery_ticks
                .checked_add(tool.ore_recovery_ticks)
                .unwrap_or_else(|| panic!("fieldwork recovery ore-processing time overflowed"));
            self.ore_feed_mass = self
                .ore_feed_mass
                .checked_add(tool.ore_feed_mass)
                .unwrap_or_else(|| panic!("fieldwork recovery ore feed overflowed"));
            self.recovered_native = self
                .recovered_native
                .checked_add(tool.recovered_native)
                .unwrap_or_else(|| panic!("fieldwork recovery native copper overflowed"));
        }
        if !tool.reused_existing {
            self.tool_builds += 1;
            self.owned_equipment.push(tool.equipment);
            self.tool_preparation_ticks = self
                .tool_preparation_ticks
                .checked_add(tool.preparation_ticks)
                .unwrap_or_else(|| panic!("fieldwork recovery tool preparation overflowed"));
        }
        if tool.label != self.current_tool_label {
            self.tool_switches += 1;
        }
        self.current_tool_label = tool.label;
    }

    fn finish(self, review: &FieldworkEpisodeReview<'_>) -> InitialShortfallRun {
        let fulfilled = review
            .extraction
            .extracted
            .checked_add(self.additional_extracted)
            .unwrap_or_else(|| panic!("fieldwork multi-site fulfilled mass overflowed"));
        assert_eq!(
            fulfilled.checked_add(self.remaining),
            Some(review.requested)
        );
        InitialShortfallRun {
            sites_visited: self.sites_visited,
            search_ticks: self.search_ticks,
            tool_preparation_ticks: self.tool_preparation_ticks,
            extraction_ticks: self.extraction_ticks,
            tool_builds: self.tool_builds,
            tool_switches: self.tool_switches,
            blocked_sites: self.blocked_sites,
            hardness_tier_changes: self.hardness_tier_changes,
            salvage_retools: self.salvage_retools,
            ore_recovery_events: self.ore_recovery_events,
            ore_recovery_required_access: self.ore_recovery_required_access,
            ore_recovery_payback: self.ore_recovery_payback,
            ore_recovery_ticks: self.ore_recovery_ticks,
            ore_feed_mass: self.ore_feed_mass,
            recovered_native: self.recovered_native,
            additional_extracted: self.additional_extracted,
            fulfilled,
            remaining: self.remaining,
        }
    }
}

fn hardness_tier(
    registries: &deep_hearth::registry::Registries,
    upper: deep_hearth::core::quantity::Pressure,
) -> u8 {
    let limits = fieldwork_mining_limits(registries);
    if upper <= limits.base_quarry_hardness {
        0
    } else if upper <= limits.reinforced_quarry_hardness {
        1
    } else {
        2
    }
}

fn recover_site(
    review: &FieldworkEpisodeReview<'_>,
    state: &mut AppState,
    strategy: FieldworkSurveyStrategy,
    start_x: i64,
    progress: &mut RecoveryProgress,
) {
    let search_started_at = state.tick().value();
    let localization = localize_target(
        review.registries,
        state,
        review.sampling_hammer,
        review.channel_voxels,
        start_x,
        strategy,
    );
    progress.search_ticks = progress
        .search_ticks
        .checked_add(state.tick().value() - search_started_at)
        .unwrap_or_else(|| panic!("fieldwork multi-site recovery search time overflowed"));
    progress.sites_visited += 1;

    let site_hardness_tier = hardness_tier(review.registries, localization.hardness.upper());
    if site_hardness_tier != progress.previous_hardness_tier {
        progress.hardness_tier_changes += 1;
    }
    progress.previous_hardness_tier = site_hardness_tier;
    let requested = progress.remaining.min(localization.resource_mass.upper());
    assert!(
        !requested.is_zero(),
        "fieldwork multi-site recovery must resolve a nonzero local opportunity"
    );
    let Some(tool) = prepare_fieldwork_tool_for_site(
        review.registries,
        state,
        FieldworkSiteToolRequest::new(
            review.raw,
            review.parts,
            FieldworkOwnedOreRecovery {
                ore_source: review.ore_source,
                crushed_destination: review.recovery_crushed,
                residue_destination: review.recovery_residue,
            },
            &progress.owned_equipment,
            localization.hardness.upper(),
            requested,
        ),
    ) else {
        progress.blocked_sites += 1;
        return;
    };
    progress.record_tool_choice(&tool);
    let extraction = execute_fieldwork_extraction(
        review.registries,
        state,
        FieldworkExtractionOrder {
            target: localization.target,
            destination: review.followup_destination,
            equipment: tool.equipment,
            requested,
            batch_limit: tool.batch,
        },
    );
    assert!(
        !extraction.extracted.is_zero(),
        "fieldwork multi-site recovery must make productive progress"
    );
    if extraction.stop == super::extraction::FieldworkStop::OrderComplete {
        assert_eq!(
            extraction.ticks, tool.projected_order_ticks,
            "completed fieldwork recovery order must match its canonical pre-action projection"
        );
    }
    progress.extraction_ticks = progress
        .extraction_ticks
        .checked_add(extraction.ticks)
        .unwrap_or_else(|| panic!("fieldwork multi-site recovery extraction time overflowed"));
    progress.additional_extracted = progress
        .additional_extracted
        .checked_add(extraction.extracted)
        .unwrap_or_else(|| panic!("fieldwork multi-site recovery mass overflowed"));
    progress.remaining = progress
        .remaining
        .checked_sub(extraction.extracted)
        .unwrap_or_else(|| unreachable!("recovery extraction cannot exceed remaining demand"));
}

fn run_initial_shortfall_sites(
    review: &FieldworkEpisodeReview<'_>,
    state: &mut AppState,
    strategy: FieldworkSurveyStrategy,
) -> InitialShortfallRun {
    let mut progress = RecoveryProgress::new(review);
    for start_x in FOLLOWUP_CHANNEL_STARTS {
        if progress.remaining.is_zero() {
            break;
        }
        recover_site(review, state, strategy, start_x, &mut progress);
    }

    validate_loaded_state(review.registries, state)
        .unwrap_or_else(|error| panic!("fieldwork multi-site recovery state invalid: {error}"));
    progress.finish(review)
}

pub(super) fn execute_initial_shortfall_recovery(
    review: &FieldworkEpisodeReview<'_>,
) -> InitialShortfallRecovery {
    let planned_sites = u64::try_from(FOLLOWUP_CHANNEL_STARTS.len())
        .unwrap_or_else(|_| unreachable!("bounded fieldwork recovery horizon fits u64"));
    let survey_decision = decide_fieldwork_survey_strategy(
        review.registries,
        review.state,
        review.raw,
        review.parts,
        review.channel_voxels,
        planned_sites,
    );

    // Freeze policy from the actor-visible projection above. The matched point-search branch below
    // is diagnostic evidence only; its realized result must never reselect the committed strategy.
    let mut baseline_state = review.state.clone();
    let baseline = run_initial_shortfall_sites(
        review,
        &mut baseline_state,
        FieldworkSurveyStrategy::PointSearch,
    );
    let (selected, upgrade_ticks) =
        if survey_decision.selected_strategy == FieldworkSurveyStrategy::IndexedChannel {
            let mut selected_state = review.state.clone();
            let actual = upgrade_sampling_hammer(
                review.registries,
                &mut selected_state,
                review.raw,
                review.parts,
                review.sampling_hammer,
            );
            assert_eq!(Some(actual), survey_decision.projected_upgrade_ticks);
            (
                run_initial_shortfall_sites(
                    review,
                    &mut selected_state,
                    FieldworkSurveyStrategy::IndexedChannel,
                ),
                actual,
            )
        } else {
            (baseline, 0)
        };
    let selected_search_attention = selected
        .search_ticks
        .checked_add(upgrade_ticks)
        .unwrap_or_else(|| panic!("fieldwork shortfall selected attention overflowed"));
    let baseline_total_attention = baseline
        .search_ticks
        .checked_add(baseline.tool_preparation_ticks)
        .and_then(|ticks| ticks.checked_add(baseline.extraction_ticks))
        .unwrap_or_else(|| panic!("fieldwork shortfall baseline attention overflowed"));
    let selected_total_attention = selected_search_attention
        .checked_add(selected.tool_preparation_ticks)
        .and_then(|ticks| ticks.checked_add(selected.extraction_ticks))
        .unwrap_or_else(|| panic!("fieldwork shortfall selected total attention overflowed"));

    InitialShortfallRecovery {
        strategy: survey_decision.selected_strategy,
        upgrade_ticks,
        projected_point_search_ticks: survey_decision.projected_point_search_ticks,
        projected_indexed_search_ticks: survey_decision.projected_indexed_search_ticks,
        baseline_search_ticks: baseline.search_ticks,
        baseline_fulfilled: baseline.fulfilled,
        realized_search_attention_delta: i128::from(baseline.search_ticks)
            - i128::from(selected_search_attention),
        realized_total_attention_delta: i128::from(baseline_total_attention)
            - i128::from(selected_total_attention),
        sites_visited: selected.sites_visited,
        search_ticks: selected.search_ticks,
        tool_preparation_ticks: selected.tool_preparation_ticks,
        extraction_ticks: selected.extraction_ticks,
        tool_builds: selected.tool_builds,
        tool_switches: selected.tool_switches,
        blocked_sites: selected.blocked_sites,
        hardness_tier_changes: selected.hardness_tier_changes,
        salvage_retools: selected.salvage_retools,
        ore_recovery_events: selected.ore_recovery_events,
        ore_recovery_required_access: selected.ore_recovery_required_access,
        ore_recovery_payback: selected.ore_recovery_payback,
        ore_recovery_ticks: selected.ore_recovery_ticks,
        ore_feed_mass: selected.ore_feed_mass,
        recovered_native: selected.recovered_native,
        additional_extracted: selected.additional_extracted,
        fulfilled: selected.fulfilled,
        remaining: selected.remaining,
        terminal: if selected.remaining.is_zero() {
            "order-complete"
        } else {
            "local-search-area-exhausted"
        },
    }
}
