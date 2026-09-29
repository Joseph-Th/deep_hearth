//! Report-only formatting for primitive progression. Excluded from test targets.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{
    EQUIPMENT_STONE_MORTAR_AND_PESTLE, PROCESS_CONCENTRATE_COPPER, PROCESS_HAND_GRIND_CRUSHED_ORE,
};

use super::*;

fn report_manual_preparation_branch(registries: &Registries) {
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("progression report lost manual ore breaking"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("progression report lost manual native-copper sorting"));
    let grinding = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("progression report lost manual mortar grinding"));
    let concentration = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .unwrap_or_else(|| panic!("progression report lost copper concentration"));
    let coarse = grinding
        .input_particle_size_range()
        .unwrap_or_else(|| panic!("manual mortar grinding lost its coarse-feed range"));
    assert_eq!(coarse, breaking.output_particle_size());
    assert_eq!(sorting.input_particle_size_range(), coarse);
    assert_eq!(
        concentration.input_particle_size_range(),
        Some(grinding.output_particle_size())
    );
    let mortar_capability = grinding
        .operating_profile()
        .equipment_profile()
        .unwrap_or_else(|| panic!("manual mortar grinding lost its optional provider"))
        .mass_flow_capability();
    let mortar = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_MORTAR_AND_PESTLE)
        .unwrap_or_else(|| panic!("progression report lost stone mortar and pestle"));
    let Some(CapabilityValue::MassFlow(mortar_rate)) =
        mortar.capabilities().get_capability(mortar_capability)
    else {
        panic!("stone mortar lost its authored grinding throughput")
    };
    let fine = grinding.output_particle_size();
    reviewln!(
        "PROGRESSION PREPARATION BRANCH coarse={}..{}um choices=[hand-sort:recovery:{}ppm machinery:none; mortar-liberate:rate:{}->{}mg/s fine:{}..{}um then-concentrate:recovery:{}ppm separator:required] read=manual-grinding-can-replace-liberation-machine-but-not-downstream-separation",
        coarse.minimum_diameter().micrometers(),
        coarse.maximum_diameter().micrometers(),
        sorting.target_recovery_ppm(),
        grinding.processing_rate().milligrams_per_second(),
        mortar_rate.milligrams_per_second(),
        fine.minimum_diameter().micrometers(),
        fine.maximum_diameter().micrometers(),
        concentration.target_recovery_ppm(),
    );
}

fn report_maintained_manual_fallback(
    seed: u64,
    manual_fallback: Option<ManualProcessingFallbackReview>,
) {
    let Some(manual_fallback) = manual_fallback else {
        return;
    };
    reviewln!(
        "PROGRESSION FALLBACK seed=0x{seed:016X} evidence=maintained-route-regression route=owned-ore->hand-break->hand-sort->cold-work captured:true input=[ore:{}mg copper:{}ppm gangue-clay-share:{}ppm] attention=[break:{}t sort:{}t cold-work:{}t total:{}t] matter=[native:{}mg residue:{}mg reinforcement:{}mg remainder:{}mg] recovery=[manual:{}ppm powered:{}ppm] survival-cost=[{}nJ {}uL] machinery=none stored-work=none matter=conserved",
        manual_fallback.ore_mass_mg,
        manual_fallback.ore_copper_ppm,
        manual_fallback.gangue_clay_share_ppm,
        manual_fallback.break_ticks,
        manual_fallback.sort_ticks,
        manual_fallback.cold_work_ticks,
        manual_fallback.total_attention_ticks,
        manual_fallback.recovered_native_mg,
        manual_fallback.residue_mg,
        manual_fallback.reinforcement_mg,
        manual_fallback.native_remainder_mg,
        manual_fallback.manual_recovery_ppm,
        manual_fallback.powered_recovery_ppm,
        manual_fallback.metabolic_cost_nj,
        manual_fallback.hydration_cost_ul,
    );
}

fn overlap_setup_equivalent_label(review: &PrimitiveProgressionReview) -> String {
    review
        .overlap_setup_equivalent_cycles
        .map(|cycles| format!("{cycles}cycles"))
        .unwrap_or_else(|| {
            format!(
                "unreached-in-{}-executed-cycles",
                review.steady_state_cycles
            )
        })
}

fn automation_economics_label(review: &PrimitiveProgressionReview) -> &'static str {
    // Overlap measures scheduling, not avoided manual processing or downstream demand.
    match review.steady_state_stop {
        PrimitiveSteadyStop::TargetSupply => "supply-ended",
        PrimitiveSteadyStop::ToolCondition => "tool-lifetime-ended",
        PrimitiveSteadyStop::CrusherCondition => "crusher-lifetime-ended",
        PrimitiveSteadyStop::CrankCondition => "crank-lifetime-ended",
        PrimitiveSteadyStop::CycleLimit => "observation-ended",
        PrimitiveSteadyStop::StockpileOrderComplete => "finite-stockpile-order-complete",
    }
}

