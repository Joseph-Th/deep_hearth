//! Canonical preservation-infrastructure gameplay subepisode.

use std::cmp::Reverse;
use std::collections::BTreeSet;

use super::preservation::{
    PreservationCandidate, preservation_buildable_definitions, preservation_candidate_for_policy,
    preservation_candidates,
};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreservationSelectionKind {
    AttentionEfficient,
    BalancedFrontier,
    MaximumProtection,
}

impl PreservationSelectionKind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::AttentionEfficient => "attention-efficient",
            Self::BalancedFrontier => "balanced-frontier",
            Self::MaximumProtection => "maximum-protection",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct PreservationCandidateProjection {
    pub(in super::super) definition: StorageDefinitionId,
    pub(in super::super) production_ticks: u64,
    pub(in super::super) raw_material_mass_mg: u64,
    pub(in super::super) remaining_fresh_ticks: u64,
}

#[derive(Clone, Copy)]
struct PreservationScenarioSpec {
    food: FoodDefinition,
    maximum_construction_ticks: u64,
    matched_observation_ticks: u64,
    bootstrap_age_ticks: u64,
    food_mass: Mass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PreservationInfrastructureReview {
    pub(super) selection_kind: PreservationSelectionKind,
    pub(super) candidate_count: usize,
    pub(super) storage_definition: StorageDefinitionId,
    pub(super) fastest_definition: StorageDefinitionId,
    pub(super) fastest_ticks: u64,
    pub(super) fastest_preservation_multiplier_ppm: u32,
    pub(super) strongest_definition: StorageDefinitionId,
    pub(super) strongest_ticks: u64,
    pub(super) strongest_preservation_multiplier_ppm: u32,
    pub(super) food_commodity: CommodityKey,
    pub(super) construction_stages: usize,
    pub(super) production_ticks: u64,
    pub(super) observation_ticks: u64,
    pub(super) raw_material_mass_mg: u64,
    pub(super) embodied_mass_mg: u64,
    pub(super) residual_mass_mg: u64,
    pub(super) capacity_mass_mg: u64,
    pub(super) preservation_multiplier_ppm: u32,
    pub(super) bootstrap_age_ticks: u64,
    pub(super) ambient_age_after_ticks: u64,
    pub(super) enclosed_age_after_ticks: u64,
    pub(super) enclosed_remaining_fresh_ticks: u64,
    pub(super) age_saved_ticks: u64,
    pub(super) ambient_spoiled: bool,
    pub(super) enclosed_fresh: bool,
    pub(super) metabolic_cost_nj: u128,
    pub(super) hydration_cost_ul: u64,
    pub(super) dismantle_ticks: u64,
    pub(super) dismantle_metabolic_cost_nj: u128,
    pub(super) dismantle_hydration_cost_ul: u64,
    pub(super) recovered_enclosure_mass_mg: u64,
}

fn preservation_scenario_spec(
    seed: u64,
    candidates: &[PreservationCandidate],
    food: FoodDefinition,
    food_mass: Mass,
) -> PreservationScenarioSpec {
    let maximum_construction_ticks = candidates
        .iter()
        .map(|candidate| candidate.construction_plan.attention_ticks)
        .max()
        .unwrap_or_else(|| unreachable!("preservation candidates are nonempty"));
    assert!(
        food.shelf_life().value() > maximum_construction_ticks.saturating_add(1),
        "protected reserve spoils before any matched preservation-construction comparison is observable"
    );
    let shelf_life_ticks = food.shelf_life().value();
    let maximum_observation = shelf_life_ticks
        .checked_sub(maximum_construction_ticks)
        .and_then(|ticks| ticks.checked_sub(1))
        .unwrap_or_else(|| unreachable!("food selection guarantees a positive observation window"));
    let observation_floor = maximum_observation.clamp(1, 200);
    let observation_ceiling = maximum_observation.min(1_200);
    let matched_observation_ticks = observation_floor
        + mix64(seed ^ 0x5052_4553_484F_5249) % (observation_ceiling - observation_floor + 1);
    let bootstrap_age_ticks = shelf_life_ticks
        .checked_sub(maximum_construction_ticks)
        .and_then(|ticks| ticks.checked_sub(matched_observation_ticks))
        .unwrap_or_else(|| unreachable!("bounded preservation horizon was derived above"));
    PreservationScenarioSpec {
        food,
        maximum_construction_ticks,
        matched_observation_ticks,
        bootstrap_age_ticks,
        food_mass,
    }
}

pub(in super::super) fn project_preservation_candidates_with_raw_opportunity(
    registries: &Registries,
    seed: u64,
    food: FoodDefinition,
    food_mass: Mass,
    available_raw_materials: Option<&[(CommodityKey, Mass)]>,
) -> Vec<PreservationCandidateProjection> {
    let buildable = available_raw_materials
        .map(|available| preservation_buildable_definitions(registries, food_mass, available));
    let candidates = preservation_candidates(registries)
        .into_iter()
        .filter(|candidate| candidate.capacity >= food_mass)
        .filter(|candidate| {
            buildable
                .as_ref()
                .is_none_or(|definitions| definitions.contains(&candidate.definition))
        })
        .collect::<Vec<_>>();
    assert!(
        !candidates.is_empty(),
        "protected reserve exceeds every authored preservation enclosure"
    );
    let scenario = preservation_scenario_spec(seed, &candidates, food, food_mass);
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x5052_4553_4552_5643));
    let stockpile = seed_stockpile(
        &mut state,
        scenario.food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let lot = seed_lot(
        registries,
        &mut state,
        stockpile,
        scenario.food.commodity(),
        scenario.food_mass,
        ROOM_TEMPERATURE,
    );
    seed_preexisting_world_age(
        &mut state,
        SimulationTick::new(scenario.bootstrap_age_ticks),
    );
    let projection_started_at = state.tick();
    let assessment_at = SimulationTick::new(
        projection_started_at
            .value()
            .checked_add(scenario.maximum_construction_ticks)
            .and_then(|tick| tick.checked_add(scenario.matched_observation_ticks))
            .unwrap_or_else(|| panic!("preservation frontier assessment tick overflowed")),
    );
    candidates
        .into_iter()
        .map(|candidate| {
            let transition_at = SimulationTick::new(
                projection_started_at
                    .value()
                    .checked_add(candidate.construction_plan.attention_ticks)
                    .unwrap_or_else(|| panic!("preservation frontier transition tick overflowed")),
            );
            let projected = project_food_freshness_after_storage_transition(
                registries,
                &state,
                lot,
                transition_at,
                candidate.definition,
                assessment_at,
            )
            .unwrap_or_else(|error| {
                panic!("preservation frontier freshness projection failed: {error:?}")
            });
            let remaining_fresh_ticks = match projected {
                FoodFreshness::Fresh { age: _, remaining } => remaining.value(),
                FoodFreshness::Spoiled { .. } => 0,
            };
            PreservationCandidateProjection {
                definition: candidate.definition,
                production_ticks: candidate.construction_plan.attention_ticks,
                raw_material_mass_mg: candidate.construction_plan.raw_mass.milligrams(),
                remaining_fresh_ticks,
            }
        })
        .collect()
}

