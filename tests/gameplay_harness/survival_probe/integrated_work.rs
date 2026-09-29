//! Integrated provision, prospect, reprovision, and manual-power survival loop.

use super::super::prospecting_timing::complete_prospecting_work;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkHydrationPolicy {
    TaskFloor,
    WorkingReserve,
}

impl WorkHydrationPolicy {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::TaskFloor => "task-floor",
            Self::WorkingReserve => "working-reserve",
        }
    }

    fn for_behavior_seed(behavior_seed: u64) -> Self {
        if behavior_seed & 0b10 == 0 {
            Self::TaskFloor
        } else {
            Self::WorkingReserve
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IntegratedSurvivalWorkReview {
    pub(super) hydration_policy: WorkHydrationPolicy,
    pub(super) initial_drink_volume_ul: u64,
    pub(super) initial_drink_ticks: u64,
    pub(super) prospecting_ticks: u64,
    pub(super) followup_prospecting_triggered: bool,
    pub(super) followup_prospecting_ticks: u64,
    pub(super) followup_found_continuation: bool,
    pub(super) power_triggered_by_observation: bool,
    pub(super) followup_reprovisioned: bool,
    pub(super) followup_reprovision_volume_ul: u64,
    pub(super) followup_reprovision_ticks: u64,
    pub(super) power_reprovisioned: bool,
    pub(super) power_reprovision_volume_ul: u64,
    pub(super) power_reprovision_ticks: u64,
    pub(super) manual_power_ticks: u64,
    pub(super) stored_work_nj: u128,
    pub(super) energy_deficit_ppm: u32,
    pub(super) hydration_deficit_ppm: u32,
    pub(super) hydration_warning_safe: bool,
}

fn hydration_target_for_policy(
    policy: WorkHydrationPolicy,
    task_floor: Volume,
    working_reserve_target: Volume,
) -> Volume {
    match policy {
        WorkHydrationPolicy::TaskFloor => task_floor,
        WorkHydrationPolicy::WorkingReserve => std::cmp::max(task_floor, working_reserve_target),
    }
}

fn provision_hydration_if_needed(
    registries: &Registries,
    state: &mut AppState,
    drink_store: FluidStoreId,
    target: Volume,
    context: &'static str,
) -> (bool, u64, Volume) {
    let current = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared before {context}"));
    if current.hydration() >= target {
        return (false, 0, Volume::ZERO);
    }
    let drink = validate_drink_store_to_hydration_target(registries, state, drink_store, target)
        .unwrap_or_else(|error| panic!("integrated survival {context} drink failed: {error}"))
        .unwrap_or_else(|| panic!("integrated survival {context} requires a drink"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("integrated survival {context} drink commit failed: {error}")
        });
    let volume = drink.volume();
    (
        true,
        finish_direct_consumption(registries, state, drink.completes_at()),
        volume,
    )
}

pub(super) fn evaluate_integrated_survival_work_loop(
    registries: &Registries,
    seed: u64,
    behavior_seed: u64,
) -> IntegratedSurvivalWorkReview {
    let physiology = registries.survival().physiology();
    let direct = physiology.direct_consumption();
    let mut drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    drinks.sort_by_key(|drink| drink.fluid());
    let drink = drinks
        .get(
            usize::try_from(mix64(seed ^ 0x494E_5445_4752_4452) % drinks.len().max(1) as u64)
                .unwrap_or_else(|_| unreachable!("integrated survival drink index fits usize")),
        )
        .copied()
        .unwrap_or_else(|| panic!("integrated survival work loop requires one authored drink"));
    let maximum_drink_volume = direct.maximum_drink_volume();
    let integrated_drink_supply = maximum_drink_volume
        .checked_add(maximum_drink_volume)
        .and_then(|volume| volume.checked_add(maximum_drink_volume))
        .unwrap_or_else(|| panic!("integrated survival drink supply overflowed"));
    let mut state = AppState::new();
    let drink_store = seed_fluid_store(
        registries,
        &mut state,
        integrated_drink_supply,
        drink.fluid(),
        integrated_drink_supply,
        ROOM_TEMPERATURE,
    );

    let crank_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_HAND_CRANK)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("integrated survival stone crank lost its assembly route"));
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("integrated survival stone flywheel lost its assembly route"));
    let component_capacity = crank_profile
        .inputs()
        .iter()
        .chain(drive_profile.inputs())
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("integrated survival primitive power component mass overflowed"));
    let component_source = seed_stockpile(
        &mut state,
        component_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in crank_profile.inputs().iter().chain(drive_profile.inputs()) {
        seed_lot(
            registries,
            &mut state,
            component_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let crank = validate_assemble_equipment(
        registries,
        &state,
        EQUIPMENT_STONE_HAND_CRANK,
        component_source,
    )
    .unwrap_or_else(|error| panic!("integrated survival crank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("integrated survival crank assembly commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        registries,
        &state,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        component_source,
    )
    .unwrap_or_else(|error| panic!("integrated survival flywheel assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("integrated survival flywheel assembly commit failed: {error}"));
    let prospecting_method = prospecting_method_for_work_pressure(registries, seed);
    let prospecting_definition = registries
        .labor()
        .get_prospecting(prospecting_method)
        .copied()
        .unwrap_or_else(|| panic!("integrated survival prospecting method disappeared"));
    let region_width = i64::try_from(prospecting_definition.maximum_region_voxels().min(4))
        .unwrap_or_else(|_| unreachable!("bounded integrated prospecting footprint fits i64"));
    let region = VoxelBounds::new(
        VoxelCoord::new(40, -1, 0),
        VoxelCoord::new(40 + region_width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("integrated survival prospecting bounds failed: {error}"));
    let followup_definition = registries
        .labor()
        .get_prospecting(PROSPECTING_REGIONAL_RECONNAISSANCE)
        .copied()
        .unwrap_or_else(|| {
            panic!("integrated survival regional follow-up prospecting method disappeared")
        });
    let followup_width = i64::try_from(followup_definition.maximum_region_voxels().min(4))
        .unwrap_or_else(|_| {
            unreachable!("bounded integrated follow-up prospecting footprint fits i64")
        });
    let followup_region = VoxelBounds::new(
        VoxelCoord::new(40 + region_width, -1, 0),
        VoxelCoord::new(40 + region_width + followup_width, 0, 1),
    )
    .unwrap_or_else(|error| {
        panic!("integrated survival follow-up prospecting bounds failed: {error}")
    });
    let primary_copper_ppm = 450_000 + (mix64(seed ^ 0x494E_5445_4750_4355) % 300_001) as u32;
    let primary_clay_share_ppm = 100_000 + (mix64(seed ^ 0x494E_5445_4750_434C) % 800_001) as u32;
    let followup_copper_ppm = 450_000 + (mix64(seed ^ 0x494E_5445_4746_4355) % 300_001) as u32;
    let followup_clay_share_ppm = 100_000 + (mix64(seed ^ 0x494E_5445_4746_434C) % 800_001) as u32;
    let opportunity_present = mix64(seed ^ 0x494E_5445_4752_4F50) & 1 == 0;
    if opportunity_present {
        seed_geological_deposit(
            registries,
            &mut state,
            GeologicalDepositSeed::new(
                region,
                CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
                Mass::from_milligrams(4_000_000),
                ROOM_TEMPERATURE,
                Pressure::from_pascals(350_000_000),
                super::super::ore_fixture::copper_ore_composition(
                    primary_copper_ppm,
                    primary_clay_share_ppm,
                ),
            ),
        );
        if mix64(seed ^ 0x494E_5445_4743_4F4E) & 1 == 0 {
            seed_geological_deposit(
                registries,
                &mut state,
                GeologicalDepositSeed::new(
                    followup_region,
                    CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
                    Mass::from_milligrams(8_000_000),
                    ROOM_TEMPERATURE,
                    Pressure::from_pascals(350_000_000),
                    super::super::ore_fixture::copper_ore_composition(
                        followup_copper_ppm,
                        followup_clay_share_ppm,
                    ),
                ),
            );
        }
    }
    seed_player_survival_at_hydration_warning_boundary(registries, &mut state);

    let start = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared at start"));
    assert_eq!(start.hydration(), physiology.thirsty_below());
    let prospecting_request =
        FieldProspectingRequest::new(prospecting_method, region, MATERIAL_COPPER);
    let prospecting_projection = project_prospecting_work(registries, prospecting_method, region)
        .unwrap_or_else(|error| {
            panic!("integrated survival prospecting work projection failed: {error}")
        });
    let prospecting_budget = prospecting_projection.resource_budget();
    let prospecting_hydration_floor = physiology
        .thirsty_below()
        .checked_add(prospecting_budget.hydration())
        .unwrap_or_else(|| panic!("integrated survival prospecting hydration target overflowed"));
    let hydration_policy = WorkHydrationPolicy::for_behavior_seed(behavior_seed);
    // Both policies respect the authoritative task floor. Working-reserve actors pay a larger
    // up-front drink to reduce interruption risk; task-floor actors carry only the known
    // prospecting requirement and reassess after observing whether follow-up work exists.
    let working_reserve_target =
        Volume::from_microliters(physiology.maximum_hydration().microliters() / 2);
    let initial_target = hydration_target_for_policy(
        hydration_policy,
        prospecting_hydration_floor,
        working_reserve_target,
    );
    let first_drink =
        validate_drink_store_to_hydration_target(registries, &state, drink_store, initial_target)
            .unwrap_or_else(|error| panic!("integrated survival initial drink failed: {error}"))
            .unwrap_or_else(|| panic!("integrated survival initial work requires a drink"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("integrated survival initial drink commit failed: {error}")
            });
    let initial_drink_ticks =
        finish_direct_consumption(registries, &mut state, first_drink.completes_at());
    let after_drink = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after drinking"));
    assert!(
        after_drink.hydration() >= initial_target,
        "initial drink did not establish the planned working hydration reserve"
    );

    let prospecting = validate_start_field_prospecting(registries, &state, prospecting_request)
        .unwrap_or_else(|error| panic!("integrated survival prospecting start failed: {error}"));
    let prospecting_work = prospecting.work();
    let prospecting_ticks = prospecting_work
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| unreachable!("validated prospecting completes after it starts"));
    assert_eq!(
        prospecting_ticks,
        prospecting_projection.duration().value(),
        "prospecting admission duration must match its pre-admission work projection"
    );
    prospecting
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival prospecting commit failed: {error}"));
    let observation = complete_prospecting_work(
        registries,
        &mut state,
        prospecting_work,
        "integrated survival prospecting",
    );
    let finding = state
        .geological_knowledge()
        .get_observation(observation.observation())
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("integrated survival prospecting finding disappeared"));
    let followup_prospecting_triggered = finding.lower_ppm() > 0;
    assert_eq!(
        followup_prospecting_triggered, opportunity_present,
        "integrated survival actor-visible finding must distinguish the disclosed opportunity"
    );

    let after_prospecting = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after prospecting"));
    let followup_projection = followup_prospecting_triggered.then(|| {
        project_prospecting_work(
            registries,
            PROSPECTING_REGIONAL_RECONNAISSANCE,
            followup_region,
        )
        .unwrap_or_else(|error| {
            panic!("integrated survival follow-up prospecting projection failed: {error}")
        })
    });
    let followup_work_floor = followup_projection.map(|projection| {
        let followup_hydration = projection.resource_budget().hydration();
        physiology
            .thirsty_below()
            .checked_add(followup_hydration)
            .unwrap_or_else(|| {
                panic!("integrated survival follow-up work hydration target overflowed")
            })
    });
    let followup_target = followup_work_floor
        .map(|floor| hydration_target_for_policy(hydration_policy, floor, working_reserve_target));
    let (followup_reprovisioned, followup_reprovision_ticks, followup_reprovision_volume) =
        followup_target.map_or((false, 0, Volume::ZERO), |target| {
            provision_hydration_if_needed(
                registries,
                &mut state,
                drink_store,
                target,
                "follow-up prospecting",
            )
        });
    debug_assert_eq!(
        after_prospecting.hydration() < followup_target.unwrap_or(Volume::ZERO),
        followup_reprovisioned
    );
    let (followup_prospecting_ticks, followup_found_continuation) =
        if followup_prospecting_triggered {
            let followup_request = FieldProspectingRequest::new(
                PROSPECTING_REGIONAL_RECONNAISSANCE,
                followup_region,
                MATERIAL_COPPER,
            );
            let followup = validate_start_field_prospecting(registries, &state, followup_request)
                .unwrap_or_else(|error| {
                    panic!("integrated survival follow-up prospecting start failed: {error}")
                });
            let work = followup.work();
            let ticks = work
                .completes_at()
                .value()
                .checked_sub(state.tick().value())
                .unwrap_or_else(|| {
                    unreachable!("validated follow-up prospecting completes after it starts")
                });
            assert_eq!(
                Some(ticks),
                followup_projection.map(|projection| projection.duration().value()),
                "follow-up prospecting admission duration must match its projection"
            );
            followup.commit(&mut state).unwrap_or_else(|error| {
                panic!("integrated survival follow-up prospecting commit failed: {error}")
            });
            let outcome = complete_prospecting_work(
                registries,
                &mut state,
                work,
                "integrated survival neighboring reconnaissance",
            );
            let continuation = state
                .geological_knowledge()
                .get_observation(outcome.observation())
                .and_then(|record| record.finding(MATERIAL_COPPER))
                .is_some_and(|finding| finding.lower_ppm() > 0);
            (ticks, continuation)
        } else {
            (0, false)
        };
    let power_triggered_by_observation = followup_found_continuation;
    let requested_energy = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("integrated survival flywheel definition disappeared"));
    let power_request =
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested_energy);
    let power_projection = power_triggered_by_observation.then(|| {
        let crank_record = state
            .equipment()
            .get_equipment(crank)
            .unwrap_or_else(|| panic!("integrated survival hand crank disappeared"));
        project_manual_power(
            registries,
            MANUAL_POWER_HAND_CRANK,
            crank_record.definition(),
            crank_record.condition(),
            ENERGY_STONE_FLYWHEEL_DRIVE,
            requested_energy,
        )
        .unwrap_or_else(|error| {
            panic!("integrated survival manual-power projection failed: {error}")
        })
    });
    let power_target = power_projection.map(|projection| {
        let floor = physiology
            .thirsty_below()
            .checked_add(projection.resource_budget().hydration())
            .unwrap_or_else(|| panic!("integrated survival power hydration target overflowed"));
        hydration_target_for_policy(hydration_policy, floor, working_reserve_target)
    });
    let (power_reprovisioned, power_reprovision_ticks, power_reprovision_volume) = power_target
        .map_or((false, 0, Volume::ZERO), |target| {
            provision_hydration_if_needed(
                registries,
                &mut state,
                drink_store,
                target,
                "observed power opportunity",
            )
        });
    let manual_power_ticks = if power_triggered_by_observation {
        let power = validate_start_manual_power(registries, &state, power_request).unwrap_or_else(
            |error| panic!("integrated survival manual-power start failed: {error}"),
        );
        assert_eq!(
            Some(power.resource_budget()),
            power_projection.map(|projection| projection.resource_budget()),
            "manual-power admission budget must match its pre-admission projection"
        );
        let work = power.work();
        let ticks = work
            .completes_at()
            .value()
            .checked_sub(state.tick().value())
            .unwrap_or_else(|| unreachable!("validated manual power completes after it starts"));
        assert_eq!(
            Some(ticks),
            power_projection.map(|projection| projection.duration().value()),
            "manual-power admission duration must match its pre-admission projection"
        );
        power.commit(&mut state).unwrap_or_else(|error| {
            panic!("integrated survival manual-power commit failed: {error}")
        });
        assert_eq!(
            finish_manual_power_work(
                registries,
                &mut state,
                work,
                "integrated survival manual power"
            ),
            ticks
        );
        ticks
    } else {
        0
    };
    let stored_work = state
        .energy()
        .get_store(drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("integrated survival flywheel disappeared"));
    assert_eq!(
        stored_work,
        if power_triggered_by_observation {
            requested_energy
        } else {
            Energy::ZERO
        },
        "integrated survival stored work must follow the actor-visible prospecting decision"
    );
    let final_survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after work loop"));
    let energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        final_survival.metabolic_energy(),
    );
    let hydration_deficit_ppm = normalized_hydration_deficit_ppm(
        physiology.maximum_hydration(),
        final_survival.hydration(),
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("integrated survival work-loop audit failed: {error}"));

    IntegratedSurvivalWorkReview {
        hydration_policy,
        initial_drink_volume_ul: first_drink.volume().microliters(),
        initial_drink_ticks,
        prospecting_ticks,
        followup_prospecting_triggered,
        followup_prospecting_ticks,
        followup_found_continuation,
        power_triggered_by_observation,
        followup_reprovisioned,
        followup_reprovision_volume_ul: followup_reprovision_volume.microliters(),
        followup_reprovision_ticks,
        power_reprovisioned,
        power_reprovision_volume_ul: power_reprovision_volume.microliters(),
        power_reprovision_ticks,
        manual_power_ticks,
        stored_work_nj: stored_work.nanojoules(),
        energy_deficit_ppm,
        hydration_deficit_ppm,
        hydration_warning_safe: final_survival.hydration() >= physiology.thirsty_below(),
    }
}
