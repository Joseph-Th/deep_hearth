//! Bounded exact-local replay of the primitive progression information/extraction frontier.
//!
//! The long progression campaign still owns automation, maintenance, and reinvestment evidence.
//! This episode asks a narrower question: does the same generated world and actor-visible evidence
//! still produce the same first scarce-copper decision when prospecting, tools, mining output, and
//! workshop stock all have ordinary locality and the player must physically travel between them?

use deep_hearth::content::gameplay_fixture::{locate_ground_stockpile, seed_traversable_edge};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    PROSPECTING_REGIONAL_RECONNAISSANCE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::logistics::{
    assess_player_carrying, validate_drop_equipment, validate_drop_to_ground,
    validate_pickup_equipment, validate_start_player_step,
};
use deep_hearth::mining::{
    MiningStartError, MiningTargetRequest, MiningTargetResolutionError, resolve_mining_target,
    validate_start_mining,
};
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};

use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use super::episode::{ProgressionWorldSetup, build_progression_world_unadmitted};
use super::*;

const WORKSHOP: VoxelCoord = VoxelCoord::new(-1, -4, 0);
const CORRIDOR_MAX_X: i64 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExactLocalProgressionFrontierReview {
    pub(super) natural_priority: PrimitivePriority,
    pub(super) regional_upper_bounds_ppm: [u32; PROGRESSION_REGIONAL_ZONE_COUNT],
    pub(super) direct_copper_evidence_lower_ppm: u32,
    pub(super) direct_copper_evidence_upper_ppm: u32,
    pub(super) bulk_ore_evidence_lower_ppm: u32,
    pub(super) bulk_ore_evidence_upper_ppm: u32,
    pub(super) hard_ore_evidence_lower_ppm: u32,
    pub(super) hard_ore_evidence_upper_ppm: u32,
    pub(super) bulk_sample_copper_ppm: u32,
    pub(super) information_refinement_required: bool,
    pub(super) refined_sample_copper_ppm: u32,
    pub(super) direct_second_upgrade_blocked: bool,
    pub(super) movement_steps: u64,
    pub(super) haul_trips: u64,
    pub(super) hard_sample_returned: Mass,
}

fn assert_exact_local_progression_state(registries: &Registries, state: &AppState, context: &str) {
    assert!(
        state.logistics().player().is_some() && state.survival().player().is_some(),
        "exact-local progression {context} requires complete player admission"
    );
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("exact-local progression {context} lost carried custody"));
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(carrying.position())
    );
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("exact-local progression {context} state invalid: {error}"));
}

fn locate_workshop_stockpiles(
    state: &mut AppState,
    stockpiles: [deep_hearth::inventory::StockpileId; 8],
) {
    for stockpile in stockpiles {
        locate_ground_stockpile(state, stockpile, WORKSHOP);
    }
}

fn seed_progression_corridor(state: &mut AppState) {
    for x in WORKSHOP.x()..CORRIDOR_MAX_X {
        seed_traversable_edge(
            state,
            VoxelCoord::new(x, WORKSHOP.y(), WORKSHOP.z()),
            VoxelCoord::new(x + 1, WORKSHOP.y(), WORKSHOP.z()),
        );
    }
}

fn walk_step(registries: &Registries, state: &mut AppState, destination: VoxelCoord) {
    let work = validate_start_player_step(registries, state, destination)
        .unwrap_or_else(|error| panic!("exact-local progression step admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local progression step commit failed: {error}"));
    let mut completed = false;
    while state.tick() < work.completes_at() {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("exact-local progression step tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                player_step: true,
                ..TickEventAllowance::default()
            },
            "exact-local primitive progression movement",
        );
        completed |= outcome.player_step().is_some();
    }
    assert!(completed, "exact-local progression step never completed");
}

fn walk_to_x(registries: &Registries, state: &mut AppState, x: i64, steps: &mut u64) {
    loop {
        let current = state
            .logistics()
            .player()
            .map(|player| player.position())
            .unwrap_or_else(|| panic!("exact-local progression movement lost player"));
        if current.x() == x {
            break;
        }
        let next_x = if current.x() < x {
            current.x() + 1
        } else {
            current.x() - 1
        };
        walk_step(
            registries,
            state,
            VoxelCoord::new(next_x, current.y(), current.z()),
        );
        *steps = steps
            .checked_add(1)
            .unwrap_or_else(|| panic!("exact-local progression movement count overflowed"));
    }
}

