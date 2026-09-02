//! Completion audit and reporting for the focused ore-preparation capability probe.

use super::*;

fn represented_copper_ppm_mg(state: &AppState, stockpiles: &[StockpileId]) -> u128 {
    stockpiles
        .iter()
        .flat_map(|stockpile| state.inventory().lot_ids(*stockpile))
        .map(|lot| {
            let record = state
                .inventory()
                .get_lot(lot)
                .unwrap_or_else(|| panic!("ore preparation accounting lot disappeared"));
            u128::from(record.mass().milligrams())
                * u128::from(record.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum()
}

pub(super) struct OreCompletionEvidence {
    pub(super) crush: PoweredStageResult,
    pub(super) grind: PoweredStageResult,
    pub(super) screen: ScreeningStageResult,
    pub(super) regrind: RegrindStageResult,
    pub(super) concentration: PoweredStageResult,
    pub(super) crusher_output_matches_authoring: bool,
    pub(super) grinding_matches_authoring: bool,
    pub(super) grinding_resolved_screen_cut: bool,
}

pub(super) fn finalize_completed_ore_probe(
    registries: &Registries,
    episode: &OreProbeEpisode,
    evidence: OreCompletionEvidence,
) -> OreProbeOutcome {
    let state = &episode.state;
    let ids = episode.ids;
    let screen_definition = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical screen definition disappeared"));
    let concentration_definition = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .unwrap_or_else(|| panic!("canonical copper concentration definition disappeared"));
    let final_matter = calculate_matter_accounting(state)
        .unwrap_or_else(|error| panic!("ore preparation final matter accounting failed: {error}"))
        .total();
    let final_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared after completion"));
    let final_crusher_condition = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation crusher disappeared after completion"));
    let final_grinder_condition = state
        .equipment()
        .get_equipment(ids.grinder)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation grinder disappeared after completion"));
    let final_screen_condition = state
        .equipment()
        .get_equipment(ids.screen)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation screen disappeared after completion"));
    let final_separator_condition = state
        .equipment()
        .get_equipment(ids.separator)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation separator disappeared after completion"));
    let undersize_mass = state
        .inventory()
        .get_stockpile(ids.undersize_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation undersize storage disappeared"));
    let oversize_mass = state
        .inventory()
        .get_stockpile(ids.oversize_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation oversize storage disappeared"));
    let concentrate_mass = state
        .inventory()
        .get_stockpile(ids.concentrate_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation concentrate storage disappeared"));
    let tailings_mass = state
        .inventory()
        .get_stockpile(ids.tailings_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation tailings storage disappeared"));
    let source_remaining = state
        .inventory()
        .get_stockpile(ids.ore_source)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation source storage disappeared"));
    assert_eq!(
        source_remaining,
        Mass::ZERO,
        "completed ore preparation must consume the complete attempted batch"
    );
    let concentrate_identity_is_valid =
        state
            .inventory()
            .lot_ids(ids.concentrate_storage)
            .all(|lot| {
                state.inventory().get_lot(lot).is_some_and(|lot| {
                    lot.commodity().material() == MATERIAL_COPPER
                        && lot.commodity().form() == FORM_CONCENTRATE
                        && lot.composition().parts_per_million(MATERIAL_COPPER) != 0
                })
            });
    let concentrate_contains_gangue =
        state
            .inventory()
            .lot_ids(ids.concentrate_storage)
            .any(|lot| {
                state.inventory().get_lot(lot).is_some_and(|lot| {
                    lot.composition().parts_per_million(MATERIAL_COPPER) < 1_000_000
                })
            });
    let concentrate_distribution_is_fine =
        state
            .inventory()
            .lot_ids(ids.concentrate_storage)
            .all(|lot| {
                state
                    .inventory()
                    .get_lot(lot)
                    .and_then(|lot| lot.particle_size_distribution())
                    .is_some_and(|distribution| {
                        distribution.classes().iter().all(|class| {
                            class.range().maximum_diameter() <= screen_definition.aperture()
                        })
                    })
            });
    let tailings_retain_unrecovered_copper =
        state.inventory().lot_ids(ids.tailings_storage).any(|lot| {
            state
                .inventory()
                .get_lot(lot)
                .is_some_and(|lot| lot.composition().parts_per_million(MATERIAL_COPPER) != 0)
        });
    let tailings_distribution_is_fine =
        state.inventory().lot_ids(ids.tailings_storage).all(|lot| {
            state.inventory().get_lot(lot).is_some_and(|lot| {
                lot.commodity().form() == FORM_TAILINGS
                    && lot
                        .particle_size_distribution()
                        .is_some_and(|distribution| {
                            distribution.classes().iter().all(|class| {
                                class.range().maximum_diameter() <= screen_definition.aperture()
                            })
                        })
            })
        });
    let represented_copper =
        represented_copper_ppm_mg(state, &[ids.concentrate_storage, ids.tailings_storage]);
    let concentrate_copper = represented_copper_ppm_mg(state, &[ids.concentrate_storage]);
    let expected_copper =
        u128::from(episode.batch_mass.milligrams()) * u128::from(episode.input_copper_ppm);
    let expected_recovered_copper_milligrams = u64::try_from(
        expected_copper * u128::from(concentration_definition.target_recovery_ppm())
            / 1_000_000_000_000_u128,
    )
    .unwrap_or_else(|_| panic!("ore preparation recovered copper mass exceeded u64"));
    let concentrate_grade_ppm =
        u32::try_from(concentrate_copper / u128::from(concentrate_mass.milligrams()))
            .unwrap_or_else(|_| {
                panic!("ore preparation concentrate grade exceeded normalized ppm")
            });
    let consumed_energy = evidence
        .crush
        .energy
        .checked_add(evidence.grind.energy)
        .and_then(|energy| energy.checked_add(evidence.screen.powered.energy))
        .and_then(|energy| energy.checked_add(evidence.regrind.powered.energy))
        .and_then(|energy| energy.checked_add(evidence.concentration.energy))
        .unwrap_or_else(|| panic!("ore preparation consumed energy overflowed"));

    assert_eq!(
        final_matter, episode.initial_matter,
        "ore preparation must conserve world matter"
    );
    assert_eq!(
        episode.initial_energy.checked_sub(consumed_energy),
        Some(final_energy),
        "ore preparation must consume exactly the resolved work energy"
    );
    assert_eq!(
        final_crusher_condition, evidence.crush.condition_after,
        "crusher condition must match resolved wear"
    );
    assert_eq!(
        final_grinder_condition, evidence.regrind.powered.condition_after,
        "grinder condition must match resolved wear"
    );
    assert_eq!(
        final_screen_condition, evidence.screen.powered.condition_after,
        "screen condition must match resolved wear"
    );
    assert_eq!(
        final_separator_condition, evidence.concentration.condition_after,
        "separator condition must match resolved wear"
    );
    assert_eq!(
        evidence
            .screen
            .undersize_mass
            .checked_add(evidence.screen.oversize_mass),
        Some(episode.batch_mass),
        "screening must partition the complete ground batch without changing represented mass"
    );
    assert_eq!(
        undersize_mass,
        Mass::ZERO,
        "prepared feed must be consumed into concentrate and tailings"
    );
    assert_eq!(
        oversize_mass,
        Mass::ZERO,
        "oversize storage must be empty after regrind"
    );
    assert_eq!(
        concentrate_mass.checked_add(tailings_mass),
        Some(episode.batch_mass),
        "concentration outputs must conserve the prepared feed mass"
    );
    assert_eq!(
        represented_copper, expected_copper,
        "concentration must conserve exact represented copper content"
    );
    assert!(
        !concentrate_mass.is_zero(),
        "concentration must recover copper"
    );
    assert!(
        !tailings_mass.is_zero(),
        "concentration must produce physical tailings"
    );
    assert_eq!(
        concentrate_copper,
        u128::from(expected_recovered_copper_milligrams) * 1_000_000,
        "industrial concentration must apply the authored finite target recovery to exact copper content"
    );
    assert!(
        concentrate_mass.milligrams() > expected_recovered_copper_milligrams,
        "finite non-target recovery must carry physical gangue into the concentrate stream"
    );
    assert!(
        concentrate_grade_ppm > episode.input_copper_ppm && concentrate_grade_ppm < 1_000_000,
        "industrial concentration must improve feed grade without fabricating pure copper"
    );

    let qualitative_requirements = [
        (
            "crusher output matches authored particle state",
            evidence.crusher_output_matches_authoring,
        ),
        (
            "grinder output matches authored particle state",
            evidence.grinding_matches_authoring,
        ),
        (
            "grinder output resolves the authored screen cut",
            evidence.grinding_resolved_screen_cut,
        ),
        (
            "fine-grind output fits the authored screen undersize",
            evidence.screen.oversize_mass.is_zero() || evidence.regrind.fine_output_fits_undersize,
        ),
        (
            "screen oversize preserves its particle profile",
            evidence.regrind.oversize_profile_is_preserved,
        ),
        (
            "probe feed exercises variable multi-constituent gangue",
            episode.input_composition.components().len() >= 3,
        ),
        (
            "copper concentrate retains target identity while carrying selectively recovered gangue",
            concentrate_identity_is_valid && concentrate_contains_gangue,
        ),
        (
            "copper concentrate retains the liberated fine-particle state",
            concentrate_distribution_is_fine,
        ),
        (
            "finite concentration recovery leaves unrecovered copper in physical tailings",
            tailings_retain_unrecovered_copper,
        ),
        (
            "tailings retain the physically prepared fine particle state in a terminal current-tier form",
            tailings_distribution_is_fine,
        ),
    ];
    for (name, observed) in qualitative_requirements {
        assert!(
            observed,
            "ore-preparation capability contract failed: {name}"
        );
    }

    let concentration_batches = 1_u64;
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "CAPABILITY ORE_PREP seed=0x{:016X} sample={} outcome=completed reachability=bootstrapped-industrial installation=required+structurally-supported role=capability-evidence player-loop=not-claimed system-depth=[particle-state,routing,finite-work,wear,constituent-concentration] attempted={}mg execution=canonical-stage-resolution feed=[copper:{}ppm stone:{}ppm clay:{}ppm] concentrate={}mg tailings={}mg concentrate-grade={}ppm target-recovery={}ppm gangue-recovery={}ppm initial-condition=[crusher:{} grinder:{} screen:{} separator:{}ppm] stored-work=[initial:{}nJ consumed:{}nJ remaining:{}nJ] stages=[crush:{}t grind:{}t screen:{}t regrind:{}t concentrate:{}b/{}t] matter=conserved composition=exact energy=resolved",
            episode.case.seed(),
            focused_probe_role_label(episode.case.role()),
            episode.batch_mass.milligrams(),
            episode.input_copper_ppm,
            episode.input_stone_ppm,
            episode.input_clay_ppm,
            concentrate_mass.milligrams(),
            tailings_mass.milligrams(),
            concentrate_grade_ppm,
            concentration_definition.target_recovery_ppm(),
            concentration_definition.non_target_recovery_ppm(),
            episode.initial_crusher_condition.parts_per_million(),
            episode.initial_grinder_condition.parts_per_million(),
            episode.initial_screen_condition.parts_per_million(),
            episode.initial_separator_condition.parts_per_million(),
            episode.initial_energy.nanojoules(),
            consumed_energy.nanojoules(),
            final_energy.nanojoules(),
            evidence.crush.duration.value(),
            evidence.grind.duration.value(),
            evidence.screen.powered.duration.value(),
            evidence.regrind.powered.duration.value(),
            concentration_batches,
            evidence.concentration.duration.value(),
        );
    } else {
        std::println!(
            "ORE REVIEW seed=0x{:016X} sample={} role=capability-only outcome=completed pipeline=crush->grind->screen->regrind->concentrate attempted={}mg execution=canonical-stage-resolution feed=[copper:{}ppm stone:{}ppm clay:{}ppm] concentrate={}mg tailings={}mg concentrate-grade={}ppm target-recovery={}ppm gangue-recovery={}ppm stored-work=[used:{}nJ remaining:{}nJ] durations=[{}+{}+{}+{}t concentration:{}b/{}t] matter=conserved composition=exact",
            episode.case.seed(),
            focused_probe_role_label(episode.case.role()),
            episode.batch_mass.milligrams(),
            episode.input_copper_ppm,
            episode.input_stone_ppm,
            episode.input_clay_ppm,
            concentrate_mass.milligrams(),
            tailings_mass.milligrams(),
            concentrate_grade_ppm,
            concentration_definition.target_recovery_ppm(),
            concentration_definition.non_target_recovery_ppm(),
            consumed_energy.nanojoules(),
            final_energy.nanojoules(),
            evidence.crush.duration.value(),
            evidence.grind.duration.value(),
            evidence.screen.powered.duration.value(),
            evidence.regrind.powered.duration.value(),
            concentration_batches,
            evidence.concentration.duration.value(),
        );
    }
    OreProbeOutcome::Completed {
        processed_mass: episode.batch_mass,
    }
}
