//! Matched survival pressure from prospecting and manual power work.

use super::super::prospecting_timing::complete_prospecting_work;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SurvivalWorkPressureReview {
    pub(super) prospecting_method: ProspectingMethodId,
    pub(super) prospecting_region_voxels: u128,
    pub(super) prospecting_ticks: u64,
    pub(super) prospecting_energy_deficit_ppm: u32,
    pub(super) prospecting_hydration_deficit_ppm: u32,
    pub(super) manual_power_ticks: u64,
    pub(super) manual_power_energy_deficit_ppm: u32,
    pub(super) manual_power_hydration_deficit_ppm: u32,
    pub(super) stored_work_nj: u128,
}

pub(in super::super) fn prospecting_method_for_work_pressure(
    registries: &Registries,
    seed: u64,
) -> ProspectingMethodId {
    let mut methods = registries
        .labor()
        .prospecting_definitions()
        .filter(|definition| definition.equipment().is_none())
        .map(|definition| definition.id())
        .collect::<Vec<_>>();
    methods.sort_unstable();
    assert!(
        !methods.is_empty(),
        "survival work-pressure probe requires one authored prospecting method"
    );
    let index = usize::try_from(mix64(seed ^ 0x5052_4F53_4D45_5448) % methods.len() as u64)
        .unwrap_or_else(|_| unreachable!("prospecting method index fits usize"));
    methods[index]
}