fn walk_to_region(
    registries: &Registries,
    state: &mut AppState,
    region: VoxelBounds,
    steps: &mut u64,
) {
    let x = region.min().x();
    walk_to_x(registries, state, x, steps);
    assert!(
        region.has_voxel(
            state
                .logistics()
                .player()
                .unwrap_or_else(|| panic!("exact-local progression movement lost player"))
                .position()
        ),
        "exact-local progression route did not enter requested prospecting/mining region"
    );
}

fn drop_all_carried_material(
    registries: &Registries,
    state: &mut AppState,
    destination: deep_hearth::inventory::StockpileId,
) -> Mass {
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("exact-local progression drop lost carrying state"));
    let selections = state
        .inventory()
        .lots()
        .filter(|lot| lot.stockpile() == carrying.stockpile())
        .map(|lot| MaterialLotSelection::new(lot.id(), lot.mass()))
        .collect::<Vec<_>>();
    let mass = selections
        .iter()
        .try_fold(Mass::ZERO, |total, selection| {
            state
                .inventory()
                .get_lot(selection.lot())
                .and_then(|lot| total.checked_add(lot.mass()))
        })
        .unwrap_or_else(|| panic!("exact-local progression carried material mass overflowed"));
    if selections.is_empty() {
        return Mass::ZERO;
    }
    validate_drop_to_ground(registries, state, destination, &selections)
        .unwrap_or_else(|error| panic!("exact-local progression material drop failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("exact-local progression material drop commit failed: {error}")
        });
    mass
}

fn extract_and_haul(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: EquipmentId,
    requested: Mass,
    batch_limit: Mass,
    steps: &mut u64,
    haul_trips: &mut u64,
) -> (Mass, u64) {
    walk_to_region(registries, state, target.region(), steps);
    let mut remaining = requested;
    let mut total = Mass::ZERO;
    let mut ticks = 0_u64;
    while !remaining.is_zero() {
        let carrying = assess_player_carrying(state)
            .unwrap_or_else(|| panic!("exact-local progression extraction lost carried storage"));
        let batch = remaining.min(batch_limit).min(carrying.available());
        assert!(
            !batch.is_zero(),
            "exact-local progression field kit left no carrying room for extraction"
        );
        let attempt = try_mine_and_claim(
            registries,
            state,
            target,
            carrying.stockpile(),
            equipment,
            batch,
        )
        .unwrap_or_else(|error| panic!("exact-local progression mining failed: {error}"));
        ticks = ticks
            .checked_add(attempt.ticks)
            .unwrap_or_else(|| panic!("exact-local progression mining time overflowed"));
        total = total
            .checked_add(attempt.output)
            .unwrap_or_else(|| panic!("exact-local progression output mass overflowed"));
        remaining = remaining
            .checked_sub(attempt.output)
            .unwrap_or_else(|| unreachable!("mining output cannot exceed requested mass"));
        if attempt.output < batch {
            break;
        }
    }
    walk_to_x(registries, state, WORKSHOP.x(), steps);
    let dropped = drop_all_carried_material(registries, state, destination);
    assert_eq!(
        dropped, total,
        "exact-local progression haul must return exactly the extracted material"
    );
    *haul_trips = haul_trips
        .checked_add(1)
        .unwrap_or_else(|| panic!("exact-local progression haul count overflowed"));
    (total, ticks)
}