fn concise_reinvestment_summary(outcome: &PrimitiveReinvestmentOutcome) -> String {
    let reinvestment = match outcome {
        PrimitiveReinvestmentOutcome::Completed(reinvestment) => reinvestment,
        PrimitiveReinvestmentOutcome::TargetSupplyLimited => {
            return "blocked:known-target-supply".to_string();
        }
        PrimitiveReinvestmentOutcome::StorageCapacityLimited {
            available,
            required_above,
        } => {
            return format!(
                "blocked:crushed-storage available:{}mg requires-more-than:{}mg",
                available.milligrams(),
                required_above.milligrams()
            );
        }
    };
    format!(
        "completed copper-invested:{}mg executed=[crusher:{}->{}t separator-resolved:{}->{}t flow:{}->{}mg/s recovery:{}->{}mg separator-batch:{}->{}mg flywheel:{}->{}nJ expanded:crusher:{}mg/{}t separator:{}t]",
        reinvestment.invested_copper_mass.milligrams(),
        reinvestment.base_crush_ticks,
        reinvestment.reinforced_crush_ticks,
        reinvestment.base_separator_ticks,
        reinvestment.reinforced_separator_ticks,
        reinvestment
            .base_separator_processing_rate
            .milligrams_per_second(),
        reinvestment
            .reinforced_separator_processing_rate
            .milligrams_per_second(),
        reinvestment.base_separator_target_mass.milligrams(),
        reinvestment.reinforced_separator_target_mass.milligrams(),
        reinvestment.base_separator_batch_capacity.milligrams(),
        reinvestment.upgraded_separator_batch_capacity.milligrams(),
        reinvestment.base_drive_capacity.nanojoules(),
        reinvestment.upgraded_drive_capacity.nanojoules(),
        reinvestment.expanded_batch_mass.milligrams(),
        reinvestment.expanded_crush_ticks,
        reinvestment.expanded_separator_ticks,
    )
}

fn stockpile_demand_summary(outcome: &PrimitiveReinvestmentOutcome) -> String {
    match outcome {
        PrimitiveReinvestmentOutcome::Completed(work) => format!(
            "executed:{} basis:post-order-reinvestment-counterfactual purpose:upgrade-copper feed:{}mg stockpile:{}->{}mg recovered:{}mg separation:{}t charge:{}t process-energy:{}nJ scope:first-two-recoveries-before-new-crushing",
            work.stockpile_demand_executed,
            work.stockpile_demand_feed.milligrams(),
            work.stockpile_before_demand.milligrams(),
            work.stockpile_after_demand.milligrams(),
            work.stockpile_demand_copper.milligrams(),
            work.stockpile_demand_separation_ticks,
            work.stockpile_demand_charge_ticks,
            work.stockpile_demand_energy.nanojoules(),
        ),
        PrimitiveReinvestmentOutcome::TargetSupplyLimited
        | PrimitiveReinvestmentOutcome::StorageCapacityLimited { .. } => {
            "evidence:not-established basis:incomplete-reinvestment-counterfactual".to_string()
        }
    }
}

fn detailed_reinvestment_summary(outcome: &PrimitiveReinvestmentOutcome) -> String {
    let reinvestment = match outcome {
        PrimitiveReinvestmentOutcome::Completed(reinvestment) => reinvestment,
        PrimitiveReinvestmentOutcome::TargetSupplyLimited => {
            return "blocked:known-target-supply".to_string();
        }
        PrimitiveReinvestmentOutcome::StorageCapacityLimited {
            available,
            required_above,
        } => {
            return format!(
                "blocked:crushed-storage available:{}mg requires-more-than:{}mg",
                available.milligrams(),
                required_above.milligrams()
            );
        }
    };
    format!(
        "completed copper-invested:{}mg next-stage=[sizing-plate-continuation:{}t] executed=[crusher-time:{}->{}t reduction:{}ppm separator-resolved-time:{}->{}t flow:{}->{}mg/s gain:{}ppm separator-recovery:{}->{}mg separator-batch:{}->{}mg flywheel:{}->{}nJ expanded:[mass:{}mg crusher-energy:{}nJ charge:{}t crush:{}t separator-energy:{}nJ separator:{}t target:{}mg] survival:{}nJ/{}uL]",
        reinvestment.invested_copper_mass.milligrams(),
        reinvestment.sizing_plate_continuation_ticks,
        reinvestment.base_crush_ticks,
        reinvestment.reinforced_crush_ticks,
        reinvestment.crusher_time_reduction_ppm,
        reinvestment.base_separator_ticks,
        reinvestment.reinforced_separator_ticks,
        reinvestment
            .base_separator_processing_rate
            .milligrams_per_second(),
        reinvestment
            .reinforced_separator_processing_rate
            .milligrams_per_second(),
        reinvestment.separator_processing_rate_gain_ppm,
        reinvestment.base_separator_target_mass.milligrams(),
        reinvestment.reinforced_separator_target_mass.milligrams(),
        reinvestment.base_separator_batch_capacity.milligrams(),
        reinvestment.upgraded_separator_batch_capacity.milligrams(),
        reinvestment.base_drive_capacity.nanojoules(),
        reinvestment.upgraded_drive_capacity.nanojoules(),
        reinvestment.expanded_batch_mass.milligrams(),
        reinvestment.expanded_batch_energy.nanojoules(),
        reinvestment.expanded_charge_ticks,
        reinvestment.expanded_crush_ticks,
        reinvestment.expanded_separator_energy.nanojoules(),
        reinvestment.expanded_separator_ticks,
        reinvestment.expanded_separator_target_mass.milligrams(),
        reinvestment.survival_energy_spent_nj,
        reinvestment.survival_hydration_spent_ul,
    )
}

