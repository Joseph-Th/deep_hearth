//! Structural observation, controlled-delivery consequences, and crusher relocation policy.

use super::*;

pub(super) fn stage_rank(stage: StructuralStage) -> u8 {
    match stage {
        StructuralStage::Stable => 0,
        StructuralStage::Strained => 1,
        StructuralStage::Cracking => 2,
        StructuralStage::Failed => 3,
    }
}

pub(super) fn structural_assessment(
    analysis: &deep_hearth::structural::StructuralAnalysis,
    element: StructuralElementId,
) -> StructuralAssessment {
    analysis
        .assessments()
        .iter()
        .find(|assessment| assessment.element() == element)
        .copied()
        .unwrap_or_else(|| panic!("gameplay harness structural assessment missing"))
}

pub(super) fn structural_label(assessment: StructuralAssessment) -> String {
    if assessment.stage() == StructuralStage::Failed {
        "Failed".to_owned()
    } else {
        format!(
            "{:?}/{}ppm",
            assessment.stage(),
            assessment.utilization_ppm()
        )
    }
}

fn analyze_workshop_supports(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
) -> (StructuralAssessment, StructuralAssessment) {
    let analysis = analyze_structure(
        registries.structural(),
        registries.materials(),
        state.structures(),
    )
    .unwrap_or_else(|error| panic!("workshop structural analysis failed: {error}"));
    (
        structural_assessment(&analysis, ids.compact_support),
        structural_assessment(&analysis, ids.reinforced_support),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrusherRelocationOutcome {
    Relocated,
    Blocked,
}

fn try_relocate_crusher(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    current_support: &mut StructuralElementId,
    alternate_support: &mut StructuralElementId,
    structure: &mut ScenarioStructureReport,
) -> CrusherRelocationOutcome {
    let relocation = match validate_relocate_equipment(
        registries,
        state,
        ids.crusher,
        *alternate_support,
    ) {
        Ok(relocation) => relocation,
        Err(EquipmentSupportError::TargetNotActive {
            element: _element,
            lifecycle,
        }) => {
            println!(
                "  recovery blocked: alternate bay is {lifecycle:?} after the stored-matter delivery"
            );
            return CrusherRelocationOutcome::Blocked;
        }
        Err(error) => panic!("crusher recovery relocation validation failed: {error}"),
    };
    let preview_analysis = relocation
        .structural_analysis()
        .unwrap_or_else(|| panic!("crusher relocation produced no structural load change"));
    let preview_assessment = structural_assessment(preview_analysis, *alternate_support);
    if preview_assessment.stage() == StructuralStage::Failed {
        println!(
            "  recovery blocked: mounting the crusher on the alternate bay would fail it at {}ppm utilization",
            preview_assessment.utilization_ppm()
        );
        return CrusherRelocationOutcome::Blocked;
    }

    let abandoned_support = *current_support;
    let assessment = structural_assessment(preview_analysis, *alternate_support);
    debug_assert_ne!(assessment.stage(), StructuralStage::Failed);
    let _ = relocation
        .commit(state)
        .unwrap_or_else(|error| panic!("crusher recovery relocation commit failed: {error}"));
    println!(
        "  recovery: relocated crusher to alternate support -> {:?}/{}ppm utilization",
        assessment.stage(),
        assessment.utilization_ppm()
    );
    let abandoned = state
        .structures()
        .get_element(abandoned_support)
        .unwrap_or_else(|| panic!("abandoned workshop support disappeared during recovery"));
    if abandoned.lifecycle() == StructuralLifecycle::Failed || abandoned.is_cracked() {
        structure.structural_damage_debt = true;
        println!(
            "  recovery debt: previous bay remains {:?} cracked={} after relocation; restoring production did not repair the structure",
            abandoned.lifecycle(),
            abandoned.is_cracked(),
        );
    } else {
        println!(
            "  recovery note: previous bay retains {}mN of inventory-owned stored-matter load after relocation",
            abandoned
                .load(StructuralLoadKind::StoredMatter)
                .millinewtons(),
        );
    }
    std::mem::swap(current_support, alternate_support);
    structure.support_relocation = true;
    CrusherRelocationOutcome::Relocated
}

pub(super) fn adapt_after_delivery(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    actor: &mut ScenarioActorRuntime<'_>,
    after: StructuralAssessment,
) {
    if matches!(
        state.player_work().active(),
        Some(PlayerWork::EquipmentMaintenance { .. })
    ) {
        println!(
            "  recovery deferred: structural change occurred during crusher maintenance; relocation waits for service ownership to release"
        );
        return;
    }
    if after.stage() == StructuralStage::Failed {
        let suspended_wip = state.production().jobs().find(|job| {
            job.is_suspended()
                && job
                    .equipment_provider()
                    .is_some_and(|provider| provider.equipment() == ids.crusher)
        });
        if let Some(job) = suspended_wip {
            let suspension = job
                .suspension()
                .unwrap_or_else(|| panic!("suspended crusher job lost suspension state"));
            println!(
                "  consequence: failed support suspends job {} with {} active tick(s) remaining; its selected ore is still conserved as work-in-process",
                job.id().value(),
                suspension.remaining_active_time().value()
            );
        }
        let untouched_mass = state
            .inventory()
            .get_lot(ids.ore_lot)
            .map(|lot| lot.mass())
            .unwrap_or(Mass::ZERO);
        if !untouched_mass.is_zero() {
            let probe_mass = Mass::from_milligrams(
                untouched_mass
                    .milligrams()
                    .min(actor.nominal_batch_mass.milligrams()),
            );
            let selection = [MaterialLotSelection::new(ids.ore_lot, probe_mass)];
            let blocked = resolve_comminution_process(
                registries,
                state,
                ComminutionRequest::new(
                    PROCESS_CRUSH_ORE,
                    ids.ore_source,
                    &selection,
                    ids.crusher,
                    ids.small_drive,
                ),
            );
            actor.report.structure.support_failure_blocked_production = matches!(
                blocked,
                Err(ComminutionResolutionError::Equipment(
                    EquipmentProviderError::StructuralSupportNotActive {
                        equipment: _equipment,
                        element: _element,
                        lifecycle: _lifecycle,
                    }
                ))
            );
            println!(
                "  consequence: failed support blocks the next untouched ore operation={}",
                actor.report.structure.support_failure_blocked_production
            );
        } else {
            if suspended_wip.is_none() {
                println!(
                    "  consequence: support failed after the work order was already complete; recovery still leaves structural damage debt"
                );
            } else {
                println!(
                    "  queue state: no untouched ore remains behind the suspended work-in-process"
                );
            }
        }
        if try_relocate_crusher(
            registries,
            state,
            ids,
            actor.current_support,
            actor.alternate_support,
            actor.report.structure,
        ) == CrusherRelocationOutcome::Blocked
        {
            actor.report.structure.structural_stop = true;
            println!(
                "  structural frontier: no surviving bay can carry the crusher, so new production remains blocked"
            );
        }
        return;
    }

    if after.stage() == StructuralStage::Cracking || after.stage() == StructuralStage::Strained {
        if actor.policy.structural_preference == StructuralPreference::MoveOnlyForFailure {
            println!(
                "  decision: remain on current support at {}; player policy moves equipment only when support actually fails",
                structural_label(after)
            );
            return;
        }
        let alternate = match validate_relocate_equipment(
            registries,
            state,
            ids.crusher,
            *actor.alternate_support,
        ) {
            Ok(alternate) => alternate,
            Err(EquipmentSupportError::TargetNotActive {
                element: _element,
                lifecycle,
            }) => {
                println!(
                    "  decision: remain on current support; alternate bay is {lifecycle:?} after the stored-matter delivery"
                );
                return;
            }
            Err(error) => panic!("crusher relocation prediction failed: {error}"),
        };
        let alternate_assessment = structural_assessment(
            alternate
                .structural_analysis()
                .unwrap_or_else(|| panic!("crusher relocation produced no structural load change")),
            *actor.alternate_support,
        );
        if (
            stage_rank(alternate_assessment.stage()),
            alternate_assessment.utilization_ppm(),
        ) < (stage_rank(after.stage()), after.utilization_ppm())
        {
            println!(
                "  decision: alternate bay with the crusher mounted would be {}; relocate before failure",
                structural_label(alternate_assessment)
            );
            let relocated = try_relocate_crusher(
                registries,
                state,
                ids,
                actor.current_support,
                actor.alternate_support,
                actor.report.structure,
            );
            debug_assert_eq!(relocated, CrusherRelocationOutcome::Relocated);
        } else {
            println!(
                "  decision: remain on current support at {}; alternate bay with the crusher mounted would be {}",
                structural_label(after),
                structural_label(alternate_assessment)
            );
        }
    }
}

pub(super) fn adapt_current_support_if_needed(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    actor: &mut ScenarioActorRuntime<'_>,
) {
    let (compact, reinforced) = analyze_workshop_supports(registries, state, ids);
    let current = if *actor.current_support == ids.compact_support {
        compact
    } else {
        reinforced
    };
    if current.stage() != StructuralStage::Stable {
        adapt_after_delivery(registries, state, ids, actor, current);
    }
}

pub(super) fn apply_delivery(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    controller: &mut ControlledDeliveryRuntime<'_>,
    actor: &mut ScenarioActorRuntime<'_>,
) -> StructuralAssessment {
    assert_eq!(
        state.tick().value(),
        controller.delivery.delivery_at_tick,
        "controlled gameplay event must occur at its planned world tick"
    );
    actor.report.progress.delivery_applied = true;
    actor.report.progress.operations_before_delivery = actor.report.progress.operations_completed;
    let authorization = controller
        .authorization
        .take()
        .unwrap_or_else(|| panic!("controlled delivery authorization was already consumed"));
    commit_controlled_material_delivery(registries, state, authorization);
    let (compact, reinforced) = analyze_workshop_supports(registries, state, ids);
    let (after, alternate_after) = if *actor.current_support == ids.compact_support {
        (compact, reinforced)
    } else {
        (reinforced, compact)
    };
    actor.report.structure.structural_consequence =
        compact.stage() != StructuralStage::Stable || reinforced.stage() != StructuralStage::Stable;
    actor.report.structure.structural_damage_debt |= [ids.compact_support, ids.reinforced_support]
        .into_iter()
        .any(|support| {
            state
                .structures()
                .get_element(support)
                .is_some_and(|record| record.is_cracked())
        });
    let destination = if controller.delivery.destination_is_compact {
        "compact"
    } else {
        "reinforced"
    };
    println!(
        "  delivery: move={}mg wood into {destination} supported storage at tick={} after {} operation(s) / {}mg processed -> active={} alternate={}",
        controller.delivery.mass.milligrams(),
        state.tick().value(),
        actor.report.progress.operations_completed,
        actor.report.progress.processed_mass.milligrams(),
        structural_label(after),
        structural_label(alternate_after),
    );
    after
}

pub(super) fn apply_delivery_and_adapt(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    controller: &mut ControlledDeliveryRuntime<'_>,
    actor: &mut ScenarioActorRuntime<'_>,
) {
    let after = apply_delivery(registries, state, ids, controller, actor);
    adapt_after_delivery(registries, state, ids, actor, after);
}
