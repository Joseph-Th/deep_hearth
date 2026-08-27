//! Matched-counterfactual progression review, evidence classification, and report output.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrimitiveProgressionReview {
    prospecting_ticks: u64,
    surface_prospecting_ticks: u64,
    detailed_survey_ticks: u64,
    surface_clue_count: u8,
    surface_resolved_clue_count: u8,
    information_refinement_required: bool,
    refinement_triggered_by_direct_shortage: bool,
    refined_coarse_lower_ppm: u32,
    refined_coarse_upper_ppm: u32,
    refined_detailed_lower_ppm: u32,
    refined_detailed_upper_ppm: u32,
    refined_sample_copper_ppm: u32,
    refined_sample_is_ore: bool,
    stone_mineable_clue_count: u8,
    hardness_blocked_clue_count: u8,
    direct_copper_evidence_lower_ppm: u32,
    direct_copper_evidence_upper_ppm: u32,
    bulk_ore_evidence_lower_ppm: u32,
    bulk_ore_evidence_upper_ppm: u32,
    bulk_sample_copper_ppm: u32,
    processing_feed_selected_from_bulk: bool,
    direct_second_upgrade_blocked: bool,
    refined_clue_sample_mg: u64,
    refined_clue_mining_ticks: u64,
    tool_attention_reduction_ppm: u32,
    processed_output_has_playable_acquisition_use: bool,
    extraction_feed_copper_ppm: u32,
    mechanization_feed_copper_ppm: u32,
    extraction_separation_feed_mg: u64,
    mechanization_separation_feed_mg: u64,
    recovered_copper_mg: u64,
    extraction_separation_energy_nj: u128,
    mechanization_separation_energy_nj: u128,
    extraction_separation_ticks: u64,
    mechanization_separation_ticks: u64,
    material_efficiency_tradeoff: bool,
    crank_power_gain_ppm: u32,
    crank_attention_reduction_ppm: u32,
    extraction_hard_access_lead_ticks: u64,
    extraction_hard_material_window_ticks: u64,
    mechanization_processed_output_window_ticks: u64,
    mechanization_autonomy_lead_ticks: u64,
    mechanization_output_delta_ticks: i128,
    mechanization_convergence_delta_ticks: i128,
    extraction_hard_ore_before_convergence_mg: u64,
    sequencing_tradeoff: bool,
    converged_both_upgrades: bool,
    mechanization_processed_before_pick_upgrade: bool,
    automation_preparation_ticks: u64,
    separator_preparation_ticks: u64,
    processing_line_preparation_ticks: u64,
    attention_payback_cycles: Option<u64>,
    steady_state_cycles: u64,
    steady_state_stop: PrimitiveSteadyStop,
    final_crusher_condition_ppm: u32,
    machine_work_ticks: u64,
    reserve_machine_work_ticks: u64,
    mechanization_useful_overlap_ticks: u64,
    reserve_useful_overlap_ticks: u64,
    returned_player_free_ticks: u64,
    mechanization_player_free_delta_ticks: i128,
    mechanization_elapsed_delta_ticks: i128,
}

fn tick_delta(left: u64, right: u64) -> i128 {
    i128::from(left) - i128::from(right)
}

fn attention_reduction_ppm(baseline_ticks: u64, improved_ticks: u64) -> u32 {
    assert!(baseline_ticks > 0 && improved_ticks <= baseline_ticks);
    u32::try_from(
        u128::from(baseline_ticks - improved_ticks) * 1_000_000 / u128::from(baseline_ticks),
    )
    .unwrap_or_else(|_| unreachable!("bounded attention reduction ratio fits u32"))
}