pub(super) struct PrimitiveProgressionReportContext<'a> {
    pub(super) seed: u64,
    pub(super) sample: &'static str,
    pub(super) manual_fallback: Option<ManualProcessingFallbackReview>,
    pub(super) maintained_reinvestment_required: bool,
    pub(super) extraction: &'a PrimitiveProgressionExperience,
    pub(super) mechanization: &'a PrimitiveProgressionExperience,
    pub(super) natural: &'a PrimitiveProgressionExperience,
    pub(super) extraction_pick_at: u64,
    pub(super) mechanization_pick_at: u64,
    pub(super) extraction_hard_at: u64,
    pub(super) mechanization_hard_at: u64,
    pub(super) reinforced_crank_power: Power,
    pub(super) primitive_flywheel_input_power: Power,
}

pub(super) fn report_primitive_progression_review(
    registries: &Registries,
    review: &PrimitiveProgressionReview,
    context: PrimitiveProgressionReportContext<'_>,
) {
    let PrimitiveProgressionReportContext {
        seed,
        sample,
        manual_fallback,
        maintained_reinvestment_required,
        extraction,
        mechanization,
        natural,
        extraction_pick_at,
        mechanization_pick_at,
        extraction_hard_at,
        mechanization_hard_at,
        reinforced_crank_power,
        primitive_flywheel_input_power,
    } = context;
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        let extraction_reinforced_mining_ticks = extraction
            .reinforced_mining_ticks
            .unwrap_or_else(|| panic!("pick-first never exercised its reinforced pick"));
        reviewln!(
            "PROGRESSION SEQUENCING seed=0x{seed:016X} first-investment=[extraction:pick@{}t hard-access@{}t hard-before-convergence:{}mg; mechanization:crank@{}t machine@{}t output@{}t] convergence=[extraction:{}t mechanization:{}t lead:{:+}t mechanization-pick:{}t hard-access:{}t] post-maturity-throughput=[hard-ore:{}vs{}mg total-ore:{}vs{}mg direct-second-upgrade-blocked:{}]",
            extraction.first_upgrade_at,
            extraction_hard_at,
            extraction.hard_ore_before_convergence.milligrams(),
            mechanization.first_upgrade_at,
            mechanization.machine_started_at,
            mechanization.first_processed_output_at,
            extraction.second_upgrade_at,
            mechanization.second_upgrade_at,
            review.mechanization_convergence_delta_ticks,
            mechanization_pick_at,
            mechanization_hard_at,
            extraction.hard_ore_mined.milligrams(),
            mechanization.hard_ore_mined.milligrams(),
            extraction.total_ore_mined.milligrams(),
            mechanization.total_ore_mined.milligrams(),
            extraction.direct_second_upgrade_blocked,
        );
        reviewln!(
            "PROGRESSION AGENCY seed=0x{seed:016X} matched-world branches=[pick-first,crank-first-counterfactual] milestones=[machine-start:{}vs{}t first-output:{}vs{}t second-upgrade:{}vs{}t] attention=[mining:stone:{}t reinforced:{}t reduction:{}ppm episode-charge:{}vs{}t full-accumulator:stone:{}t reinforced:{}t reduction:{}ppm] autonomy=[machine-total:{}t reserve-cycle:{}t initial-overlap:{}vs{}t feed-replenishment-overlap:{}vs{}t reserve-replenishment:{}vs{}t player-free:{}vs{}t] durability=[pick:{}vs{}ppm] survival=[energy:{}vs{}nJ hydration:{}vs{}uL] elapsed=[{}vs{}t]",
            extraction.machine_started_at,
            mechanization.machine_started_at,
            extraction.first_processed_output_at,
            mechanization.first_processed_output_at,
            extraction.second_upgrade_at,
            mechanization.second_upgrade_at,
            extraction.soft_ore_mining_ticks,
            extraction_reinforced_mining_ticks,
            review.tool_attention_reduction_ppm,
            extraction.charge_ticks,
            mechanization.charge_ticks,
            extraction.initial_full_charge_ticks,
            mechanization.initial_full_charge_ticks,
            review.crank_attention_reduction_ppm,
            extraction.machine_work_ticks,
            extraction.reserve_machine_work_ticks,
            extraction.overlap_ticks,
            mechanization.overlap_ticks,
            extraction.machine_useful_overlap_ticks,
            mechanization.machine_useful_overlap_ticks,
            extraction.reserve_useful_overlap_ticks,
            mechanization.reserve_useful_overlap_ticks,
            extraction.machine_player_free_ticks,
            mechanization.machine_player_free_ticks,
            extraction.final_pick_condition_ppm,
            mechanization.final_pick_condition_ppm,
            extraction.metabolic_energy_spent_nj,
            mechanization.metabolic_energy_spent_nj,
            extraction.hydration_spent_ul,
            mechanization.hydration_spent_ul,
            extraction.elapsed_ticks,
            mechanization.elapsed_ticks,
        );
    }
    let choice_windows_are_consequential = review.extraction_hard_material_window_ticks > 0
        && review.mechanization_processed_output_window_ticks > 0;
    let post_overlap_setup_equivalent_cycles = review
        .overlap_setup_equivalent_cycles
        .and_then(|payback| review.steady_state_cycles.checked_sub(payback))
        .unwrap_or(0);
    let fantasy_captured = regional_information_captured(review)
        && information_path_captured(review)
        && investment_choice_captured(review, choice_windows_are_consequential)
        && manual_bridge_evidence_captured(review)
        && automation_maturity_captured(review)
        && reinvestment_captured(registries, review, maintained_reinvestment_required);
    let maintenance_coverage_captured = lifecycle_obligations_captured(review);
    let overlap_setup_equivalent = overlap_setup_equivalent_label(review);
    let automation_economics = automation_economics_label(review);
    let physiology = registries.survival().physiology();
    let natural_energy_spent_ppm = u32::try_from(
        natural.metabolic_energy_spent_nj * 1_000_000
            / physiology.maximum_metabolic_energy().nanojoules(),
    )
    .unwrap_or_else(|_| unreachable!("bounded progression energy cost fits normalized ppm"));
    let natural_hydration_spent_ppm = u32::try_from(
        u128::from(natural.hydration_spent_ul) * 1_000_000
            / u128::from(physiology.maximum_hydration().microliters()),
    )
    .unwrap_or_else(|_| unreachable!("bounded progression hydration cost fits normalized ppm"));
    let unresolved_surface_clues = review
        .surface_clue_count
        .checked_sub(review.surface_resolved_clue_count)
        .unwrap_or_else(|| unreachable!("resolved clue count cannot exceed observed clue count"));
    let regional_priority =
        if review.regional_upper_bounds_ppm[0] == review.regional_upper_bounds_ppm[1] {
            "tied"
        } else {
            "ranked"
        };
    let extraction_window_time =
        format_physical_duration(registries, review.extraction_hard_material_window_ticks);
    let manual_bridge_time =
        format_physical_duration(registries, review.manual_bridge_attention_ticks);
    let machine_preparation_time =
        format_physical_duration(registries, review.processing_line_preparation_ticks);
    let manual_second_ready_delay = natural
        .manual_bridge_ready_at
        .checked_sub(natural.processing_decision_at)
        .unwrap_or_else(|| unreachable!("manual bridge cannot precede its decision state"));
    let milestone_delay = |tick: u64, label: &str| {
        tick.checked_sub(natural.processing_decision_at)
            .unwrap_or_else(|| {
                panic!("{label} cannot precede the shared processing decision state")
            })
    };
    let extraction_pick_delay = milestone_delay(extraction_pick_at, "pick-first pick");
    let extraction_hard_delay = milestone_delay(extraction_hard_at, "pick-first hard seam");
    let extraction_machine_start_delay =
        milestone_delay(extraction.machine_started_at, "pick-first machine start");
    let extraction_second_upgrade_delay =
        milestone_delay(extraction.second_upgrade_at, "pick-first second upgrade");
    let mechanization_crank_delay = milestone_delay(
        mechanization.first_upgrade_at,
        "crank-first counterfactual crank upgrade",
    );
    let mechanization_machine_start_delay = milestone_delay(
        mechanization.machine_started_at,
        "crank-first counterfactual machine start",
    );
    let mechanization_output_delay = milestone_delay(
        mechanization.first_processed_output_at,
        "crank-first counterfactual first processed output",
    );
    let mechanization_pick_delay = milestone_delay(
        mechanization_pick_at,
        "crank-first counterfactual pick upgrade",
    );
    let manual_bootstrap_feed = if natural.manual_bootstrap_selected_hard_feed {
        "hard-sample"
    } else {
        "owned-bulk"
    };
    let manual_bootstrap_hard_information_lead_ticks =
        mechanization_crank_delay.saturating_sub(natural.manual_bootstrap_hard_sample_ticks);
    let manual_bootstrap_automation_delay_ticks = natural
        .manual_bootstrap_machine_ready_ticks
        .saturating_sub(mechanization_machine_start_delay);
    let productive_setup_recovery_ticks = natural
        .machine_useful_overlap_ticks
        .min(natural.automation_preparation_ticks);
    let productive_setup_recovery_ppm = u32::try_from(
        u128::from(productive_setup_recovery_ticks) * 1_000_000
            / u128::from(natural.automation_preparation_ticks),
    )
    .unwrap_or_else(|_| unreachable!("bounded productive setup recovery fits normalized ppm"));
    let productive_setup_gap_ticks = natural
        .automation_preparation_ticks
        .saturating_sub(natural.machine_useful_overlap_ticks);
    let returned_attention_ppm =
        1_000_000_u32.saturating_sub(review.productive_autonomy_utilization_ppm);
    let feed_attention_ticks = natural
        .machine_useful_overlap_ticks
        .checked_sub(natural.maintenance_preparation_overlap_ticks)
        .unwrap_or_else(|| {
            panic!("maintenance preparation overlap exceeded total productive machine overlap")
        });
    let manual_disclosed_work_attention = natural
        .manual_bridge_attention_ticks
        .checked_add(natural.manual_stockpile_breaking_ticks)
        .unwrap_or_else(|| panic!("primitive manual disclosed-work attention overflowed"));
    let disclosed_work_attention_saved = manual_disclosed_work_attention
        .checked_sub(natural.mechanized_stockpile_player_ticks)
        .unwrap_or_else(|| {
            panic!("primitive disclosed workload no longer favors the mechanized route")
        });
    let reinvestment_summary = concise_reinvestment_summary(&review.reinvestment);
    let stockpile_demand = stockpile_demand_summary(&review.stockpiling_reinvestment);
    if let PrimitiveReinvestmentOutcome::Completed(immediate) = &review.reinvestment {
        let delayed_ticks = match &review.stockpiling_reinvestment {
            PrimitiveReinvestmentOutcome::Completed(delayed) => {
                let total = review.stockpiling_delay_ticks + delayed.elapsed_ticks;
                format!(
                    "{}t ({})",
                    total,
                    format_physical_duration(registries, total)
                )
            }
            PrimitiveReinvestmentOutcome::TargetSupplyLimited => "blocked:target-supply".to_owned(),
            PrimitiveReinvestmentOutcome::StorageCapacityLimited { .. } => {
                "blocked:storage-capacity".to_owned()
            }
        };
        reviewln!(
            "PROGRESSION GOAL seed=0x{seed:016X} basis=matched-start-completion-cost goal=three-machine-upgrades+expanded-batch immediate={}t ({}) delayed={} buffered-feed={}mg consumed-before-new-crushing={}mg invested-copper={}mg stockpiling-delay={}t terminal-reserves=unequal chosen=immediate execution=primary-state selected-maintenance=none comparison=stockpiling-plus-forced-service read=use-existing-feed-before-speculative-stockpiling",
            immediate.elapsed_ticks,
            format_physical_duration(registries, immediate.elapsed_ticks),
            delayed_ticks,
            immediate.stockpile_before_demand.milligrams(),
            immediate.stockpile_demand_feed.milligrams(),
            immediate.invested_copper_mass.milligrams(),
            review.stockpiling_delay_ticks,
        );
    }
    let selected = review.selected_end;
    if !matches!(
        review.reinvestment,
        PrimitiveReinvestmentOutcome::Completed(_)
    ) {
        let delayed = match &review.stockpiling_reinvestment {
            PrimitiveReinvestmentOutcome::Completed(work) => {
                format!("{}t", review.stockpiling_delay_ticks + work.elapsed_ticks)
            }
            PrimitiveReinvestmentOutcome::TargetSupplyLimited
            | PrimitiveReinvestmentOutcome::StorageCapacityLimited { .. } => {
                concise_reinvestment_summary(&review.stockpiling_reinvestment)
            }
        };
        reviewln!(
            "PROGRESSION GOAL seed=0x{seed:016X} basis=matched-start-completion-cost goal=three-machine-upgrades+expanded-batch immediate={} delayed={} immediate-spent={}t terminal-reserves=unequal chosen=immediate execution=primary-state read=blocked-goals-retain-partial-progress-not-rolled-back",
            concise_reinvestment_summary(&review.reinvestment),
            delayed,
            selected.completed_at - selected.decision_at,
        );
    }
    reviewln!(
        "PROGRESSION SELECTED seed=0x{seed:016X} policy=goal-driven-immediate-reinvestment decision={}t end={}t continuation={}t physical={} inventory=[crushed:{}mg native:{}mg] equipment=[crusher:{} separator:{} drive:{} pick-condition:{}ppm] survival-spent=[{}nJ {}uL] maintenance=none state=canonical-committed+trusted-load-validated",
        selected.decision_at,
        selected.completed_at,
        selected.completed_at - selected.decision_at,
        format_physical_duration(registries, selected.completed_at - selected.decision_at),
        selected.crushed_mass.milligrams(),
        selected.native_copper.milligrams(),
        selected.crusher_reinforced,
        selected.separator_reinforced,
        selected.drive_reinforced,
        selected.pick_condition_ppm,
        selected.metabolic_energy_spent_nj,
        selected.hydration_spent_ul,
    );
    report_maintained_manual_fallback(seed, manual_fallback);
    if sample == "anchor" {
        report_manual_preparation_branch(registries);
    }
    reviewln!(
        "PROGRESSION BUFFER seed=0x{seed:016X} evidence=stockpiling-coverage-counterfactual selected=false policy=two-upcoming-batches work-order={}cycles mining=[steady:{}jobs feed-ready:{}cycles capacity-blocked:{}cycles] machine={}t productive-attention=[feed:{}t maintenance-prep:{}t] returned-attention={}t payback=not-established outcome=stockpile-order demand=[{stockpile_demand}]",
        STOCKPILE_WORK_ORDER_CYCLES,
        review.steady_mining_jobs,
        review.steady_feed_buffer_ready_cycles,
        review.steady_feed_buffer_capacity_cycles,
        review.machine_work_ticks,
        feed_attention_ticks,
        review.maintenance_preparation_overlap_ticks,
        review.unfilled_autonomous_ticks,
    );
    reviewln!(
        "PROGRESSION EXPERIENCE seed=0x{seed:016X} sample={sample} information={} local-copper-sequence={} decision-evidence=[owned-bulk:{}ppm blocked-hard:{}..{}ppm rule=buy-access-only-if-hard-lower-beats-owned-bulk] scarcity=[direct-second-upgrade-blocked:{} processed-output-playable:{} converged-both-upgrades:{}] processing-investment=[selected:mechanized preaction-manual:{}t conservative-machine-upper:{}t assembly:{}t initial-charge:{}t repeated-charge-upper:{}t choice-frozen-before-action:true] matched-alternatives=[pick-first,crank-first] tradeoff=[hard-access-lead:{}t autonomous-output-window:{}t] portfolio-scope=pick-vs-crank-only pick-first=[pick:{}t hard-sample:{}t exclusive-hard-window:{}t/{}:{}mg machine:{}t crank:{}t] crank-first=[crank:{}t machine:{}t output:{}t pick:{}t eventual-convergence:{:+}t] bridge-tradeoff=[manual-second:{}t/{} feed:{}mg recovery:{}ppm body:{}nJ/{}uL; powered-line:{}t/{} feed:{}mg recovery:{}ppm body:{}nJ/{}uL] disclosed-order-economics=[cycles:{} manual-player-attention:{}t mechanized-player-attention:{}t saved:{}t] manual-second-counterfactual=[pick:{}t hard-sample:{}t second:{}t charged-line:{}t feed:{} trade=[hard-info-lead-vs-crank-first:{}t automation-delay:+{}t]] post-upgrade-feed={} stockpiling-coverage-delegation=[feed-attention:{}t maintenance-prep-overlap:{}t productive-attention:{}t returned-attention:{}t returned:{}ppm overlap/setup:{}ppm unrecovered-setup:{}t overlap-equivalent:{overlap_setup_equivalent} post-equivalent:{}cycles stop:{} economics:{automation_economics}] leverage=[pick-attention:-{}ppm crank-power:+{}ppm] selected-reinvestment=[{reinvestment_summary}] coverage-obligations=[maintenance-material-prep:{}t overlapped:{}t maintenance-service:{}t survival:{}ppm/{}ppm]",
        if review.information_refinement_required {
            "deferred-refinement"
        } else {
            "surface-resolved"
        },
        review.natural_priority.label(),
        review.bulk_sample_copper_ppm,
        review.hard_ore_evidence_lower_ppm,
        review.hard_ore_evidence_upper_ppm,
        review.direct_second_upgrade_blocked,
        review.processed_output_has_playable_acquisition_use,
        review.converged_both_upgrades,
        review.preaction_manual_processing_attention_ticks,
        review.preaction_mechanized_attention_upper_ticks,
        review.preaction_machine_assembly_ticks,
        review.preaction_machine_initial_charge_ticks,
        review.preaction_machine_repeated_charge_ticks,
        review.extraction_hard_access_lead_ticks,
        review.mechanization_processed_output_window_ticks,
        extraction_pick_delay,
        extraction_hard_delay,
        review.extraction_hard_material_window_ticks,
        extraction_window_time,
        review.extraction_hard_ore_before_convergence_mg,
        extraction_machine_start_delay,
        extraction_second_upgrade_delay,
        mechanization_crank_delay,
        mechanization_machine_start_delay,
        mechanization_output_delay,
        mechanization_pick_delay,
        review.mechanization_convergence_delta_ticks,
        review.manual_bridge_attention_ticks,
        manual_bridge_time,
        review.manual_bridge_feed_mg,
        review.manual_bridge_recovery_ppm,
        review.manual_bridge_metabolic_cost_nj,
        review.manual_bridge_hydration_cost_ul,
        review.processing_line_preparation_ticks,
        machine_preparation_time,
        natural.separation_feed_mass.milligrams(),
        registries
            .ore_processing()
            .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
            .map(ConstituentSeparationProcessDefinition::target_recovery_ppm)
            .unwrap_or_else(|| panic!("primitive powered sorting route disappeared during review")),
        review.processing_line_preparation_metabolic_cost_nj,
        review.processing_line_preparation_hydration_cost_ul,
        STOCKPILE_WORK_ORDER_CYCLES,
        manual_disclosed_work_attention,
        natural.mechanized_stockpile_player_ticks,
        disclosed_work_attention_saved,
        natural.manual_bootstrap_pick_ready_ticks,
        natural.manual_bootstrap_hard_sample_ticks,
        natural.manual_bootstrap_second_ready_ticks,
        natural.manual_bootstrap_machine_ready_ticks,
        manual_bootstrap_feed,
        manual_bootstrap_hard_information_lead_ticks,
        manual_bootstrap_automation_delay_ticks,
        if review.post_convergence_mining_target_is_hard {
            "hard-sample"
        } else {
            "owned-bulk"
        },
        feed_attention_ticks,
        review.maintenance_preparation_overlap_ticks,
        natural.machine_useful_overlap_ticks,
        review.unfilled_autonomous_ticks,
        returned_attention_ppm,
        productive_setup_recovery_ppm,
        productive_setup_gap_ticks,
        post_overlap_setup_equivalent_cycles,
        review.steady_state_stop.label(),
        review.tool_attention_reduction_ppm,
        review.crank_power_gain_ppm,
        review.maintenance_material_preparation_ticks,
        review.maintenance_preparation_overlap_ticks,
        review.component_service_ticks,
        natural_energy_spent_ppm,
        natural_hydration_spent_ppm,
    );
    let reinvestment_review = detailed_reinvestment_summary(&review.reinvestment);
    reviewln!(
        "PROGRESSION REVIEW seed=0x{seed:016X} sample={sample} role=runtime-experience-after-disclosed-bootstrap continuity=single-state fantasy=observe->infer->prepare->extract->invest->delegate->reassess->reinvest-when-justified captured:{fantasy_captured} knowledge=[path:{} regional:{}t zones:{} upper:[{},{}]ppm priority:{} local:{}t hardness-sampling:{}t clues:{} resolved:{} deferred:{} shortage-triggered-refinement:{} deferred-refinement:{}t alternative-evidence:{}..{}ppm] local-copper=[selected:{} rule=hard-lower>owned-bulk scope:pick-vs-crank-sequencing global-portfolio:not-claimed owned-bulk:{}ppm hard-evidence:{}..{}ppm] investment-effects=[pick-attention-reduction:{}ppm crank-power-gain:{}ppm crank-charge-attention-reduction:{}ppm] tradeoff=[pick-feed:{} pick-grade:{}ppm crank-first-grade:{}ppm efficiency-gain:{} avoided-worse-hard:{} hard-access-lead:{}t hard-window:{}t/{}mg crank-output-window:{}t autonomy-lead:{}t eventual-convergence:{:+}t converged:{}] strategy-timing=[pick-first=[pick:{}t hard-sample:{}t machine:{}t crank:{}t] crank-first=[crank:{}t machine:{}t output:{}t pick:{}t]] manual-second-counterfactual=[isolated:{}t pick:{}t hard-sample:{}t second:{}t charged-line:{}t feed:{} hard-info-lead-vs-crank-first:{}t automation-delay:+{}t manual-recovery:{}ppm powered-recovery:{}ppm] coverage-autonomy=[feed-attention:{}t maintenance-prep-overlap:{}t productive-attention:{}t returned-attention:{}t returned:{}ppm overlap/setup:{}ppm unrecovered-setup:{}t post-convergence-target:{} feed-actions=[primary:{}jobs/{} reserve:{}jobs/{} steady:{}jobs feed-ready:{}/{}cycles capacity-blocked:{}] overlap-setup-equivalent:{overlap_setup_equivalent} post-equivalent:{}cycles repeat-horizon:{}/{}cycles stop:{}] selected-reinvestment=[{reinvestment_review}] stored-work=[passive-loss:{}nJ reserve-recharge:{}t] coverage-maintenance=[captured:{maintenance_coverage_captured} pick:{}->{}ppm component:{}mg material-preparation:{}t overlapped:{}t service:{}t copper-upgrade-preserved:{}] survival-cost=[energy:{}ppm hydration:{}ppm elapsed:{}t]",
        if review.information_refinement_required {
            "deferred-survey"
        } else {
            "surface-resolved"
        },
        review.regional_recon_ticks,
        PROGRESSION_REGIONAL_ZONE_COUNT,
        review.regional_upper_bounds_ppm[0],
        review.regional_upper_bounds_ppm[1],
        regional_priority,
        review.surface_prospecting_ticks,
        review.hardness_sampling_ticks,
        review.surface_clue_count,
        review.surface_resolved_clue_count,
        unresolved_surface_clues,
        review.refinement_triggered_by_direct_shortage,
        review.detailed_survey_ticks,
        review.refined_coarse_lower_ppm,
        review.refined_coarse_upper_ppm,
        review.natural_priority.label(),
        review.bulk_sample_copper_ppm,
        review.hard_ore_evidence_lower_ppm,
        review.hard_ore_evidence_upper_ppm,
        review.tool_attention_reduction_ppm,
        review.crank_power_gain_ppm,
        review.crank_attention_reduction_ppm,
        if review.extraction_selected_hard_feed {
            "hard-sample"
        } else {
            "owned-bulk-after-reassessment"
        },
        review.extraction_feed_copper_ppm,
        review.mechanization_feed_copper_ppm,
        review.material_efficiency_tradeoff,
        review.extraction_reassessment_avoided_worse_feed,
        review.extraction_hard_access_lead_ticks,
        review.extraction_hard_material_window_ticks,
        review.extraction_hard_ore_before_convergence_mg,
        review.mechanization_processed_output_window_ticks,
        review.mechanization_autonomy_lead_ticks,
        review.mechanization_convergence_delta_ticks,
        review.converged_both_upgrades,
        extraction_pick_delay,
        extraction_hard_delay,
        extraction_machine_start_delay,
        extraction_second_upgrade_delay,
        mechanization_crank_delay,
        mechanization_machine_start_delay,
        mechanization_output_delay,
        mechanization_pick_delay,
        manual_second_ready_delay,
        natural.manual_bootstrap_pick_ready_ticks,
        natural.manual_bootstrap_hard_sample_ticks,
        natural.manual_bootstrap_second_ready_ticks,
        natural.manual_bootstrap_machine_ready_ticks,
        manual_bootstrap_feed,
        manual_bootstrap_hard_information_lead_ticks,
        manual_bootstrap_automation_delay_ticks,
        review.manual_bridge_recovery_ppm,
        registries
            .ore_processing()
            .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
            .map(ConstituentSeparationProcessDefinition::target_recovery_ppm)
            .unwrap_or_else(|| panic!("primitive powered sorting route disappeared during review")),
        feed_attention_ticks,
        review.maintenance_preparation_overlap_ticks,
        natural.machine_useful_overlap_ticks,
        review.unfilled_autonomous_ticks,
        returned_attention_ppm,
        productive_setup_recovery_ppm,
        productive_setup_gap_ticks,
        if review.post_convergence_mining_target_is_hard {
            "hard-sample"
        } else {
            "owned-bulk"
        },
        review.primary_mining_jobs,
        review.primary_autonomous_stop.label(),
        review.reserve_mining_jobs,
        review.reserve_autonomous_stop.label(),
        review.steady_mining_jobs,
        review.steady_feed_buffer_ready_cycles,
        review.steady_state_cycles,
        review.steady_feed_buffer_capacity_cycles,
        post_overlap_setup_equivalent_cycles,
        review.steady_state_cycles,
        MAX_STEADY_STATE_CRUSH_CYCLES,
        review.steady_state_stop.label(),
        review.flywheel_loss_before_reserve_nj,
        review.reserve_recharge_ticks,
        review.component_service_condition_before_ppm,
        review.final_pick_condition_ppm,
        review.component_service_mass_mg,
        review.maintenance_material_preparation_ticks,
        review.maintenance_preparation_overlap_ticks,
        review.component_service_ticks,
        review.component_service_preserved_reinforcement,
        natural_energy_spent_ppm,
        natural_hydration_spent_ppm,
        natural.elapsed_ticks,
    );
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        let reinforced_crank_power_microwatts = reinforced_crank_power
            .whole_microwatts()
            .unwrap_or_else(|| panic!("reinforced crank power must be whole microwatts"));
        let primitive_flywheel_input_power_microwatts = primitive_flywheel_input_power
            .whole_microwatts()
            .unwrap_or_else(|| panic!("primitive flywheel input power must be whole microwatts"));
        reviewln!(
            "PROGRESSION TRADEOFF seed=0x{seed:016X} evidence=matched-counterfactual same-decision-state:true authorship=distinct-physical-consequences pick-first=[unlock:hard-seam grade:{}ppm feed:{}mg separation-energy:{}nJ separation:{}t hard-window:{}t] crank-first-counterfactual=[feed-grade:{}ppm feed:{}mg separation-energy:{}nJ separation:{}t autonomy-lead:{}t first-output-delta:{:+}t crank:{}uW flywheel-input:{}uW unclipped:true full-charge-attention-reduction:{}ppm pre-pick-output-window:{}t] counterfactual-distinct:{} convergence=[both-upgrades:{} delta:{:+}t final-hard-ore:{}vs{}mg]",
            review.extraction_feed_copper_ppm,
            review.extraction_separation_feed_mg,
            review.extraction_separation_energy_nj,
            review.extraction_separation_ticks,
            review.extraction_hard_material_window_ticks,
            review.mechanization_feed_copper_ppm,
            review.mechanization_separation_feed_mg,
            review.mechanization_separation_energy_nj,
            review.mechanization_separation_ticks,
            review.mechanization_autonomy_lead_ticks,
            review.mechanization_output_delta_ticks,
            reinforced_crank_power_microwatts,
            primitive_flywheel_input_power_microwatts,
            review.crank_attention_reduction_ppm,
            review.mechanization_processed_output_window_ticks,
            review.sequencing_tradeoff,
            review.converged_both_upgrades,
            review.mechanization_convergence_delta_ticks,
            extraction.hard_ore_mined.milligrams(),
            mechanization.hard_ore_mined.milligrams(),
        );
        reviewln!(
            "PROGRESSION AUTONOMY seed=0x{seed:016X} evidence=stockpiling-coverage-counterfactual setup=[automation:{}t separator:{}t line:{}t] overlap-setup-equivalent=[{overlap_setup_equivalent} post-equivalent:{}cycles economic-payback:not-established] delegated-work=[machine:{}t necessary-feed-attention:{}t reserve-feed-attention:{}t returned-attention:{}t returned:{}ppm primary:{}jobs/{} reserve:{}jobs/{} steady:{}jobs feed-ready:{}/{}cycles capacity-blocked:{}] lifecycle=[cycles:{} stop:{} crusher-condition:{}ppm] branch-deltas=[returned-attention:{:+}t elapsed:{:+}t]",
            review.automation_preparation_ticks,
            review.separator_preparation_ticks,
            review.processing_line_preparation_ticks,
            post_overlap_setup_equivalent_cycles,
            review.machine_work_ticks,
            review.mechanization_useful_overlap_ticks,
            review.reserve_useful_overlap_ticks,
            review.unfilled_autonomous_ticks,
            returned_attention_ppm,
            review.primary_mining_jobs,
            review.primary_autonomous_stop.label(),
            review.reserve_mining_jobs,
            review.reserve_autonomous_stop.label(),
            review.steady_mining_jobs,
            review.steady_feed_buffer_ready_cycles,
            review.steady_state_cycles,
            review.steady_feed_buffer_capacity_cycles,
            review.steady_state_cycles,
            review.steady_state_stop.label(),
            review.final_crusher_condition_ppm,
            review.mechanization_player_free_delta_ticks,
            review.mechanization_elapsed_delta_ticks,
        );
    }
}