fn assemble_local_field_kit(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
) -> (EquipmentId, EquipmentId) {
    craft_for_profile(
        registries,
        state,
        raw,
        native,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_PICK),
    );
    let pick = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, shaped)
        .unwrap_or_else(|error| panic!("exact-local progression pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local progression pick commit failed: {error}"));
    craft_for_profile(
        registries,
        state,
        raw,
        native,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_GEOLOGICAL_HAMMER),
    );
    let hammer =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, shaped)
            .unwrap_or_else(|error| {
                panic!("exact-local progression hammer assembly failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("exact-local progression hammer commit failed: {error}")
            });
    for equipment in [pick, hammer] {
        assert!(
            state
                .available_local_equipment()
                .any(|record| record.id() == equipment),
            "exact-local progression must rediscover newly assembled local field equipment"
        );
        validate_pickup_equipment(registries, state, equipment)
            .unwrap_or_else(|error| {
                panic!("exact-local progression field-equipment pickup failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("exact-local progression field-equipment pickup commit failed: {error}")
            });
    }
    (pick, hammer)
}

pub(super) fn run_exact_local_progression_frontier(
    registries: &Registries,
    case: FocusedProbeCase,
) -> ExactLocalProgressionFrontierReview {
    assert!(matches!(
        case.role(),
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay
    ));
    let (low_trace_grade_regime, ore_opportunity_batch_budget) = progression_world_parameters(case);
    let (world, _diagnostics) = build_progression_world_unadmitted(
        registries,
        case.seed(),
        low_trace_grade_regime,
        ore_opportunity_batch_budget,
    );
    let ProgressionWorldSetup {
        mut state,
        mined_mass,
        raw_surplus: _,
        stone_pick_batch_limit,
        stone_hardness_limit,
        reinforced_hardness_limit: _,
        pick_upgrade_native,
        crank_upgrade_native,
        concurrent_soft_mass: _,
        raw,
        shaped,
        ore_storage,
        hard_ore_storage,
        refined_clue_storage,
        native_storage,
        crushed_storage,
        separation_residue_storage,
        visible_clue_requests,
        refined_clue_sample_mass,
    } = world;
    locate_workshop_stockpiles(
        &mut state,
        [
            raw,
            shaped,
            ore_storage,
            hard_ore_storage,
            refined_clue_storage,
            native_storage,
            crushed_storage,
            separation_residue_storage,
        ],
    );
    seed_progression_corridor(&mut state);
    validate_admit_player(registries, &state, WORKSHOP)
        .unwrap_or_else(|error| panic!("exact-local progression player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("exact-local progression player admission commit failed: {error}")
        });
    assert_exact_local_progression_state(registries, &state, "start");
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("exact-local progression matter audit failed: {error}"))
        .total();
    let mut movement_steps = 0_u64;
    let mut haul_trips = 0_u64;

    for request in visible_clue_requests {
        assert!(matches!(
            resolve_mining_target(&state, request),
            Err(MiningTargetResolutionError::NoEvidence { .. })
        ));
    }

    let regional_zones = std::array::from_fn(progression_regional_bounds);
    for region in regional_zones {
        walk_to_region(registries, &mut state, region, &mut movement_steps);
        acquire_copper_evidence(
            registries,
            &mut state,
            PROSPECTING_REGIONAL_RECONNAISSANCE,
            region,
        );
    }
    let regional_upper_bounds_ppm = regional_zones.map(|region| {
        let (lower, upper) =
            observed_copper_bounds(&state, MiningTargetRequest::new(region, MATERIAL_COPPER));
        assert_eq!(lower, 0);
        upper
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
    for request in clue_requests.iter().copied() {
        walk_to_region(
            registries,
            &mut state,
            request.region(),
            &mut movement_steps,
        );
        acquire_copper_evidence(
            registries,
            &mut state,
            PROSPECTING_FIELD_INSPECTION,
            request.region(),
        );
    }
    let mut surface_clues = Vec::new();
    let mut refinement = None;
    for request in clue_requests.iter().copied() {
        let (lower_ppm, upper_ppm) = observed_copper_bounds(&state, request);
        match resolve_mining_target(&state, request) {
            Ok(_) => surface_clues.push(ObservedCopperClue {
                request,
                lower_ppm,
                upper_ppm,
            }),
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }) if region == request.region() && lower_ppm == 0 => {
                assert!(refinement.is_none());
                refinement = Some((request, lower_ppm, upper_ppm));
            }
            Err(error) => panic!("exact-local progression surface evidence failed: {error}"),
        }
    }
    let information_refinement_required = refinement.is_some();

    walk_to_x(registries, &mut state, WORKSHOP.x(), &mut movement_steps);
    let (pick, hammer) =
        assemble_local_field_kit(registries, &mut state, raw, native_storage, shaped);
    let mut mineable_clues = Vec::new();
    let mut hardness_blocked_clues = Vec::new();
    for clue in surface_clues.iter().copied() {
        walk_to_region(
            registries,
            &mut state,
            clue.request.region(),
            &mut movement_steps,
        );
        acquire_copper_evidence_with_equipment(
            registries,
            &mut state,
            PROSPECTING_DETAILED_FIELD_SURVEY,
            clue.request.region(),
            Some(hammer),
        );
        let sampled = observed_resolved_copper_clue(&state, clue.request);
        let hardness = resolve_progression_mining_target(&state, clue.request)
            .excavation_hardness()
            .unwrap_or_else(|| panic!("exact-local progression sample lost hardness"));
        if hardness.upper() <= stone_hardness_limit {
            mineable_clues.push(sampled);
        } else {
            hardness_blocked_clues.push(sampled);
        }
    }
    let hard_clue = strongest_observed_copper_clue(hardness_blocked_clues.iter().copied());
    let direct_copper_clue = strongest_observed_copper_clue(mineable_clues.iter().copied());
    let bulk_ore_clue = strongest_observed_copper_clue(
        mineable_clues
            .iter()
            .copied()
            .filter(|clue| clue.request != direct_copper_clue.request),
    );

    walk_to_region(
        registries,
        &mut state,
        hard_clue.request.region(),
        &mut movement_steps,
    );
    let hard_target = resolve_progression_mining_target(&state, hard_clue.request);
    let carried = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("exact-local progression hard-block test lost carrying state"));
    let hard_upper = hard_target
        .excavation_hardness()
        .unwrap_or_else(|| panic!("exact-local progression hard clue lost hardness"))
        .upper();
    assert_eq!(
        validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            hard_target,
            carried.stockpile(),
            pick,
            mined_mass,
        )
        .err(),
        Some(
            MiningStartError::ExcavationHardnessEvidenceExceedsCapability {
                observed_upper: hard_upper,
                maximum: stone_hardness_limit,
            }
        )
    );

    let (bulk_output, _) = extract_and_haul(
        registries,
        &mut state,
        bulk_ore_clue.request,
        ore_storage,
        pick,
        mined_mass,
        stone_pick_batch_limit,
        &mut movement_steps,
        &mut haul_trips,
    );
    assert_eq!(bulk_output, mined_mass);
    let bulk_sample = observe_material_sample(&state, ore_storage, "exact-local bulk ore");
    assert_eq!(bulk_sample.commodity.form(), FORM_ORE);

    let (first_direct, _) = extract_and_haul(
        registries,
        &mut state,
        direct_copper_clue.request,
        native_storage,
        pick,
        pick_upgrade_native,
        stone_pick_batch_limit,
        &mut movement_steps,
        &mut haul_trips,
    );
    assert_eq!(first_direct, pick_upgrade_native);
    let direct_sample =
        observe_material_sample(&state, native_storage, "exact-local direct copper");
    assert_eq!(direct_sample.commodity.form(), FORM_NATIVE_METAL);
    let (second_direct, _) = extract_and_haul(
        registries,
        &mut state,
        direct_copper_clue.request,
        native_storage,
        pick,
        crank_upgrade_native,
        stone_pick_batch_limit,
        &mut movement_steps,
        &mut haul_trips,
    );
    let direct_second_upgrade_blocked = second_direct < crank_upgrade_native;
    assert!(direct_second_upgrade_blocked);

    let refined_sample_copper_ppm = if let Some((request, coarse_lower, coarse_upper)) = refinement
    {
        assert!(matches!(
            resolve_mining_target(&state, request),
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. })
        ));
        walk_to_region(
            registries,
            &mut state,
            request.region(),
            &mut movement_steps,
        );
        acquire_copper_evidence_with_equipment(
            registries,
            &mut state,
            PROSPECTING_DETAILED_FIELD_SURVEY,
            request.region(),
            Some(hammer),
        );
        let refined = observed_resolved_copper_clue(&state, request);
        assert!(refined.lower_ppm > coarse_lower && refined.upper_ppm < coarse_upper);
        let (sampled, _) = extract_and_haul(
            registries,
            &mut state,
            request,
            refined_clue_storage,
            pick,
            refined_clue_sample_mass,
            stone_pick_batch_limit,
            &mut movement_steps,
            &mut haul_trips,
        );
        assert_eq!(sampled, refined_clue_sample_mass);
        let sample =
            observe_material_sample(&state, refined_clue_storage, "exact-local refined clue");
        assert_eq!(sample.commodity.form(), FORM_ORE);
        assert!(bulk_sample.copper_ppm > sample.copper_ppm);
        sample.copper_ppm
    } else {
        0
    };

    walk_to_x(registries, &mut state, WORKSHOP.x(), &mut movement_steps);
    let natural_priority = observed_primitive_priority(hard_clue, bulk_sample);
    let (hard_sample_returned, investment_handling) = match natural_priority {
        PrimitivePriority::PickFirst => {
            validate_drop_equipment(registries, &state, pick)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression pick drop before reinforcement failed: {error}")
                })
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!(
                        "exact-local progression pick drop before reinforcement commit failed: {error}"
                    )
                });
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            assert!(
                state.available_local_equipment().any(|record| {
                    record.id() == pick && record.definition() == EQUIPMENT_COPPER_REINFORCED_PICK
                }),
                "exact-local progression must rediscover the locally reinforced pick before reuse"
            );
            validate_pickup_equipment(registries, &state, pick)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression reinforced-pick pickup failed: {error}")
                })
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression reinforced-pick pickup commit failed: {error}")
                });
            let (returned, _) = extract_and_haul(
                registries,
                &mut state,
                hard_clue.request,
                hard_ore_storage,
                pick,
                mined_mass,
                stone_pick_batch_limit,
                &mut movement_steps,
                &mut haul_trips,
            );
            assert_eq!(returned, mined_mass);
            (returned, "drop-modify-repickup")
        }
        PrimitivePriority::CrankFirst => {
            craft_for_profile(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                equipment_assembly_profile(registries, EQUIPMENT_STONE_HAND_CRANK),
            );
            let crank =
                validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_HAND_CRANK, shaped)
                    .unwrap_or_else(|error| {
                        panic!("exact-local progression crank assembly failed: {error}")
                    })
                    .commit(&mut state)
                    .unwrap_or_else(|error| {
                        panic!("exact-local progression crank commit failed: {error}")
                    });
            assert!(
                state
                    .available_local_equipment()
                    .any(|record| record.id() == crank),
                "exact-local progression must rediscover its locally assembled crank"
            );
            reinforce_crank(registries, &mut state, raw, native_storage, shaped, crank);
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(crank)
                    .map(|record| record.definition()),
                Some(EQUIPMENT_COPPER_REINFORCED_HAND_CRANK)
            );
            (Mass::ZERO, "stationary-local")
        }
    };

    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.definition()),
        Some(match natural_priority {
            PrimitivePriority::PickFirst => EQUIPMENT_COPPER_REINFORCED_PICK,
            PrimitivePriority::CrankFirst => EQUIPMENT_STONE_PICK,
        })
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "exact-local progression final matter audit failed: {error}"
            ))
            .total(),
        matter_before,
        "exact-local progression must conserve matter through travel, extraction, and first investment"
    );
    assert_exact_local_progression_state(registries, &state, "completed frontier");

    reviewln!(
        "PROGRESSION LOCAL FRONTIER seed=0x{:016X} sample={} evidence=ordinary-exact-local generation=matched-progression-world corridor=[workshop:-1 clues:0,2,4,6 max-distance:7 steps:{}] information=[regional:{}..{}ppm surface:{} resolved:{} refinement:{} hard:{}..{}ppm bulk:{}..{}ppm direct:{}..{}ppm] field-kit=[assembled-after-admission:true rediscovered:true carried:true] extraction=[bulk:{}mg direct-first:{}mg direct-second:{}mg shortage:{} haul-trips:{}] decision=[selected:{} rule:hard-lower>owned-bulk bulk-grade:{}ppm] first-investment=[local:true handling:{} hard-sample-returned:{}mg] matter=conserved",
        case.seed(),
        case.role().label(),
        movement_steps,
        regional_upper_bounds_ppm[0],
        regional_upper_bounds_ppm[1],
        clue_requests.len(),
        surface_clues.len(),
        information_refinement_required,
        hard_clue.lower_ppm,
        hard_clue.upper_ppm,
        bulk_ore_clue.lower_ppm,
        bulk_ore_clue.upper_ppm,
        direct_copper_clue.lower_ppm,
        direct_copper_clue.upper_ppm,
        bulk_output.milligrams(),
        first_direct.milligrams(),
        second_direct.milligrams(),
        direct_second_upgrade_blocked,
        haul_trips,
        natural_priority.label(),
        bulk_sample.copper_ppm,
        investment_handling,
        hard_sample_returned.milligrams(),
    );

    ExactLocalProgressionFrontierReview {
        natural_priority,
        regional_upper_bounds_ppm,
        direct_copper_evidence_lower_ppm: direct_copper_clue.lower_ppm,
        direct_copper_evidence_upper_ppm: direct_copper_clue.upper_ppm,
        bulk_ore_evidence_lower_ppm: bulk_ore_clue.lower_ppm,
        bulk_ore_evidence_upper_ppm: bulk_ore_clue.upper_ppm,
        hard_ore_evidence_lower_ppm: hard_clue.lower_ppm,
        hard_ore_evidence_upper_ppm: hard_clue.upper_ppm,
        bulk_sample_copper_ppm: bulk_sample.copper_ppm,
        information_refinement_required,
        refined_sample_copper_ppm,
        direct_second_upgrade_blocked,
        movement_steps,
        haul_trips,
        hard_sample_returned,
    }
}