pub(in super::super) fn select_preservation_projection(
    behavior_seed: u64,
    projections: &[PreservationCandidateProjection],
) -> PreservationCandidateProjection {
    let attention_value_ppm = preservation_freshness_return_threshold_ppm(behavior_seed);
    select_preservation_projection_for_attention_value(attention_value_ppm, projections)
}

pub(in super::super) fn select_preservation_projection_for_attention_value(
    attention_value_ppm: u32,
    projections: &[PreservationCandidateProjection],
) -> PreservationCandidateProjection {
    let value_key = |projection: &PreservationCandidateProjection| {
        let freshness_value = i128::from(projection.remaining_fresh_ticks)
            .checked_mul(1_000_000)
            .unwrap_or_else(|| panic!("preservation projected freshness value overflowed"));
        let attention_cost = i128::from(projection.production_ticks)
            .checked_mul(i128::from(attention_value_ppm))
            .unwrap_or_else(|| panic!("preservation projected attention value overflowed"));
        (
            freshness_value - attention_cost,
            projection.remaining_fresh_ticks,
            Reverse(projection.raw_material_mass_mg),
            Reverse(projection.production_ticks),
        )
    };
    let best_value = projections
        .iter()
        .map(value_key)
        .max()
        .unwrap_or_else(|| panic!("preservation projection has no authored candidates"));
    let selected = projections
        .iter()
        .copied()
        .filter(|projection| value_key(projection) == best_value)
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        1,
        "preservation value frontier has physically equivalent winners; author another observable tradeoff instead of using identity as a tie-break"
    );
    selected[0]
}