pub(super) fn evaluate_survival_work_pressure_probe(
    registries: &Registries,
    seed: u64,
) -> SurvivalWorkPressureReview {
    let physiology = registries.survival().physiology();

    let mut prospecting = AppState::new();
    initialize_player_survival(registries, &mut prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting survival setup failed: {error}"));
    let prospecting_method = prospecting_method_for_work_pressure(registries, seed);
    let prospecting_definition = registries
        .labor()
        .get_prospecting(prospecting_method)
        .copied()
        .unwrap_or_else(|| panic!("selected work-pressure prospecting method disappeared"));
    let region_width = i64::try_from(prospecting_definition.maximum_region_voxels().min(4))
        .unwrap_or_else(|_| unreachable!("bounded prospecting footprint fits i64"));
    let region = VoxelBounds::new(
        VoxelCoord::new(24, -1, 0),
        VoxelCoord::new(24 + region_width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("work-pressure prospecting bounds failed: {error}"));
    let prospecting_region_voxels = region
        .voxel_count()
        .unwrap_or_else(|| panic!("work-pressure prospecting region volume overflowed"));
    let prospecting_before = assess_survival(registries, &prospecting)
        .unwrap_or_else(|| panic!("work-pressure prospecting player disappeared"));
    let prospecting_start = validate_start_field_prospecting(
        registries,
        &prospecting,
        FieldProspectingRequest::new(prospecting_method, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("work-pressure prospecting start failed: {error}"));
    let prospecting_work = prospecting_start.work();
    let prospecting_ticks = prospecting_work
        .completes_at()
        .value()
        .checked_sub(prospecting_work.started_at().value())
        .unwrap_or_else(|| unreachable!("validated prospecting completes after it starts"));
    prospecting_start
        .commit(&mut prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting commit failed: {error}"));
    let prospecting_completion = complete_prospecting_work(
        registries,
        &mut prospecting,
        prospecting_work,
        "work-pressure prospecting",
    );
    assert_eq!(prospecting_completion.method(), prospecting_method);
    assert_eq!(prospecting_completion.region(), region);
    assert_eq!(prospecting_completion.material(), MATERIAL_COPPER);
    assert!(
        prospecting
            .geological_knowledge()
            .get_observation(prospecting_completion.observation())
            .is_some(),
        "work-pressure prospecting completion must identify its persisted observation"
    );
    let prospecting_after = assess_survival(registries, &prospecting)
        .unwrap_or_else(|| panic!("work-pressure prospecting player disappeared after work"));
    let prospecting_energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        prospecting_after.metabolic_energy(),
    );
    let prospecting_hydration_deficit_ppm = normalized_hydration_deficit_ppm(
        physiology.maximum_hydration(),
        prospecting_after.hydration(),
    );
    assert_eq!(
        prospecting_before.metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(
        prospecting_before.hydration(),
        physiology.maximum_hydration()
    );
    assert!(prospecting_energy_deficit_ppm > 0);
    assert!(prospecting_hydration_deficit_ppm > 0);

    let mut power = AppState::new();
    let crank_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_HAND_CRANK)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("work-pressure stone crank lost its assembly route"));
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("work-pressure stone flywheel lost its assembly route"));
    let component_capacity = crank_profile
        .inputs()
        .iter()
        .chain(drive_profile.inputs())
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("work-pressure primitive power component mass overflowed"));
    let component_source = seed_stockpile(
        &mut power,
        component_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in crank_profile.inputs().iter().chain(drive_profile.inputs()) {
        seed_lot(
            registries,
            &mut power,
            component_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let crank = validate_assemble_equipment(
        registries,
        &power,
        EQUIPMENT_STONE_HAND_CRANK,
        component_source,
    )
    .unwrap_or_else(|error| panic!("work-pressure stone crank assembly failed: {error}"))
    .commit(&mut power)
    .unwrap_or_else(|error| panic!("work-pressure stone crank assembly commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        registries,
        &power,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        component_source,
    )
    .unwrap_or_else(|error| panic!("work-pressure stone flywheel assembly failed: {error}"))
    .commit(&mut power)
    .unwrap_or_else(|error| panic!("work-pressure stone flywheel assembly commit failed: {error}"));
    initialize_player_survival(registries, &mut power).unwrap_or_else(|error| {
        panic!("work-pressure manual-power survival setup failed: {error}")
    });
    let requested_energy = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("work-pressure stone flywheel definition disappeared"));
    let power_before = assess_survival(registries, &power)
        .unwrap_or_else(|| panic!("work-pressure manual-power player disappeared"));
    let power_start = validate_start_manual_power(
        registries,
        &power,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested_energy),
    )
    .unwrap_or_else(|error| panic!("work-pressure manual-power start failed: {error}"));
    let power_work = power_start.work();
    let power_ticks = power_work
        .completes_at()
        .value()
        .checked_sub(power_work.started_at().value())
        .unwrap_or_else(|| unreachable!("validated manual power completes after it starts"));
    power_start
        .commit(&mut power)
        .unwrap_or_else(|error| panic!("work-pressure manual-power commit failed: {error}"));
    assert_eq!(
        finish_manual_power_work(
            registries,
            &mut power,
            power_work,
            "work-pressure manual power"
        ),
        power_ticks
    );
    assert_eq!(
        power.energy().get_store(drive).map(|store| store.stored()),
        Some(requested_energy),
        "matched work-pressure manual labor must create the requested finite stored work"
    );
    let power_after = assess_survival(registries, &power)
        .unwrap_or_else(|| panic!("work-pressure manual-power player disappeared after work"));
    let power_energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        power_after.metabolic_energy(),
    );
    let power_hydration_deficit_ppm =
        normalized_hydration_deficit_ppm(physiology.maximum_hydration(), power_after.hydration());
    assert_eq!(
        power_before.metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(power_before.hydration(), physiology.maximum_hydration());
    assert!(power_energy_deficit_ppm > 0);
    assert!(power_hydration_deficit_ppm > 0);

    let prospecting_priority = normalized_deficit_priority(
        prospecting_energy_deficit_ppm,
        prospecting_hydration_deficit_ppm,
    );
    let power_priority =
        normalized_deficit_priority(power_energy_deficit_ppm, power_hydration_deficit_ppm);
    let activity_changes_dominant_pressure = prospecting_priority != power_priority;

    validate_loaded_state(registries, &prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting state audit failed: {error}"));
    validate_loaded_state(registries, &power)
        .unwrap_or_else(|error| panic!("work-pressure manual-power state audit failed: {error}"));
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        reviewln!(
            "SURVIVAL WORK PRESSURE seed=0x{seed:016X} matched-full-reserve-work=[prospecting:[method:{} region:{}vox {}t energy:{}ppm hydration:{}ppm dominant:{}] manual-power:[{}t energy:{}ppm hydration:{}ppm dominant:{} stored-work:{}nJ]] activity-changes-dominant-pressure:{} canonical-actions=true",
            prospecting_method.value(),
            prospecting_region_voxels,
            prospecting_ticks,
            prospecting_energy_deficit_ppm,
            prospecting_hydration_deficit_ppm,
            prospecting_priority.label(),
            power_ticks,
            power_energy_deficit_ppm,
            power_hydration_deficit_ppm,
            power_priority.label(),
            requested_energy.nanojoules(),
            activity_changes_dominant_pressure,
        );
    }
    SurvivalWorkPressureReview {
        prospecting_method,
        prospecting_region_voxels,
        prospecting_ticks,
        prospecting_energy_deficit_ppm,
        prospecting_hydration_deficit_ppm,
        manual_power_ticks: power_ticks,
        manual_power_energy_deficit_ppm: power_energy_deficit_ppm,
        manual_power_hydration_deficit_ppm: power_hydration_deficit_ppm,
        stored_work_nj: requested_energy.nanojoules(),
    }
}
