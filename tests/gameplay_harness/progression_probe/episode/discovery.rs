//! Evidence-driven geological discovery and early extraction for primitive progression.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct ProgressionDiscoveryPlan {
    pub(super) raw: deep_hearth::inventory::StockpileId,
    pub(super) shaped: deep_hearth::inventory::StockpileId,
    pub(super) native_storage: deep_hearth::inventory::StockpileId,
    pub(super) ore_storage: deep_hearth::inventory::StockpileId,
    pub(super) refined_clue_storage: deep_hearth::inventory::StockpileId,
    pub(super) visible_clue_requests: [MiningTargetRequest; 4],
    pub(super) soft_ore_target: MiningTargetRequest,
    pub(super) hard_ore_target: MiningTargetRequest,
    pub(super) native_target: MiningTargetRequest,
    pub(super) trace_target: MiningTargetRequest,
    pub(super) stone_hardness_limit: Pressure,
    pub(super) stone_pick_batch_limit: Mass,
    pub(super) pick_upgrade_native: Mass,
    pub(super) crank_upgrade_native: Mass,
    pub(super) refined_clue_sample_mass: Mass,
    pub(super) mined_mass: Mass,
    pub(super) deferred_trace_refinement: bool,
}

#[derive(Clone, Copy)]
pub(super) struct ProgressionDiscovery {
    pub(super) pick: deep_hearth::equipment::EquipmentId,
    pub(super) prospecting_ticks: u64,
    pub(super) regional_recon_ticks: u64,
    pub(super) regional_upper_bounds_ppm: [u32; PROGRESSION_REGIONAL_ZONE_COUNT],
    pub(super) surface_prospecting_ticks: u64,
    pub(super) hardness_sampling_ticks: u64,
    pub(super) clue_count: usize,
    pub(super) surface_resolved_clues: u8,
    pub(super) mineable_clue_count: usize,
    pub(super) hardness_blocked_clue_count: usize,
    pub(super) hard_clue: ObservedCopperClue,
    pub(super) direct_copper_clue: ObservedCopperClue,
    pub(super) bulk_ore_clue: ObservedCopperClue,
    pub(super) stone_mining_ticks: u64,
    pub(super) direct_copper_mining_ticks: u64,
    pub(super) direct_second_upgrade_blocked: bool,
    pub(super) bulk_sample: ObservedMaterialSample,
    pub(super) detailed_survey_ticks: u64,
    pub(super) refined_coarse_lower_ppm: u32,
    pub(super) refined_coarse_upper_ppm: u32,
    pub(super) refined_detailed_lower_ppm: u32,
    pub(super) refined_detailed_upper_ppm: u32,
    pub(super) refined_sample_copper_ppm: u32,
    pub(super) refined_sample_is_ore: bool,
    pub(super) actual_refined_sample_mass: Mass,
    pub(super) refined_clue_mining_ticks: u64,
    pub(super) refinement_triggered_by_direct_shortage: bool,
    pub(super) processing_feed_selected_from_bulk: bool,
    pub(super) information_refinement_required: bool,
}