fn projection_physically_dominates(
    candidate: PreservationCandidateProjection,
    other: PreservationCandidateProjection,
) -> bool {
    candidate.production_ticks <= other.production_ticks
        && candidate.raw_material_mass_mg <= other.raw_material_mass_mg
        && candidate.remaining_fresh_ticks >= other.remaining_fresh_ticks
        && (candidate.production_ticks < other.production_ticks
            || candidate.raw_material_mass_mg < other.raw_material_mass_mg
            || candidate.remaining_fresh_ticks > other.remaining_fresh_ticks)
}

/// Returns candidate definitions that are not strictly worse in construction attention, raw mass,
/// and matched remaining freshness than another feasible preservation option.
pub(in super::super) fn preservation_physical_frontier(
    projections: &[PreservationCandidateProjection],
) -> BTreeSet<StorageDefinitionId> {
    projections
        .iter()
        .copied()
        .filter(|projection| {
            !projections.iter().copied().any(|candidate| {
                candidate.definition != projection.definition
                    && projection_physically_dominates(candidate, *projection)
            })
        })
        .map(|projection| projection.definition)
        .collect()
}

fn attention_value_probe_points(projections: &[PreservationCandidateProjection]) -> BTreeSet<u32> {
    const MINIMUM: u32 = 1_000_000;
    const MAXIMUM: u32 = 4_000_000;
    let mut values = BTreeSet::from([MINIMUM, MAXIMUM]);
    for (index, left) in projections.iter().enumerate() {
        for right in &projections[index + 1..] {
            if left.production_ticks == right.production_ticks {
                continue;
            }
            let freshness_delta = i128::from(left.remaining_fresh_ticks)
                .checked_sub(i128::from(right.remaining_fresh_ticks))
                .and_then(|delta| delta.checked_mul(1_000_000))
                .unwrap_or_else(|| panic!("preservation policy crossover freshness overflowed"));
            let attention_delta = i128::from(left.production_ticks)
                .checked_sub(i128::from(right.production_ticks))
                .unwrap_or_else(|| panic!("preservation policy crossover attention overflowed"));
            if freshness_delta == 0 || freshness_delta.signum() != attention_delta.signum() {
                continue;
            }
            let crossover = freshness_delta.unsigned_abs() / attention_delta.unsigned_abs();
            let crossover = u32::try_from(crossover).unwrap_or(u32::MAX);
            for offset in -2_i64..=2 {
                let value = i64::from(crossover).saturating_add(offset);
                if (i64::from(MINIMUM)..=i64::from(MAXIMUM)).contains(&value) {
                    values.insert(u32::try_from(value).unwrap_or_else(|_| {
                        unreachable!("bounded preservation attention value fits u32")
                    }));
                }
            }
        }
    }
    values
}

/// Returns every preservation definition that can win the actor's exact linear value rule anywhere
/// in the authored 1.0x..4.0x attention-value interval. Pairwise crossover neighborhoods make the
/// probe exact for integer-valued policy changes without scanning millions of redundant values.
pub(in super::super) fn preservation_policy_reachable_definitions(
    projections: &[PreservationCandidateProjection],
) -> BTreeSet<StorageDefinitionId> {
    attention_value_probe_points(projections)
        .into_iter()
        .map(|attention_value| {
            select_preservation_projection_for_attention_value(attention_value, projections)
                .definition
        })
        .collect()
}

