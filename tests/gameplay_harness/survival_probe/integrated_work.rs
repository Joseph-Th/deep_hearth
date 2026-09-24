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
    pub(super) power_triggered_by_observation: bool,
    pub(super) reprovisioned_after_prospecting: bool,
    pub(super) reprovision_volume_ul: u64,
    pub(super) reprovision_ticks: u64,
    pub(super) manual_power_ticks: u64,
    pub(super) stored_work_nj: u128,
    pub(super) energy_deficit_ppm: u32,
    pub(super) hydration_deficit_ppm: u32,
    pub(super) hydration_warning_safe: bool,
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
    let mut state = AppState::new();
    let drink_store = seed_fluid_store(
        registries,
        &mut state,
        maximum_drink_volume
            .checked_add(maximum_drink_volume)
            .unwrap_or_else(|| panic!("integrated survival drink capacity overflowed")),
        drink.fluid(),
        maximum_drink_volume
            .checked_add(maximum_drink_volume)
            .unwrap_or_else(|| panic!("integrated survival drink supply overflowed")),
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
                MaterialComposition::pure(MATERIAL_COPPER),
            ),
        );
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
    let initial_target = match hydration_policy {
        WorkHydrationPolicy::TaskFloor => prospecting_hydration_floor,
        WorkHydrationPolicy::WorkingReserve => {
            std::cmp::max(prospecting_hydration_floor, working_reserve_target)
        }
    };
    let initial_drink_projection = project_minimum_drink_to_hydration_target(
        physiology,
        drink,
        start.hydration(),
        initial_target,
    )
    .unwrap_or_else(|error| panic!("integrated survival initial drink projection failed: {error}"))
    .unwrap_or_else(|| panic!("integrated survival initial work requires a drink"));
    let initial_drink_volume = initial_drink_projection.volume();
    let first_drink = validate_drink(registries, &state, drink_store, initial_drink_volume)
        .unwrap_or_else(|error| panic!("integrated survival initial drink failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival initial drink commit failed: {error}"));
    let initial_drink_ticks =
        finish_direct_consumption(registries, &mut state, first_drink.completes_at());
    assert_eq!(
        initial_drink_ticks,
        initial_drink_projection.duration().value(),
        "initial working-reserve drink execution must match the owner projection"
    );
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
    let power_triggered_by_observation = finding.lower_ppm() > 0;
    assert_eq!(
        power_triggered_by_observation, opportunity_present,
        "integrated survival actor-visible finding must distinguish the disclosed opportunity"
    );

    let requested_energy = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("integrated survival flywheel definition disappeared"));
    let power_request =
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested_energy);
    let after_prospecting = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after prospecting"));
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
    let manual_power_floor = power_projection.map(|projection| {
        physiology
            .thirsty_below()
            .checked_add(projection.resource_budget().hydration())
            .unwrap_or_else(|| {
                panic!("integrated survival manual-power hydration target overflowed")
            })
    });
    let reprovisioned_after_prospecting =
        manual_power_floor.is_some_and(|floor| after_prospecting.hydration() < floor);
    let reprovision_projection = if reprovisioned_after_prospecting {
        let manual_power_floor = manual_power_floor
            .unwrap_or_else(|| unreachable!("reprovision requires a manual-power floor"));
        let target = match hydration_policy {
            WorkHydrationPolicy::TaskFloor => manual_power_floor,
            WorkHydrationPolicy::WorkingReserve => {
                std::cmp::max(manual_power_floor, working_reserve_target)
            }
        };
        Some(
            project_minimum_drink_to_hydration_target(
                physiology,
                drink,
                after_prospecting.hydration(),
                target,
            )
            .unwrap_or_else(|error| {
                panic!("integrated survival follow-up drink projection failed: {error}")
            })
            .unwrap_or_else(|| panic!("integrated survival follow-up work requires a drink")),
        )
    } else {
        None
    };
    let reprovision_volume =
        reprovision_projection.map_or(Volume::ZERO, |projection| projection.volume());
    let reprovision_ticks = if reprovisioned_after_prospecting {
        let drink = validate_drink(registries, &state, drink_store, reprovision_volume)
            .unwrap_or_else(|error| panic!("integrated survival follow-up drink failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("integrated survival follow-up drink commit failed: {error}")
            });
        let ticks = finish_direct_consumption(registries, &mut state, drink.completes_at());
        assert_eq!(
            Some(ticks),
            reprovision_projection.map(|projection| projection.duration().value()),
            "follow-up task-sized drink execution must match the owner projection"
        );
        ticks
    } else {
        0
    };
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
        initial_drink_volume_ul: initial_drink_volume.microliters(),
        initial_drink_ticks,
        prospecting_ticks,
        power_triggered_by_observation,
        reprovisioned_after_prospecting,
        reprovision_volume_ul: reprovision_volume.microliters(),
        reprovision_ticks,
        manual_power_ticks,
        stored_work_nj: stored_work.nanojoules(),
        energy_deficit_ppm,
        hydration_deficit_ppm,
        hydration_warning_safe: final_survival.hydration() >= physiology.thirsty_below(),
    }
}