fn nominal_manual_power(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> Power {
    let capability = registries
        .labor()
        .get_manual_power(MANUAL_POWER_HAND_CRANK)
        .map(|definition| definition.power_capability())
        .unwrap_or_else(|| panic!("primitive progression manual-power definition disappeared"));
    let value = registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|definition| definition.capabilities().get_capability(capability))
        .unwrap_or_else(|| {
            panic!(
                "primitive progression equipment {} lost manual-power capability {}",
                equipment.value(),
                capability.value()
            )
        });
    match value {
        CapabilityValue::Power(power) => power,
        other => panic!(
            "primitive progression equipment {} manual-power capability has wrong kind {:?}",
            equipment.value(),
            other.kind()
        ),
    }
}

fn relative_power_gain_ppm(base: Power, upgraded: Power) -> u32 {
    let base = base.whole_microwatts();
    let upgraded = upgraded.whole_microwatts();
    assert!(base > 0 && upgraded > base);
    u32::try_from((upgraded - base) * 1_000_000 / base)
        .unwrap_or_else(|_| panic!("primitive manual-power gain exceeds report range"))
}

fn evaluate_primitive_progression_probe(
    registries: &Registries,
    seed: u64,
) -> PrimitiveProgressionReview {
    assert_playable_catalog_coverage(registries);
    let selected_priority = primitive_priority(seed);
    let selected = run_primitive_progression_case(registries, seed, selected_priority, true);
    let alternative =
        run_primitive_progression_case(registries, seed, selected_priority.opposite(), false);
    let (extraction, mechanization) = match selected.priority {
        PrimitivePriority::ExtractionFirst => (selected, alternative),
        PrimitivePriority::MechanizationFirst => (alternative, selected),
    };

    let extraction_pick_at = extraction
        .pick_upgraded_at
        .unwrap_or_else(|| panic!("extraction-first never acquired its pick reinforcement"));
    let mechanization_pick_at = mechanization
        .pick_upgraded_at
        .unwrap_or_else(|| panic!("mechanization-first never converged on the pick reinforcement"));
    let extraction_hard_at = extraction.hard_seam_accessed_at.unwrap_or_else(|| {
        panic!("extraction-first pick upgrade failed to unlock the known hard seam")
    });
    let mechanization_hard_at = mechanization.hard_seam_accessed_at.unwrap_or_else(|| {
        panic!("mechanization-first failed to reach the hard seam after convergence")
    });
    let extraction_reinforced_mining_ticks = extraction
        .reinforced_mining_ticks
        .unwrap_or_else(|| panic!("extraction-first never exercised its reinforced pick"));
    let mechanization_reinforced_mining_ticks = mechanization
        .reinforced_mining_ticks
        .unwrap_or_else(|| panic!("mechanization-first never exercised its reinforced pick"));
    assert!(!extraction.initial_crank_reinforced);
    assert!(mechanization.initial_crank_reinforced);
    assert!(extraction.crank_reinforced && mechanization.crank_reinforced);
    assert_eq!(extraction.first_upgrade_at, extraction_pick_at);
    assert_eq!(
        extraction.first_upgrade_at, mechanization.first_upgrade_at,
        "matched-world priorities must allocate the scarce copper parcel from the same prepared decision state"
    );
    assert!(mechanization.first_upgrade_at < mechanization_pick_at);
    assert!(extraction.second_upgrade_at > extraction.first_upgrade_at);
    assert!(mechanization.second_upgrade_at > mechanization.first_upgrade_at);
    assert!(
        extraction.separation_completed_at < extraction.second_upgrade_at
            && mechanization.separation_completed_at < mechanization.second_upgrade_at,
        "both matched-world branches must derive the second upgrade from completed ore separation"
    );
    assert!(
        extraction.direct_second_upgrade_blocked && mechanization.direct_second_upgrade_blocked,
        "matched-world branches must independently discover the same direct-copper supply limit"
    );
    assert!(
        mechanization.machine_started_at < extraction.machine_started_at,
        "mechanization-first must deliver autonomous work earlier on the same world"
    );
    assert!(
        extraction_reinforced_mining_ticks < extraction.soft_ore_mining_ticks,
        "pick reinforcement must reduce actual mining attention"
    );
    assert!(mechanization_reinforced_mining_ticks < mechanization.soft_ore_mining_ticks);
    assert_eq!(extraction.hard_ore_mined, mechanization.hard_ore_mined);
    assert_eq!(extraction.total_ore_mined, mechanization.total_ore_mined);
    assert!(extraction.hard_ore_before_convergence > Mass::ZERO);
    assert_eq!(mechanization.hard_ore_before_convergence, Mass::ZERO);
    assert_eq!(
        extraction.machine_work_ticks, mechanization.machine_work_ticks,
        "matched-world priorities must compare the same autonomous crusher workload"
    );
    assert_eq!(
        extraction.reserve_machine_work_ticks, mechanization.reserve_machine_work_ticks,
        "matched-world priorities must compare the same banked follow-up crusher workload"
    );
    assert_eq!(
        extraction.primary_batch_mass, mechanization.primary_batch_mass,
        "matched-world priorities must compare the same primary crusher batch"
    );
    assert_eq!(
        extraction.steady_state_cycles, mechanization.steady_state_cycles,
        "matched-world priorities must reach the same primitive crusher lifecycle endpoint"
    );
    assert_eq!(
        extraction.steady_state_stop, mechanization.steady_state_stop,
        "matched-world priorities must observe the same primitive lifecycle stop reason"
    );
    assert_eq!(
        extraction.final_crusher_condition_ppm, mechanization.final_crusher_condition_ppm,
        "matched-world priorities must finish repeated crusher work at the same condition"
    );
    assert_eq!(
        extraction.prospecting_ticks, mechanization.prospecting_ticks,
        "matched-world priorities must pay the same geological-information acquisition cost"
    );
    assert_eq!(
        extraction.surface_prospecting_ticks, mechanization.surface_prospecting_ticks,
        "matched-world priorities must observe the same cheap geological evidence"
    );
    assert_eq!(
        extraction.detailed_survey_ticks, mechanization.detailed_survey_ticks,
        "matched-world priorities must pay the same geological refinement cost"
    );
    assert_eq!(
        extraction.information_refinement_required, mechanization.information_refinement_required,
        "matched-world priorities must face the same geological ambiguity"
    );
    assert_eq!(
        (
            extraction.surface_clue_count,
            extraction.surface_resolved_clue_count,
            extraction.refinement_triggered_by_direct_shortage,
            extraction.refined_coarse_lower_ppm,
            extraction.refined_coarse_upper_ppm,
            extraction.refined_detailed_lower_ppm,
            extraction.refined_detailed_upper_ppm,
            extraction.refined_sample_copper_ppm,
            extraction.refined_sample_is_ore,
        ),
        (
            mechanization.surface_clue_count,
            mechanization.surface_resolved_clue_count,
            mechanization.refinement_triggered_by_direct_shortage,
            mechanization.refined_coarse_lower_ppm,
            mechanization.refined_coarse_upper_ppm,
            mechanization.refined_detailed_lower_ppm,
            mechanization.refined_detailed_upper_ppm,
            mechanization.refined_sample_copper_ppm,
            mechanization.refined_sample_is_ore,
        ),
        "matched-world priorities must see the same deferred geological refinement and sample"
    );
    assert_eq!(
        (
            extraction.stone_mineable_clue_count,
            extraction.hardness_blocked_clue_count,
            extraction.direct_copper_evidence_lower_ppm,
            extraction.direct_copper_evidence_upper_ppm,
            extraction.bulk_ore_evidence_lower_ppm,
            extraction.bulk_ore_evidence_upper_ppm,
            extraction.bulk_sample_copper_ppm,
            extraction.processing_feed_selected_from_bulk,
        ),
        (
            mechanization.stone_mineable_clue_count,
            mechanization.hardness_blocked_clue_count,
            mechanization.direct_copper_evidence_lower_ppm,
            mechanization.direct_copper_evidence_upper_ppm,
            mechanization.bulk_ore_evidence_lower_ppm,
            mechanization.bulk_ore_evidence_upper_ppm,
            mechanization.bulk_sample_copper_ppm,
            mechanization.processing_feed_selected_from_bulk,
        ),
        "matched-world priorities must make their branch choice from the same observable mining affordances and sampled processing feed"
    );
    assert_eq!(
        extraction.refined_clue_sample_mass, mechanization.refined_clue_sample_mass,
        "matched-world priorities must extract the same information-unlocked sample"
    );
    assert_eq!(
        extraction.refined_clue_mining_ticks, mechanization.refined_clue_mining_ticks,
        "matched-world priorities must pay the same information-unlocked extraction time"
    );
    assert!(
        extraction.selected_processing_feed_is_hard
            && !mechanization.selected_processing_feed_is_hard,
        "matched-world branch treatment must make extraction-first use the newly unlocked hard-seam feed while mechanization-first uses the already accessible bulk feed"
    );
    assert!(
        extraction.selected_processing_feed_copper_ppm
            > mechanization.selected_processing_feed_copper_ppm,
        "reinforced extraction must reveal and use higher-grade ore than the mechanization-first branch can access before convergence"
    );
    assert!(
        extraction.separation_feed_mass < mechanization.separation_feed_mass,
        "higher-grade hard-seam feed must recover the same second-upgrade copper from less processed matter"
    );
    assert_eq!(
        extraction.recovered_copper_mass, mechanization.recovered_copper_mass,
        "matched-world priorities must recover the same second-upgrade copper parcel"
    );
    assert!(
        extraction.separation_required_energy < mechanization.separation_required_energy,
        "higher-grade feed must reduce the finite separation energy required for the same copper target"
    );
    assert!(
        extraction.separation_ticks <= mechanization.separation_ticks,
        "higher-grade feed must not take longer to separate into the same copper target"
    );

    let mechanization_autonomy_lead_ticks = extraction
        .machine_started_at
        .checked_sub(mechanization.machine_started_at)
        .unwrap_or_else(|| unreachable!("mechanization-first already wins autonomous-work access"));
    let extraction_hard_access_lead_ticks = mechanization_hard_at
        .checked_sub(extraction_hard_at)
        .unwrap_or_else(|| panic!("extraction-first must reach the hard seam before convergence"));
    let extraction_hard_material_window_ticks = extraction
        .second_upgrade_at
        .checked_sub(extraction_hard_at)
        .unwrap_or_else(|| unreachable!("extraction-first hard access precedes convergence"));
    let mechanization_processed_output_window_ticks = mechanization_pick_at
        .checked_sub(mechanization.first_processed_output_at)
        .unwrap_or_else(|| unreachable!("mechanization-first output precedes pick convergence"));
    let mechanization_processed_before_pick_upgrade = mechanization.machine_started_at
        < mechanization.first_processed_output_at
        && mechanization.first_processed_output_at < mechanization_pick_at;
    let material_efficiency_tradeoff = extraction.selected_processing_feed_copper_ppm
        > mechanization.selected_processing_feed_copper_ppm
        && extraction.separation_feed_mass < mechanization.separation_feed_mass
        && extraction.separation_required_energy < mechanization.separation_required_energy
        && extraction.separation_ticks <= mechanization.separation_ticks;
    let sequencing_tradeoff = extraction.first_upgrade_at == extraction_pick_at
        && extraction_hard_at < extraction.machine_started_at
        && extraction.hard_ore_before_convergence > Mass::ZERO
        && material_efficiency_tradeoff
        && mechanization.initial_crank_reinforced
        && mechanization.machine_started_at < mechanization_pick_at
        && mechanization.hard_ore_before_convergence == Mass::ZERO
        && mechanization_processed_before_pick_upgrade;
    let converged_both_upgrades = extraction.pick_upgraded_at.is_some()
        && mechanization.pick_upgraded_at.is_some()
        && extraction.crank_reinforced
        && mechanization.crank_reinforced
        && extraction.hard_seam_accessed_at.is_some()
        && mechanization.hard_seam_accessed_at.is_some()
        && extraction.hard_ore_mined == mechanization.hard_ore_mined;
    let tool_attention_reduction_ppm = attention_reduction_ppm(
        extraction.soft_ore_mining_ticks,
        extraction_reinforced_mining_ticks,
    );
    let stone_crank_power = nominal_manual_power(registries, EQUIPMENT_STONE_HAND_CRANK);
    let reinforced_crank_power =
        nominal_manual_power(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    let primitive_flywheel_input_power = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.max_input_power())
        .unwrap_or_else(|| panic!("primitive progression flywheel definition disappeared"));
    assert!(
        primitive_flywheel_input_power >= reinforced_crank_power,
        "primitive flywheel input envelope must not clip the scarce-copper crank upgrade's authored power"
    );
    let crank_power_gain_ppm = relative_power_gain_ppm(stone_crank_power, reinforced_crank_power);
    let stone_full_charge_ticks = extraction.initial_full_charge_ticks;
    let reinforced_full_charge_ticks = mechanization.initial_full_charge_ticks;
    assert!(
        reinforced_full_charge_ticks < stone_full_charge_ticks,
        "full flywheel recharge must make the reinforced crank's higher work rate observable"
    );
    let crank_attention_reduction_ppm =
        attention_reduction_ppm(stone_full_charge_ticks, reinforced_full_charge_ticks);
    let processed_output_has_playable_acquisition_use = extraction
        .processed_output_enabled_second_upgrade
        && mechanization.processed_output_enabled_second_upgrade;
    let mechanization_output_delta_ticks = tick_delta(
        extraction.first_processed_output_at,
        mechanization.first_processed_output_at,
    );
    let mechanization_convergence_delta_ticks = tick_delta(
        extraction.second_upgrade_at,
        mechanization.second_upgrade_at,
    );
    let returned_player_free_ticks = extraction
        .machine_player_free_ticks
        .min(mechanization.machine_player_free_ticks);
    let automation_preparation_ticks = extraction
        .automation_preparation_ticks
        .max(mechanization.automation_preparation_ticks);
    let separator_preparation_ticks = extraction
        .separator_preparation_ticks
        .max(mechanization.separator_preparation_ticks);
    let processing_line_preparation_ticks = extraction
        .processing_line_preparation_ticks
        .max(mechanization.processing_line_preparation_ticks);
    let attention_payback_cycles = match (
        extraction.attention_payback_cycles,
        mechanization.attention_payback_cycles,
    ) {
        (Some(extraction), Some(mechanization)) => Some(extraction.max(mechanization)),
        _ => None,
    };

    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "PROGRESSION SEQUENCING seed=0x{seed:016X} first-investment=[extraction:pick@{}t hard-access@{}t hard-before-convergence:{}mg; mechanization:crank@{}t machine@{}t output@{}t] convergence=[extraction:{}t mechanization:{}t lead:{:+}t mechanization-pick:{}t hard-access:{}t] resource-parity=[final-hard-ore:{}vs{}mg total-ore:{}vs{}mg direct-second-upgrade-blocked:{}]",
            extraction.first_upgrade_at,
            extraction_hard_at,
            extraction.hard_ore_before_convergence.milligrams(),
            mechanization.first_upgrade_at,
            mechanization.machine_started_at,
            mechanization.first_processed_output_at,
            extraction.second_upgrade_at,
            mechanization.second_upgrade_at,
            mechanization_convergence_delta_ticks,
            mechanization_pick_at,
            mechanization_hard_at,
            extraction.hard_ore_mined.milligrams(),
            mechanization.hard_ore_mined.milligrams(),
            extraction.total_ore_mined.milligrams(),
            mechanization.total_ore_mined.milligrams(),
            extraction.direct_second_upgrade_blocked,
        );
        std::println!(
            "PROGRESSION AGENCY seed=0x{seed:016X} matched-world choices=[extraction-first,mechanization-first] milestones=[machine-start:{}vs{}t first-output:{}vs{}t second-upgrade:{}vs{}t] attention=[mining:stone:{}t reinforced:{}t reduction:{}ppm episode-charge:{}vs{}t full-accumulator:stone:{}t reinforced:{}t reduction:{}ppm] autonomy=[machine-total:{}t reserve-cycle:{}t initial-overlap:{}vs{}t productive-overlap:{}vs{}t reserve-productive:{}vs{}t player-free:{}vs{}t] durability=[pick:{}vs{}ppm] survival=[energy:{}vs{}nJ hydration:{}vs{}uL] elapsed=[{}vs{}t]",
            extraction.machine_started_at,
            mechanization.machine_started_at,
            extraction.first_processed_output_at,
            mechanization.first_processed_output_at,
            extraction.second_upgrade_at,
            mechanization.second_upgrade_at,
            extraction.soft_ore_mining_ticks,
            extraction_reinforced_mining_ticks,
            tool_attention_reduction_ppm,
            extraction.charge_ticks,
            mechanization.charge_ticks,
            stone_full_charge_ticks,
            reinforced_full_charge_ticks,
            crank_attention_reduction_ppm,
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

    let review = PrimitiveProgressionReview {
        prospecting_ticks: extraction.prospecting_ticks,
        surface_prospecting_ticks: extraction.surface_prospecting_ticks,
        detailed_survey_ticks: extraction.detailed_survey_ticks,
        surface_clue_count: extraction.surface_clue_count,
        surface_resolved_clue_count: extraction.surface_resolved_clue_count,
        information_refinement_required: extraction.information_refinement_required,
        refinement_triggered_by_direct_shortage: extraction.refinement_triggered_by_direct_shortage,
        refined_coarse_lower_ppm: extraction.refined_coarse_lower_ppm,
        refined_coarse_upper_ppm: extraction.refined_coarse_upper_ppm,
        refined_detailed_lower_ppm: extraction.refined_detailed_lower_ppm,
        refined_detailed_upper_ppm: extraction.refined_detailed_upper_ppm,
        refined_sample_copper_ppm: extraction.refined_sample_copper_ppm,
        refined_sample_is_ore: extraction.refined_sample_is_ore,
        stone_mineable_clue_count: extraction.stone_mineable_clue_count,
        hardness_blocked_clue_count: extraction.hardness_blocked_clue_count,
        direct_copper_evidence_lower_ppm: extraction.direct_copper_evidence_lower_ppm,
        direct_copper_evidence_upper_ppm: extraction.direct_copper_evidence_upper_ppm,
        bulk_ore_evidence_lower_ppm: extraction.bulk_ore_evidence_lower_ppm,
        bulk_ore_evidence_upper_ppm: extraction.bulk_ore_evidence_upper_ppm,
        bulk_sample_copper_ppm: extraction.bulk_sample_copper_ppm,
        processing_feed_selected_from_bulk: extraction.processing_feed_selected_from_bulk,
        direct_second_upgrade_blocked: extraction.direct_second_upgrade_blocked,
        refined_clue_sample_mg: extraction.refined_clue_sample_mass.milligrams(),
        refined_clue_mining_ticks: extraction.refined_clue_mining_ticks,
        tool_attention_reduction_ppm,
        processed_output_has_playable_acquisition_use,
        extraction_feed_copper_ppm: extraction.selected_processing_feed_copper_ppm,
        mechanization_feed_copper_ppm: mechanization.selected_processing_feed_copper_ppm,
        extraction_separation_feed_mg: extraction.separation_feed_mass.milligrams(),
        mechanization_separation_feed_mg: mechanization.separation_feed_mass.milligrams(),
        recovered_copper_mg: extraction.recovered_copper_mass.milligrams(),
        extraction_separation_energy_nj: extraction.separation_required_energy.nanojoules(),
        mechanization_separation_energy_nj: mechanization.separation_required_energy.nanojoules(),
        extraction_separation_ticks: extraction.separation_ticks,
        mechanization_separation_ticks: mechanization.separation_ticks,
        material_efficiency_tradeoff,
        crank_power_gain_ppm,
        crank_attention_reduction_ppm,
        extraction_hard_access_lead_ticks,
        extraction_hard_material_window_ticks,
        mechanization_processed_output_window_ticks,
        mechanization_autonomy_lead_ticks,
        mechanization_output_delta_ticks,
        mechanization_convergence_delta_ticks,
        extraction_hard_ore_before_convergence_mg: extraction
            .hard_ore_before_convergence
            .milligrams(),
        sequencing_tradeoff,
        converged_both_upgrades,
        mechanization_processed_before_pick_upgrade,
        automation_preparation_ticks,
        separator_preparation_ticks,
        processing_line_preparation_ticks,
        attention_payback_cycles,
        steady_state_cycles: extraction.steady_state_cycles,
        steady_state_stop: extraction.steady_state_stop,
        final_crusher_condition_ppm: extraction.final_crusher_condition_ppm,
        machine_work_ticks: mechanization.machine_work_ticks,
        reserve_machine_work_ticks: mechanization.reserve_machine_work_ticks,
        mechanization_useful_overlap_ticks: mechanization.machine_useful_overlap_ticks,
        reserve_useful_overlap_ticks: mechanization.reserve_useful_overlap_ticks,
        returned_player_free_ticks,
        mechanization_player_free_delta_ticks: tick_delta(
            extraction.machine_player_free_ticks,
            mechanization.machine_player_free_ticks,
        ),
        mechanization_elapsed_delta_ticks: tick_delta(
            extraction.elapsed_ticks,
            mechanization.elapsed_ticks,
        ),
    };
    let choice_windows_are_consequential = review.extraction_hard_material_window_ticks > 0
        && review.mechanization_processed_output_window_ticks > 0;
    let post_payback_cycles = review
        .attention_payback_cycles
        .and_then(|payback| review.steady_state_cycles.checked_sub(payback))
        .unwrap_or(0);
    let automation_attention_payback_precedes_end_of_life = review
        .attention_payback_cycles
        .is_some_and(|cycles| cycles < review.steady_state_cycles)
        && review.returned_player_free_ticks >= review.automation_preparation_ticks
        && post_payback_cycles > 0;
    let fantasy_captured = review.surface_resolved_clue_count < review.surface_clue_count
        && review.surface_resolved_clue_count > 0
        && review.information_refinement_required
        && review.refinement_triggered_by_direct_shortage
        && review.refined_detailed_lower_ppm > review.refined_coarse_lower_ppm
        && review.refined_detailed_upper_ppm < review.refined_coarse_upper_ppm
        && review.refined_sample_is_ore
        && review.bulk_sample_copper_ppm > review.refined_sample_copper_ppm
        && review.processing_feed_selected_from_bulk
        && review.stone_mineable_clue_count > 0
        && review.hardness_blocked_clue_count > 0
        && review.direct_second_upgrade_blocked
        && review.detailed_survey_ticks > 0
        && review.refined_clue_sample_mg > 0
        && review.sequencing_tradeoff
        && review.material_efficiency_tradeoff
        && review.converged_both_upgrades
        && review.processed_output_has_playable_acquisition_use
        && review.tool_attention_reduction_ppm > 0
        && review.crank_power_gain_ppm > 0
        && review.crank_attention_reduction_ppm > 0
        && review.extraction_hard_access_lead_ticks > 0
        && review.mechanization_autonomy_lead_ticks > 0
        && choice_windows_are_consequential
        && review.extraction_hard_ore_before_convergence_mg > 0
        && review.mechanization_processed_before_pick_upgrade
        && review.mechanization_useful_overlap_ticks > 0
        && review.reserve_useful_overlap_ticks > 0
        && automation_attention_payback_precedes_end_of_life;
    assert!(
        fantasy_captured,
        "primitive progression must turn uncertainty into a paid information choice, make the scarce-copper decision produce reciprocal physical leverage, and repay automation setup attention within bounded repetition"
    );
    let payback = review
        .attention_payback_cycles
        .map(|cycles| format!("{cycles}cycles"))
        .unwrap_or_else(|| format!("unreached-within-{MAX_STEADY_STATE_CRUSH_CYCLES}-cycles"));
    let unresolved_surface_clues = review
        .surface_clue_count
        .checked_sub(review.surface_resolved_clue_count)
        .unwrap_or_else(|| unreachable!("resolved clue count cannot exceed observed clue count"));
    std::println!(
        "PROGRESSION REVIEW seed=0x{seed:016X} fantasy=observe->infer->prepare->extract->invest->delegate->reinvest captured:{fantasy_captured} knowledge=[surface:{}t clues:{} resolved:{} deferred:{} shortage-triggered-refinement:{} survey:{}t] tradeoff=[extraction-grade:{}ppm mechanization-grade:{}ppm first-output-delta:{:+}t reciprocal:{} converged:{}] autonomy=[break-even:{payback} post-payback:{}cycles returned-free:{}t stop:{}]",
        review.surface_prospecting_ticks,
        review.surface_clue_count,
        review.surface_resolved_clue_count,
        unresolved_surface_clues,
        review.refinement_triggered_by_direct_shortage,
        review.detailed_survey_ticks,
        review.extraction_feed_copper_ppm,
        review.mechanization_feed_copper_ppm,
        review.mechanization_output_delta_ticks,
        review.material_efficiency_tradeoff,
        review.converged_both_upgrades,
        post_payback_cycles,
        review.returned_player_free_ticks,
        review.steady_state_stop.label(),
    );
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "PROGRESSION TRADEOFF seed=0x{seed:016X} evidence=matched-counterfactual same-decision-state:true authorship=distinct-physical-consequences extraction-first=[unlock:hard-seam grade:{}ppm feed:{}mg separation-energy:{}nJ separation:{}t hard-window:{}t] mechanization-first=[feed-grade:{}ppm feed:{}mg separation-energy:{}nJ separation:{}t autonomy-lead:{}t first-output-delta:{:+}t crank:{}uW flywheel-input:{}uW unclipped:true full-charge-attention-reduction:{}ppm pre-pick-output-window:{}t] reciprocal-leverage:{} convergence=[both-upgrades:{} delta:{:+}t final-hard-ore:{}vs{}mg]",
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
            reinforced_crank_power.whole_microwatts(),
            primitive_flywheel_input_power.whole_microwatts(),
            review.crank_attention_reduction_ppm,
            review.mechanization_processed_output_window_ticks,
            review.material_efficiency_tradeoff,
            review.converged_both_upgrades,
            review.mechanization_convergence_delta_ticks,
            extraction.hard_ore_mined.milligrams(),
            mechanization.hard_ore_mined.milligrams(),
        );
        std::println!(
            "PROGRESSION AUTONOMY seed=0x{seed:016X} setup=[automation:{}t separator:{}t line:{}t] payback=[break-even:{payback} before-end-of-life:{automation_attention_payback_precedes_end_of_life} post-payback:{}cycles] delegated-work=[machine:{}t productive-overlap:{}t reserve-overlap:{}t returned-free:{}t] lifecycle=[cycles:{} stop:{} crusher-condition:{}ppm] branch-deltas=[free:{:+}t elapsed:{:+}t]",
            review.automation_preparation_ticks,
            review.separator_preparation_ticks,
            review.processing_line_preparation_ticks,
            post_payback_cycles,
            review.machine_work_ticks,
            review.mechanization_useful_overlap_ticks,
            review.reserve_useful_overlap_ticks,
            review.returned_player_free_ticks,
            review.steady_state_cycles,
            review.steady_state_stop.label(),
            review.final_crusher_condition_ppm,
            review.mechanization_player_free_delta_ticks,
            review.mechanization_elapsed_delta_ticks,
        );
    }
    review
}

pub(crate) fn run_primitive_progression_probe(registries: &Registries, seed: u64) {
    let _ = evaluate_primitive_progression_probe(registries, seed);
}