pub(super) fn discover_primitive_progression(
    registries: &Registries,
    state: &mut AppState,
    plan: ProgressionDiscoveryPlan,
) -> ProgressionDiscovery {
    let ProgressionDiscoveryPlan {
        raw,
        shaped,
        native_storage,
        ore_storage,
        refined_clue_storage,
        visible_clue_requests,
        soft_ore_target,
        hard_ore_target,
        native_target,
        trace_target,
        stone_hardness_limit,
        stone_pick_batch_limit,
        pick_upgrade_native,
        crank_upgrade_native,
        refined_clue_sample_mass,
        mined_mass,
        deferred_trace_refinement,
    } = plan;

    for request in visible_clue_requests {
        assert_eq!(
            resolve_mining_target(state, request),
            Err(MiningTargetResolutionError::NoEvidence {
                material: MATERIAL_COPPER,
                region: request.region(),
            }),
            "hidden geological truth must not authorize mining before the player performs prospecting"
        );
    }

    let regional_zones = std::array::from_fn(progression_regional_bounds);
    let regional_recon_ticks = regional_zones
        .iter()
        .copied()
        .try_fold(0_u64, |total, region| {
            total.checked_add(acquire_copper_evidence(
                registries,
                state,
                PROSPECTING_REGIONAL_RECONNAISSANCE,
                region,
            ))
        })
        .unwrap_or_else(|| panic!("primitive progression regional-recon duration overflowed"));
    let regional_upper_bounds_ppm = regional_zones.map(|region| {
        let (lower_ppm, upper_ppm) = observed_copper_bounds(
            state,
            MiningTargetRequest::new(region, MATERIAL_COPPER),
        );
        assert_eq!(
            lower_ppm, 0,
            "regional reconnaissance must remain broad evidence rather than directly authorizing extraction"
        );
        upper_ppm
    });
    let mut clue_requests = visible_clue_requests.to_vec();
    clue_requests.sort_by_key(|request| {
        let zone = regional_zone_for_clue(request.region(), &regional_zones);
        (
            Reverse(regional_upper_bounds_ppm[zone]),
            request.region().min(),
            request.region().max_exclusive(),
        )
    });

    let surface_prospecting_ticks = clue_requests
        .iter()
        .copied()
        .try_fold(0_u64, |total, request| {
            total.checked_add(acquire_copper_evidence(
                registries,
                state,
                PROSPECTING_FIELD_INSPECTION,
                request.region(),
            ))
        })
        .unwrap_or_else(|| panic!("primitive progression local-inspection duration overflowed"));
    let mut surface_resolved_clues = 0_u8;
    let mut surface_clues = Vec::new();
    let mut refinement = None;
    for request in clue_requests.iter().copied() {
        let (lower_ppm, upper_ppm) = observed_copper_bounds(state, request);
        match resolve_mining_target(state, request) {
            Ok(target) => {
                assert_eq!(target.region(), request.region());
                assert_eq!(target.material(), MATERIAL_COPPER);
                surface_clues.push(ObservedCopperClue {
                    request,
                    lower_ppm,
                    upper_ppm,
                });
                surface_resolved_clues = surface_resolved_clues
                    .checked_add(1)
                    .unwrap_or_else(|| panic!("surface clue count overflowed"));
            }
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }) if region == request.region() && lower_ppm == 0 => {
                assert!(
                    refinement.is_none(),
                    "primitive progression should expose one information-escalation decision at a time"
                );
                refinement = Some((request, lower_ppm, upper_ppm));
            }
            Err(error) => panic!("unexpected surface prospecting outcome: {error}"),
        }
    }
    let trace_surface_bounds = observed_copper_bounds(state, trace_target);
    let visible_clue_count = u8::try_from(clue_requests.len())
        .unwrap_or_else(|_| panic!("primitive progression visible clue count exceeds u8"));
    let unresolved_clue_count = visible_clue_count
        .checked_sub(surface_resolved_clues)
        .unwrap_or_else(|| panic!("resolved surface clues exceeded visible clues"));
    // Resolve trace/surface clues from observation; boundary traces may need revisit-after-shortage.
    if deferred_trace_refinement {
        assert_eq!(
            unresolved_clue_count, 1,
            "maintained information path should leave one low-grade clue unresolved after cheap inspection"
        );
        assert_eq!(
            refinement.map(|(request, _, _)| request),
            Some(trace_target),
            "maintained information path lost its deferred trace-copper clue"
        );
    } else if let Some((request, _, _)) = refinement {
        assert_eq!(
            unresolved_clue_count, 1,
            "boundary-trace organic information path should leave only the poor trace clue unresolved after cheap inspection"
        );
        assert_eq!(
            request, trace_target,
            "boundary-trace organic refinement must target the poor trace-copper clue"
        );
    } else {
        assert_eq!(
            unresolved_clue_count, 0,
            "surface-resolved organic information path should make every visible clue actionable after cheap inspection"
        );
    }
    let information_refinement_required = refinement.is_some();
    let projected_surface_prospecting_ticks = clue_requests
        .iter()
        .copied()
        .try_fold(0_u64, |total, request| {
            let projected = project_prospecting_work(
                registries,
                PROSPECTING_FIELD_INSPECTION,
                request.region(),
            )
            .unwrap_or_else(|error| {
                panic!("primitive progression field-inspection projection failed: {error}")
            })
            .duration()
            .value();
            total.checked_add(projected)
        })
        .unwrap_or_else(|| {
            panic!("primitive progression projected field-inspection duration overflowed")
        });
    assert_eq!(
        surface_prospecting_ticks, projected_surface_prospecting_ticks,
        "primitive progression must pay canonical projected surface-inspection time for every visible clue region"
    );

    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_PICK),
    );
    let pick = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("primitive progression pick assembly commit failed: {error}")
        });
    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_GEOLOGICAL_HAMMER),
    );
    let sampling_hammer =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, shaped)
            .unwrap_or_else(|error| {
                panic!("primitive progression sampling-hammer assembly failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("primitive progression sampling-hammer assembly commit failed: {error}")
            });
    let mut mineable_clues = Vec::new();
    let mut hardness_blocked_clues = Vec::new();
    let mut hardness_sampling_ticks = 0_u64;
    for clue in surface_clues {
        let sample_ticks = acquire_copper_evidence_with_equipment(
            registries,
            state,
            PROSPECTING_DETAILED_FIELD_SURVEY,
            clue.request.region(),
            Some(sampling_hammer),
        );
        hardness_sampling_ticks = hardness_sampling_ticks
            .checked_add(sample_ticks)
            .unwrap_or_else(|| {
                panic!("primitive progression hardness-sampling duration overflowed")
            });
        let sampled_clue = observed_resolved_copper_clue(state, clue.request);
        let hardness = resolve_progression_mining_target(state, clue.request)
            .excavation_hardness()
            .unwrap_or_else(|| {
                panic!(
                    "physical sampling produced no excavation-hardness evidence for {:?}",
                    clue.request.region()
                )
            });
        if hardness.upper() <= stone_hardness_limit {
            mineable_clues.push(sampled_clue);
        } else {
            hardness_blocked_clues.push(sampled_clue);
        }
    }
    assert!(
        hardness_sampling_ticks > 0,
        "primitive progression must pay physical-sampling work before classifying extraction hardness"
    );
    assert!(
        mineable_clues
            .iter()
            .any(|clue| clue.request == native_target),
        "direct-copper target must remain mineable with the stone pick"
    );
    assert!(
        mineable_clues
            .iter()
            .any(|clue| clue.request == soft_ore_target),
        "bulk soft-ore target must remain mineable with the stone pick"
    );
    assert_eq!(
        mineable_clues
            .iter()
            .any(|clue| clue.request == trace_target),
        !information_refinement_required,
        "trace target visibility must follow the authored information-refinement branch"
    );
    let hard_clue = hardness_blocked_clues
        .iter()
        .copied()
        .find(|clue| clue.request == hard_ore_target)
        .unwrap_or_else(|| {
            panic!("hard-ore target no longer exposes the intended stone-pick hardness gate")
        });
    let direct_copper_clue = strongest_observed_copper_clue(mineable_clues.iter().copied());
    let bulk_ore_clue = strongest_observed_copper_clue(
        mineable_clues
            .iter()
            .copied()
            .filter(|clue| clue.request != direct_copper_clue.request),
    );
    assert_eq!(
        direct_copper_clue.request, native_target,
        "strongest player-visible copper evidence no longer points at the direct-copper occurrence"
    );
    assert_eq!(
        bulk_ore_clue.request, soft_ore_target,
        "best remaining mineable copper evidence no longer points at the bulk processing feed"
    );
    assert_eq!(
        hard_clue.request, hard_ore_target,
        "acquired hardness evidence no longer identifies the intended stone-pick blocker"
    );
    if !information_refinement_required {
        let trace_clue = observed_resolved_copper_clue(state, trace_target);
        assert!(
            trace_clue.upper_ppm < bulk_ore_clue.lower_ppm,
            "surface-resolved low-grade clue must be safely dominated by the player's bulk-ore evidence before the actor skips further investigation"
        );
    }

    let stone_mining_ticks = mine_and_claim(
        registries,
        state,
        bulk_ore_clue.request,
        ore_storage,
        pick,
        mined_mass,
    );
    let blocked_hard_target = resolve_progression_mining_target(state, hard_clue.request);
    let blocked_hardness_upper = blocked_hard_target
        .excavation_hardness()
        .unwrap_or_else(|| panic!("known hard seam lost acquired hardness evidence"))
        .upper();
    assert_eq!(
        validate_start_mining(
            registries,
            state,
            MINING_METHOD_HAND_PICK,
            blocked_hard_target,
            ore_storage,
            pick,
            mined_mass,
        )
        .err(),
        Some(
            MiningStartError::ExcavationHardnessEvidenceExceedsCapability {
                observed_upper: blocked_hardness_upper,
                maximum: stone_hardness_limit,
            }
        ),
        "the known hard seam must be a real blocked affordance before pick reinforcement"
    );
    let initial_direct_copper_mining_ticks = mine_total_and_claim(
        registries,
        state,
        direct_copper_clue.request,
        native_storage,
        pick,
        pick_upgrade_native,
        stone_pick_batch_limit,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(native_storage)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
            }),
        Some(pick_upgrade_native),
        "the actor's strongest copper clue must reveal directly usable native metal only after extraction"
    );
    let second_direct_attempt = try_mine_and_claim(
        registries,
        state,
        direct_copper_clue.request,
        native_storage,
        pick,
        crank_upgrade_native,
    )
    .unwrap_or_else(|error| panic!("direct-copper exhaustion attempt failed: {error}"));
    let direct_second_upgrade_blocked = second_direct_attempt.output < crank_upgrade_native;
    assert!(
        direct_second_upgrade_blocked,
        "the player must learn only after committed mining work that the promising direct-copper occurrence cannot fund both upgrades"
    );
    let direct_copper_mining_ticks = initial_direct_copper_mining_ticks
        .checked_add(second_direct_attempt.ticks)
        .unwrap_or_else(|| panic!("direct-copper mining duration overflowed"));
    let direct_native_total = pick_upgrade_native
        .checked_add(second_direct_attempt.output)
        .unwrap_or_else(|| panic!("direct-copper recovered mass overflowed"));
    assert!(
        direct_native_total
            < pick_upgrade_native
                .checked_add(crank_upgrade_native)
                .unwrap_or_else(|| panic!("two-upgrade copper requirement overflowed")),
        "committed direct-copper exhaustion must still leave the two upgrades underfunded"
    );
    let direct_supply_blocked_at = state.tick().value();
    let bulk_sample = observe_material_sample(state, ore_storage, "bulk ore");
    assert_eq!(
        bulk_sample.commodity.form(),
        FORM_ORE,
        "the maintained bulk occurrence must reveal processable ore after extraction"
    );

    let (
        detailed_survey_ticks,
        refined_coarse_lower_ppm,
        refined_coarse_upper_ppm,
        refined_detailed_lower_ppm,
        refined_detailed_upper_ppm,
        refined_sample_copper_ppm,
        refined_sample_is_ore,
        actual_refined_sample_mass,
        refined_clue_mining_ticks,
        refinement_triggered_by_direct_shortage,
        processing_feed_selected_from_bulk,
    ) = if let Some((refinement_request, coarse_lower_ppm, coarse_upper_ppm)) = refinement {
        assert!(
            matches!(
                resolve_mining_target(state, refinement_request),
                Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. })
            ),
            "the actor must defer unresolved geological work until the direct-copper shortage makes another occurrence relevant"
        );
        let refinement_started_at = state.tick().value();
        assert_eq!(
            refinement_started_at, direct_supply_blocked_at,
            "the actor should revisit unresolved evidence as the immediate response to exhausting the clear direct-copper option"
        );
        let detailed_survey_ticks = acquire_copper_evidence_with_equipment(
            registries,
            state,
            PROSPECTING_DETAILED_FIELD_SURVEY,
            refinement_request.region(),
            Some(sampling_hammer),
        );
        let refined_clue = observed_resolved_copper_clue(state, refinement_request);
        assert!(
            refined_clue.lower_ppm > coarse_lower_ppm && refined_clue.upper_ppm < coarse_upper_ppm,
            "detailed survey must materially narrow the deferred ambiguous clue"
        );
        assert_eq!(
            detailed_survey_ticks,
            project_prospecting_work(
                registries,
                PROSPECTING_DETAILED_FIELD_SURVEY,
                refinement_request.region(),
            )
            .unwrap_or_else(|error| {
                panic!("primitive progression detailed-survey projection failed: {error}")
            })
            .duration()
            .value(),
            "deferred ambiguity recovery must pay the canonical projected refinement cost"
        );
        let refined_clue_mining_ticks = mine_and_claim(
            registries,
            state,
            refined_clue.request,
            refined_clue_storage,
            pick,
            refined_clue_sample_mass,
        );
        let refined_sample = observe_material_sample(state, refined_clue_storage, "refined clue");
        assert_eq!(
            refined_sample.commodity.form(),
            FORM_ORE,
            "the refined alternative must reveal its physical form only after extraction"
        );
        assert!(
            bulk_sample.copper_ppm > refined_sample.copper_ppm,
            "the actor should choose the richer already-mined bulk ore after the deferred sample rules out another direct native source"
        );
        (
            detailed_survey_ticks,
            coarse_lower_ppm,
            coarse_upper_ppm,
            refined_clue.lower_ppm,
            refined_clue.upper_ppm,
            refined_sample.copper_ppm,
            true,
            refined_clue_sample_mass,
            refined_clue_mining_ticks,
            true,
            true,
        )
    } else {
        let trace_clue = observed_resolved_copper_clue(state, trace_target);
        assert!(
            trace_clue.upper_ppm < bulk_ore_clue.lower_ppm,
            "already-resolved alternative must be conservatively worse than the selected bulk feed"
        );
        (
            0,
            trace_surface_bounds.0,
            trace_surface_bounds.1,
            trace_surface_bounds.0,
            trace_surface_bounds.1,
            0,
            false,
            Mass::ZERO,
            0,
            false,
            true,
        )
    };
    let prospecting_ticks = regional_recon_ticks
        .checked_add(surface_prospecting_ticks)
        .and_then(|ticks| ticks.checked_add(hardness_sampling_ticks))
        .and_then(|ticks| ticks.checked_add(detailed_survey_ticks))
        .unwrap_or_else(|| panic!("primitive progression prospecting duration overflowed"));

    ProgressionDiscovery {
        pick,
        prospecting_ticks,
        regional_recon_ticks,
        regional_upper_bounds_ppm,
        surface_prospecting_ticks,
        hardness_sampling_ticks,
        clue_count: clue_requests.len(),
        surface_resolved_clues,
        mineable_clue_count: mineable_clues.len(),
        hardness_blocked_clue_count: hardness_blocked_clues.len(),
        hard_clue,
        direct_copper_clue,
        bulk_ore_clue,
        stone_mining_ticks,
        direct_copper_mining_ticks,
        direct_second_upgrade_blocked,
        bulk_sample,
        detailed_survey_ticks,
        refined_coarse_lower_ppm,
        refined_coarse_upper_ppm,
        refined_detailed_lower_ppm,
        refined_detailed_upper_ppm,
        refined_sample_copper_ppm,
        refined_sample_is_ore,
        actual_refined_sample_mass,
        refined_clue_mining_ticks,
        refinement_triggered_by_direct_shortage,
        processing_feed_selected_from_bulk,
        information_refinement_required,
    }
}