pub(super) fn evaluate_preservation_infrastructure_definition_with_raw_opportunity(
    registries: &Registries,
    seed: u64,
    food: FoodDefinition,
    food_mass: Mass,
    storage_definition: StorageDefinitionId,
    available_raw_materials: Option<&[(CommodityKey, Mass)]>,
) -> PreservationInfrastructureReview {
    let buildable = available_raw_materials
        .map(|available| preservation_buildable_definitions(registries, food_mass, available));
    let candidates = preservation_candidates(registries)
        .into_iter()
        .filter(|candidate| candidate.capacity >= food_mass)
        .filter(|candidate| {
            buildable
                .as_ref()
                .is_none_or(|definitions| definitions.contains(&candidate.definition))
        })
        .collect::<Vec<_>>();
    let selected_index = candidates
        .iter()
        .position(|candidate| candidate.definition == storage_definition)
        .unwrap_or_else(|| unreachable!("selected preservation definition came from candidates"));
    let fastest_index = preservation_candidate_for_policy(
        &candidates,
        PreservationInvestmentPolicy::AttentionEfficient,
    );
    let strongest_index = preservation_candidate_for_policy(
        &candidates,
        PreservationInvestmentPolicy::MaximumProtection,
    );
    let selected = candidates[selected_index].clone();
    let fastest = &candidates[fastest_index];
    let strongest = &candidates[strongest_index];
    let selection_kind = if selected_index == fastest_index {
        PreservationSelectionKind::AttentionEfficient
    } else if selected_index == strongest_index {
        PreservationSelectionKind::MaximumProtection
    } else {
        PreservationSelectionKind::BalancedFrontier
    };
    assert_eq!(storage_definition, selected.definition);
    let definition = registries
        .storage()
        .get(storage_definition)
        .unwrap_or_else(|| unreachable!("selected storage definition came from this registry"));
    let construction_plan = &selected.construction_plan;
    let construction_ticks = construction_plan.attention_ticks;
    let construction_stages = construction_plan
        .routes
        .iter()
        .map(|route| route.steps.len())
        .sum::<usize>();
    let scenario = preservation_scenario_spec(seed, &candidates, food, food_mass);
    let shelf_life_ticks = food.shelf_life().value();
    let bootstrap_age_ticks = scenario.bootstrap_age_ticks;
    let observation_ticks = scenario
        .matched_observation_ticks
        .checked_add(
            scenario
                .maximum_construction_ticks
                .checked_sub(construction_ticks)
                .unwrap_or_else(|| {
                    unreachable!("selected construction is within candidate maximum")
                }),
        )
        .unwrap_or_else(|| panic!("preservation matched observation duration overflowed"));
    let food_mass = scenario.food_mass;
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x5052_4553_4552_5643));
    let enclosed_food = seed_stockpile(
        &mut state,
        food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let ambient_food = seed_stockpile(
        &mut state,
        food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let enclosed_lot = seed_lot(
        registries,
        &mut state,
        enclosed_food,
        food.commodity(),
        food_mass,
        ROOM_TEMPERATURE,
    );
    let ambient_lot = seed_lot(
        registries,
        &mut state,
        ambient_food,
        food.commodity(),
        food_mass,
        ROOM_TEMPERATURE,
    );
    seed_preexisting_world_age(&mut state, SimulationTick::new(bootstrap_age_ticks));
    let assembled = seed_stockpile(
        &mut state,
        construction_plan.raw_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let dismantle_recovery = seed_stockpile(
        &mut state,
        definition.assembly_profile().input_mass(),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let mut maximum_raw_requirements = BTreeMap::<CommodityKey, Mass>::new();
    if let Some(available) = available_raw_materials {
        for (commodity, mass) in available {
            maximum_raw_requirements
                .entry(*commodity)
                .and_modify(|current| {
                    *current = current
                        .checked_add(*mass)
                        .unwrap_or_else(|| panic!("preservation raw opportunity overflowed"));
                })
                .or_insert(*mass);
        }
    } else {
        for candidate in &candidates {
            let mut candidate_requirements = BTreeMap::<CommodityKey, Mass>::new();
            for route in &candidate.construction_plan.routes {
                let total = candidate_requirements
                    .get(&route.raw_commodity)
                    .copied()
                    .unwrap_or(Mass::ZERO)
                    .checked_add(route.raw_mass)
                    .unwrap_or_else(|| panic!("preservation candidate raw requirement overflowed"));
                candidate_requirements.insert(route.raw_commodity, total);
            }
            for (commodity, mass) in candidate_requirements {
                maximum_raw_requirements
                    .entry(commodity)
                    .and_modify(|current| *current = (*current).max(mass))
                    .or_insert(mass);
            }
        }
    }
    let mut raw_sources = BTreeMap::<CommodityKey, StockpileId>::new();
    for (commodity, mass) in maximum_raw_requirements {
        let source = seed_stockpile(
            &mut state,
            mass,
            StockpileStorageProfile::unbounded_solid_only(),
        );
        seed_lot(
            registries,
            &mut state,
            source,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
        raw_sources.insert(commodity, source);
    }
    let route_sources = construction_plan
        .routes
        .iter()
        .map(|route| {
            assert!(
                !route.steps.is_empty(),
                "ordinary preservation investment must be produced from a manual construction route rather than fixture-ready enclosure matter"
            );
            *raw_sources
                .get(&route.raw_commodity)
                .unwrap_or_else(|| {
                    panic!(
                        "selected preservation route is not funded by the disclosed raw opportunity"
                    )
                })
        })
        .collect::<Vec<_>>();
    let route_destinations = construction_plan
        .routes
        .iter()
        .map(|route| {
            (0..route.steps.len())
                .map(|index| {
                    if index + 1 == route.steps.len() {
                        assembled
                    } else {
                        seed_stockpile(
                            &mut state,
                            route.raw_mass,
                            StockpileStorageProfile::unbounded_solid_only(),
                        )
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    // Player admission ends fixture mutation; the subepisode then uses only canonical production
    // work and simulation ticks.
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("preservation infrastructure player setup failed: {error}"));
    let projection_started_at = state.tick();
    let transition_at = SimulationTick::new(
        projection_started_at
            .value()
            .checked_add(construction_ticks)
            .unwrap_or_else(|| panic!("preservation transition forecast tick overflowed")),
    );
    let assessment_at = SimulationTick::new(
        projection_started_at
            .value()
            .checked_add(scenario.maximum_construction_ticks)
            .and_then(|tick| tick.checked_add(scenario.matched_observation_ticks))
            .unwrap_or_else(|| panic!("preservation assessment forecast tick overflowed")),
    );
    let projected_freshness = project_food_freshness_after_storage_transition(
        registries,
        &state,
        enclosed_lot,
        transition_at,
        definition.id(),
        assessment_at,
    )
    .unwrap_or_else(|error| panic!("preservation investment freshness forecast failed: {error:?}"));
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("preservation infrastructure player disappeared at setup"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("preservation infrastructure matter setup failed: {error}"))
        .total();
    let mut executed_ticks = 0_u64;
    let mut executed_stages = 0_usize;
    for ((route, initial_source), destinations) in construction_plan
        .routes
        .iter()
        .zip(route_sources)
        .zip(route_destinations)
    {
        let mut source = initial_source;
        for (step, destination) in route.steps.iter().zip(destinations) {
            assert!(
                state
                    .inventory()
                    .get_stockpile(source)
                    .is_some_and(|stockpile| stockpile.get_mass(step.input) >= step.input_mass),
                "manual preservation construction input must come from the previous canonical craft stage"
            );
            let craft = select_manual_craft_request(
                registries,
                &state,
                step.process,
                source,
                step.batches,
                "preservation construction",
            );
            let job = validate_start_manual_craft(
                registries,
                &state,
                ManualCraftStartRequest::new(craft, destination),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "preservation construction process {} failed: {error}",
                    step.process.value()
                )
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!(
                    "preservation construction process {} commit failed: {error}",
                    step.process.value()
                )
            });
            let actual_ticks = state
                .production()
                .get_job(job)
                .map(|record| record.active_duration().value())
                .unwrap_or_else(|| panic!("preservation construction job disappeared after start"));
            assert_eq!(actual_ticks, step.duration_ticks);
            finish_uninterrupted_production_job(
                registries,
                &mut state,
                job,
                deep_hearth::core::time::TickSpan::new(actual_ticks),
                "preservation construction",
            );
            assert!(
                state
                    .inventory()
                    .get_stockpile(destination)
                    .is_some_and(|stockpile| !stockpile.get_mass(step.output).is_zero()),
                "manual preservation construction stage must produce its downstream commodity"
            );
            executed_ticks = executed_ticks
                .checked_add(actual_ticks)
                .unwrap_or_else(|| panic!("preservation construction duration overflowed"));
            executed_stages += 1;
            source = destination;
        }
    }
    assert_eq!(executed_ticks, construction_ticks);
    assert_eq!(executed_stages, construction_stages);
    let survival_after_joinery = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("preservation infrastructure player disappeared after joinery"));
    let metabolic_cost_nj = survival_before
        .metabolic_energy()
        .checked_sub(survival_after_joinery.metabolic_energy())
        .unwrap_or_else(|| unreachable!("manual preservation work cannot create metabolic reserve"))
        .nanojoules();
    let hydration_cost_ul = survival_before
        .hydration()
        .checked_sub(survival_after_joinery.hydration())
        .unwrap_or_else(|| unreachable!("manual preservation work cannot create hydration reserve"))
        .microliters();
    for input in definition.assembly_profile().inputs() {
        assert!(
            state
                .inventory()
                .get_stockpile(assembled)
                .is_some_and(|record| record.get_mass(input.commodity()) >= input.mass()),
            "manual construction must produce every authored enclosure assembly input"
        );
    }
    let age_at_construction = fresh_age(registries, &state, enclosed_lot);
    assert_eq!(
        age_at_construction,
        bootstrap_age_ticks + construction_ticks
    );
    assert_eq!(
        fresh_age(registries, &state, ambient_lot),
        age_at_construction
    );

    validate_build_storage_enclosure(
        registries,
        &state,
        definition.id(),
        enclosed_food,
        assembled,
    )
    .unwrap_or_else(|error| panic!("preservation enclosure construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("preservation enclosure construction commit failed: {error}"));
    assert_eq!(
        fresh_age(registries, &state, enclosed_lot),
        age_at_construction
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(enclosed_food)
            .and_then(|record| record.enclosure())
            .map(|enclosure| enclosure.embodied_mass()),
        Some(definition.assembly_profile().input_mass())
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "preservation construction matter audit failed: {error}"
            ))
            .total(),
        matter_before,
        "constructing preservation storage must transfer shaped matter into infrastructure ownership"
    );

    advance_idle_ticks(
        registries,
        &mut state,
        observation_ticks,
        "preservation observation",
    );
    let (ambient_spoiled, ambient_age_after_ticks) = freshness_age(registries, &state, ambient_lot);
    let (enclosed_spoiled, enclosed_age_after_ticks) =
        freshness_age(registries, &state, enclosed_lot);
    let enclosed_remaining_fresh_ticks =
        match assess_food_freshness(registries, &state, enclosed_lot).unwrap_or_else(|error| {
            panic!("preservation remaining-freshness audit failed: {error:?}")
        }) {
            FoodFreshness::Fresh { age: _, remaining } => remaining.value(),
            FoodFreshness::Spoiled { .. } => 0,
        };
    assert_eq!(state.tick(), assessment_at);
    assert_eq!(
        assess_food_freshness(registries, &state, enclosed_lot),
        Ok(projected_freshness),
        "survival-owned prospective freshness must agree exactly with the canonical construction/tick outcome"
    );
    let age_saved_ticks = ambient_age_after_ticks
        .checked_sub(enclosed_age_after_ticks)
        .unwrap_or_else(|| unreachable!("preservation cannot age food faster than ambient"));
    assert!(age_saved_ticks > 0);
    assert_eq!(
        ambient_age_after_ticks,
        bootstrap_age_ticks + construction_ticks + observation_ticks
    );
    assert!(
        ambient_spoiled,
        "matched ambient witness must cross its authored spoilage boundary"
    );
    assert!(
        !enclosed_spoiled,
        "material-backed preservation must keep the matched witness edible over the same interval"
    );
    assert!(enclosed_age_after_ticks < shelf_life_ticks);

    let survival_before_dismantle = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("preservation dismantling player disappeared before service"));
    let dismantle = validate_start_storage_enclosure_dismantling(
        registries,
        &state,
        enclosed_food,
        dismantle_recovery,
    )
    .unwrap_or_else(|error| panic!("preservation enclosure dismantling failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("preservation enclosure dismantling commit failed: {error}"));
    let dismantle_ticks = dismantle
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("preservation dismantling completion precedes start"));
    assert_eq!(dismantle_ticks, definition.dismantle_duration().value());
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::StorageEnclosureDismantling { .. })
    ));
    let mut dismantle_outcome = None;
    for elapsed in 1..=dismantle_ticks {
        let outcome = advance_tick(registries, &mut state)
            .unwrap_or_else(|error| panic!("preservation dismantling tick failed: {error}"));
        if elapsed < dismantle_ticks {
            assert!(
                state
                    .inventory()
                    .get_stockpile(enclosed_food)
                    .and_then(|record| record.enclosure())
                    .is_some(),
                "storage enclosure must remain installed until dismantling completes"
            );
            assert!(outcome.storage_enclosure_dismantling().is_none());
        } else {
            dismantle_outcome = outcome.storage_enclosure_dismantling().cloned();
        }
    }
    let dismantle_outcome = dismantle_outcome
        .unwrap_or_else(|| panic!("preservation dismantling reached due tick without completion"));
    assert_eq!(dismantle_outcome.target(), enclosed_food);
    assert_eq!(dismantle_outcome.definition(), definition.id());
    assert_eq!(state.player_work().active(), None);
    let target_after = state
        .inventory()
        .get_stockpile(enclosed_food)
        .unwrap_or_else(|| panic!("preservation target disappeared after dismantling"));
    assert!(target_after.enclosure().is_none());
    assert_eq!(
        target_after.storage_profile(),
        StockpileStorageProfile::unbounded_solid_only()
    );
    let recovered_enclosure_mass_mg = dismantle_outcome
        .recovered_lots()
        .iter()
        .map(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .unwrap_or_else(|| panic!("recovered enclosure lot disappeared"))
                .mass()
                .milligrams()
        })
        .sum::<u64>();
    assert_eq!(
        recovered_enclosure_mass_mg,
        definition.assembly_profile().input_mass().milligrams(),
        "dismantling must return exactly the enclosure's embodied matter"
    );
    let survival_after_dismantle = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("preservation dismantling player disappeared after service"));
    let dismantle_metabolic_cost_nj = survival_before_dismantle
        .metabolic_energy()
        .checked_sub(survival_after_dismantle.metabolic_energy())
        .unwrap_or_else(|| unreachable!("storage dismantling cannot create metabolic reserve"))
        .nanojoules();
    let dismantle_hydration_cost_ul = survival_before_dismantle
        .hydration()
        .checked_sub(survival_after_dismantle.hydration())
        .unwrap_or_else(|| unreachable!("storage dismantling cannot create hydration reserve"))
        .microliters();
    assert!(dismantle_metabolic_cost_nj > 0);
    assert!(dismantle_hydration_cost_ul > 0);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("preservation dismantling matter audit failed: {error}"))
            .total(),
        matter_before,
        "dismantling preservation storage must conserve represented matter"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("preservation infrastructure state audit failed: {error}"));

    PreservationInfrastructureReview {
        selection_kind,
        candidate_count: candidates.len(),
        storage_definition: definition.id(),
        fastest_definition: fastest.definition,
        fastest_ticks: fastest.construction_plan.attention_ticks,
        fastest_preservation_multiplier_ppm: fastest.preservation_multiplier_ppm,
        strongest_definition: strongest.definition,
        strongest_ticks: strongest.construction_plan.attention_ticks,
        strongest_preservation_multiplier_ppm: strongest.preservation_multiplier_ppm,
        food_commodity: food.commodity(),
        construction_stages,
        production_ticks: construction_ticks,
        observation_ticks,
        raw_material_mass_mg: construction_plan.raw_mass.milligrams(),
        embodied_mass_mg: definition.assembly_profile().input_mass().milligrams(),
        residual_mass_mg: construction_plan
            .raw_mass
            .checked_sub(definition.assembly_profile().input_mass())
            .unwrap_or_else(|| {
                panic!("preservation body cannot embody more matter than its raw chain")
            })
            .milligrams(),
        capacity_mass_mg: definition.maximum_stockpile_capacity().milligrams(),
        preservation_multiplier_ppm: definition.storage_profile().preservation_multiplier_ppm(),
        bootstrap_age_ticks,
        ambient_age_after_ticks,
        enclosed_age_after_ticks,
        enclosed_remaining_fresh_ticks,
        age_saved_ticks,
        ambient_spoiled,
        enclosed_fresh: !enclosed_spoiled,
        metabolic_cost_nj,
        hydration_cost_ul,
        dismantle_ticks,
        dismantle_metabolic_cost_nj,
        dismantle_hydration_cost_ul,
        recovered_enclosure_mass_mg,
    }
}

fn freshness_age(registries: &Registries, state: &AppState, lot: MaterialLotId) -> (bool, u64) {
    match assess_food_freshness(registries, state, lot)
        .unwrap_or_else(|error| panic!("survival probe freshness projection failed: {error:?}"))
    {
        FoodFreshness::Fresh { age, remaining: _ } => (false, age.value()),
        FoodFreshness::Spoiled { age } => (true, age.value()),
    }
}
