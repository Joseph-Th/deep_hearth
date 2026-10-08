//! Executed raw-material-to-kit fabrication witness for the reusable primitive liberation kit.
//!
//! Raw stone and fallen timber are generated only by the disclosed controlled pre-admission fixture.
//! After admission the player discovers them through exact-local observation and gathers them through
//! canonical same-voxel labor into finite carried custody; all subsequent work uses runtime boundaries.

use std::collections::BTreeMap;

use deep_hearth::content::gameplay_fixture::{
    seed_assembled_energy_store_at, seed_assembled_equipment_at, seed_lot,
    seed_preused_assembled_equipment_at, seed_surface_resource,
};
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_STONE_WOODWORKING_ADZE,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, FORM_BOARD, FORM_LOG, FORM_LUMP,
    FORM_TIMBER_RIDDLE_PANEL, MATERIAL_STONE, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::PlayerWork;
use deep_hearth::logistics::{
    assess_player_carrying, validate_allocate_ground_stockpile,
    validate_allocate_player_ground_stockpile, validate_initialize_player_logistics,
};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::surface::{
    SurfaceGatheringRequest, SurfaceResourceId, validate_start_surface_gathering,
};
use deep_hearth::survival::{assess_survival, initialize_player_survival};

use super::super::environment::ROOM_TEMPERATURE;
use super::super::exact_local_runtime::{unique_local_energy_store, unique_local_equipment};
use super::super::focused_case::FocusedProbeCase;
use super::super::manual_craft_batches::execute_manual_craft_batches;
use super::super::manual_craft_equipment_planning::manual_craft_plan_with_equipment;
use super::super::manual_craft_equipment_topology_planning::manual_craft_topology_plan_with_equipment;
use super::super::manual_craft_execution::execute_manual_craft;
use super::super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::super::manual_craft_selection::select_manual_craft_request;
use super::super::primitive_liberation_kit_planning::{
    RawKitMaterialPlan, add_profile_requirements, equipment_profile, gathering_method_for,
    project_incremental_kit_acquisition_attention, raw_kit_material_plan, raw_requirement_mass,
};
use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RawKitAcquisitionReview {
    pub(super) attention_ticks: u64,
    pub(super) metabolic_cost_nj: u128,
    pub(super) hydration_cost_ul: u64,
}

fn gather_surface_resource(
    registries: &Registries,
    state: &mut AppState,
    resource: SurfaceResourceId,
    destination: StockpileId,
    requested: Mass,
    context: &'static str,
) -> u64 {
    let commodity = state
        .available_surface_resources()
        .find(|candidate| candidate.id() == resource)
        .map(|candidate| candidate.commodity())
        .unwrap_or_else(|| panic!("liberation {context} local surface resource disappeared"));
    let method_id = gathering_method_for(commodity);
    let method = registries
        .labor()
        .get_surface_gathering(method_id)
        .copied()
        .unwrap_or_else(|| panic!("liberation {context} gathering method disappeared"));
    let started_at = state.tick().value();
    let mut remaining = requested;
    while !remaining.is_zero() {
        let batch = remaining.min(method.maximum_batch_mass());
        validate_start_surface_gathering(
            registries,
            state,
            SurfaceGatheringRequest::new(method_id, resource, destination, batch),
        )
        .unwrap_or_else(|error| panic!("liberation {context} gathering admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("liberation {context} gathering start failed: {error}"));
        let work = match state.player_work().active() {
            Some(PlayerWork::SurfaceGathering { work }) => work,
            other => panic!(
                "liberation {context} gathering start did not retain surface work: {other:?}"
            ),
        };
        assert_eq!(work.resource(), resource);
        assert_eq!(work.destination(), destination);
        assert_eq!(work.gathered_mass(), batch);
        let expected_ticks = work
            .completes_at()
            .value()
            .checked_sub(state.tick().value())
            .unwrap_or_else(|| panic!("liberation {context} gathering completion precedes start"));
        assert!(
            expected_ticks > 0,
            "liberation {context} gathering must occupy authoritative time"
        );
        for elapsed in 1..=expected_ticks {
            let outcome = advance_tick(registries, state).unwrap_or_else(|error| {
                panic!("liberation {context} gathering tick failed: {error}")
            });
            assert_tick_events_within(
                &outcome,
                TickEventAllowance {
                    surface_resources: &[resource],
                    ..TickEventAllowance::default()
                },
                context,
            );
            if elapsed < expected_ticks {
                assert!(
                    outcome.surface_gathering().is_none(),
                    "liberation {context} gathering completed before its admitted schedule"
                );
                assert_eq!(
                    state.player_work().active(),
                    Some(PlayerWork::SurfaceGathering { work }),
                    "liberation {context} lost gathering attention before completion"
                );
                continue;
            }
            let gathered = outcome.surface_gathering().unwrap_or_else(|| {
                panic!("liberation {context} gathering produced no completion receipt")
            });
            assert_eq!(gathered.resource(), resource);
            assert_eq!(gathered.destination(), destination);
            assert_eq!(gathered.gathered_mass(), batch);
            assert_eq!(state.player_work().active(), None);
        }
        remaining = remaining.checked_sub(batch).unwrap_or_else(|| {
            unreachable!("surface gathering batch is bounded by remaining demand")
        });
    }
    state
        .tick()
        .value()
        .checked_sub(started_at)
        .unwrap_or_else(|| unreachable!("surface gathering cannot run backward"))
}

pub(super) struct AcquiredPrimitiveKit {
    pub(super) decision_state: AppState,
    pub(super) build_state: AppState,
    pub(super) crusher: EquipmentId,
    pub(super) quern: EquipmentId,
    pub(super) screen: EquipmentId,
    pub(super) separator: EquipmentId,
    pub(super) power_provider: EquipmentId,
    pub(super) drive: deep_hearth::energy::EnergyStoreId,
    pub(super) review: RawKitAcquisitionReview,
}

#[derive(Clone, Copy)]
struct InheritedProgressionInfrastructure {
    crusher: EquipmentId,
    separator: EquipmentId,
    power_provider: EquipmentId,
    drive: deep_hearth::energy::EnergyStoreId,
    embodied_mass: Mass,
    minimum_condition_ppm: u32,
    maximum_condition_ppm: u32,
}

fn seed_inherited_progression_infrastructure(
    registries: &Registries,
    state: &mut AppState,
    case: FocusedProbeCase,
    position: deep_hearth::spatial::VoxelCoord,
) -> InheritedProgressionInfrastructure {
    let equipment = [
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    ];
    let mut requirements = BTreeMap::new();
    for definition in equipment {
        add_profile_requirements(&mut requirements, equipment_profile(registries, definition));
    }
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .and_then(|store| store.assembly_profile())
        .unwrap_or_else(|| panic!("liberation inherited flywheel lost authored assembly"));
    add_profile_requirements(&mut requirements, drive_profile);
    let embodied_mass = raw_requirement_mass(&requirements);
    let source = validate_allocate_ground_stockpile(state, position, embodied_mass)
        .unwrap_or_else(|error| panic!("liberation inherited component allocation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("liberation inherited component commit failed: {error}"));
    for (commodity, mass) in requirements {
        seed_lot(registries, state, source, commodity, mass, ROOM_TEMPERATURE);
    }
    let mut conditions = Vec::with_capacity(equipment.len());
    let mut assemble = |definition, salt| {
        let condition = super::inherited_progression_condition(registries, definition, case, salt);
        conditions.push(condition.parts_per_million());
        if condition == Condition::PRISTINE {
            seed_assembled_equipment_at(registries, state, definition, source, position)
        } else {
            seed_preused_assembled_equipment_at(
                registries, state, definition, source, position, condition,
            )
        }
    };
    let power_provider = assemble(
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        0x4352_414E_4B00_0001,
    );
    let crusher = assemble(
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        0x4352_5553_4800_0001,
    );
    let separator = assemble(
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        0x5345_5041_5241_544F,
    );
    let drive = seed_assembled_energy_store_at(
        registries,
        state,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        source,
        position,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "liberation inherited progression line must embody its complete disclosed component stock"
    );
    InheritedProgressionInfrastructure {
        crusher,
        separator,
        power_provider,
        drive,
        embodied_mass,
        minimum_condition_ppm: *conditions
            .iter()
            .min()
            .unwrap_or_else(|| unreachable!("inherited progression line has equipment")),
        maximum_condition_ppm: *conditions
            .iter()
            .max()
            .unwrap_or_else(|| unreachable!("inherited progression line has equipment")),
    }
}

struct ComponentCraftPlan {
    raw: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    required: Mass,
    equipment: Option<EquipmentId>,
    context: &'static str,
}

fn craft_component(registries: &Registries, state: &mut AppState, plan: ComponentCraftPlan) -> u64 {
    if let Some(equipment) = plan.equipment {
        let (definition, batches, source) = manual_craft_plan_with_equipment(
            registries,
            state,
            &[plan.raw],
            plan.commodity,
            plan.required,
            equipment,
            plan.context,
        );
        let request = select_manual_craft_request(
            registries,
            state,
            definition.process(),
            source,
            batches,
            plan.context,
        )
        .with_equipment(equipment);
        return execute_manual_craft(registries, state, request, plan.destination, plan.context)
            .value();
    }
    let (definition, batches, source) = manual_craft_plan_for_available_output(
        registries,
        state,
        &[plan.raw],
        plan.commodity,
        plan.required,
        plan.context,
    );
    execute_manual_craft_batches(
        registries,
        state,
        definition.process(),
        source,
        plan.destination,
        batches,
        plan.context,
    )
    .value()
}

fn craft_riddle_panel(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    panel_feed: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
    adze: EquipmentId,
    required_panel: Mass,
) -> u64 {
    let panel = CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL);
    let board = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let adze_record = state
        .equipment()
        .get_equipment(adze)
        .unwrap_or_else(|| panic!("liberation kit adze disappeared before panel planning"));
    let (definition, panel_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        panel,
        required_panel,
        &[board],
        adze_record.definition(),
        adze_record.condition(),
        "liberation kit riddle-panel feed planning",
    );
    let board_mass = Mass::from_milligrams(
        definition
            .input_mass()
            .milligrams()
            .checked_mul(panel_batches)
            .unwrap_or_else(|| panic!("liberation kit riddle-panel feed overflowed")),
    );
    let mut ticks = craft_component(
        registries,
        state,
        ComponentCraftPlan {
            raw,
            destination: panel_feed,
            commodity: board,
            required: board_mass,
            equipment: Some(adze),
            context: "liberation kit riddle boards",
        },
    );
    let (live_panel, live_panel_batches, panel_source) = manual_craft_plan_with_equipment(
        registries,
        state,
        &[panel_feed],
        panel,
        required_panel,
        adze,
        "liberation kit riddle panel",
    );
    assert_eq!(live_panel.process(), definition.process());
    assert_eq!(live_panel_batches, panel_batches);
    let panel_request = select_manual_craft_request(
        registries,
        state,
        live_panel.process(),
        panel_source,
        live_panel_batches,
        "liberation kit riddle panel",
    )
    .with_equipment(adze);
    ticks = ticks
        .checked_add(
            execute_manual_craft(
                registries,
                state,
                panel_request,
                parts,
                "liberation kit riddle panel",
            )
            .value(),
        )
        .unwrap_or_else(|| panic!("liberation kit riddle attention overflowed"));
    ticks
}

pub(super) fn acquire_raw_kit<T>(
    registries: &Registries,
    case: FocusedProbeCase,
    planned_batches: u64,
    selected_build: bool,
    bootstrap_before_admission: impl FnOnce(&mut AppState) -> T,
) -> (AcquiredPrimitiveKit, T) {
    let seed = case.seed();
    assert!(
        planned_batches > 0,
        "liberation kit acquisition requires a disclosed nonzero campaign horizon"
    );
    let RawKitMaterialPlan {
        adze_requirements,
        final_requirements,
        required_panel,
        raw_requirements,
        raw_mass,
        ..
    } = raw_kit_material_plan(registries);
    let stone_raw = raw_requirements
        .get(&CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
        .copied()
        .unwrap_or(Mass::ZERO);
    let wood_raw = raw_requirements
        .get(&CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .copied()
        .unwrap_or(Mass::ZERO);

    let mut state = AppState::new();
    let player_position = super::PRIMITIVE_LIBERATION_ORIGIN;
    assert!(!stone_raw.is_zero() && !wood_raw.is_zero());
    seed_surface_resource(
        registries,
        &mut state,
        player_position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        stone_raw,
        ROOM_TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    );
    seed_surface_resource(
        registries,
        &mut state,
        player_position,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        wood_raw,
        ROOM_TEMPERATURE,
        MaterialComposition::pure(MATERIAL_WOOD),
    );
    let inherited =
        seed_inherited_progression_infrastructure(registries, &mut state, case, player_position);
    let bootstrap = bootstrap_before_admission(&mut state);
    let raw = validate_initialize_player_logistics(&state, player_position, raw_mass)
        .unwrap_or_else(|error| panic!("liberation carried-custody setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation carried-custody commit failed: {error}"))
        .carried_stockpile();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("liberation kit survival setup failed: {error}"));
    super::super::exact_local_runtime::assert_exact_local_runtime_ready(
        registries,
        &state,
        "primitive liberation",
    );
    let crusher = unique_local_equipment(
        &state,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        "primitive liberation inherited crusher",
    );
    let separator = unique_local_equipment(
        &state,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        "primitive liberation inherited separator",
    );
    let power_provider = unique_local_equipment(
        &state,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        "primitive liberation inherited hand crank",
    );
    let drive = unique_local_energy_store(
        &state,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        "primitive liberation inherited flywheel",
    );
    assert_eq!(
        (crusher, separator, power_provider, drive),
        (
            inherited.crusher,
            inherited.separator,
            inherited.power_provider,
            inherited.drive,
        ),
        "liberation actor-visible inherited processing line diverged from admitted fixture custody"
    );
    let decision_state = state.clone();
    let mut state = decision_state.clone();
    let parts = validate_allocate_player_ground_stockpile(&state, raw_mass)
        .unwrap_or_else(|error| panic!("liberation parts allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation parts allocation commit failed: {error}"));
    let panel_feed = validate_allocate_player_ground_stockpile(&state, raw_mass)
        .unwrap_or_else(|error| panic!("liberation panel-feed allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation panel-feed allocation commit failed: {error}"));
    let find_local_surface = |commodity: CommodityKey| {
        let matches = state
            .available_surface_resources()
            .filter(|resource| resource.commodity() == commodity)
            .map(|resource| resource.id())
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "liberation actor must observe exactly one local resource for commodity {commodity:?}"
        );
        matches[0]
    };
    let stone_surface = find_local_surface(CommodityKey::new(MATERIAL_STONE, FORM_LUMP));
    let wood_surface = find_local_surface(CommodityKey::new(MATERIAL_WOOD, FORM_LOG));
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation kit player survival disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("liberation kit matter setup failed: {error}"))
        .total();
    let started_at = state.tick().value();
    let stone_gathering = gather_surface_resource(
        registries,
        &mut state,
        stone_surface,
        raw,
        stone_raw,
        "stone",
    );
    let wood_gathering = gather_surface_resource(
        registries,
        &mut state,
        wood_surface,
        raw,
        wood_raw,
        "fallen timber",
    );
    let gathering_attention = stone_gathering
        .checked_add(wood_gathering)
        .unwrap_or_else(|| panic!("liberation raw gathering attention overflowed"));
    let remaining_local = state
        .available_surface_resources()
        .map(|resource| resource.id())
        .collect::<Vec<_>>();
    assert!(
        !remaining_local.contains(&stone_surface),
        "liberation loose-stone opportunity must be depleted by canonical gathering",
    );
    assert!(
        !remaining_local.contains(&wood_surface),
        "liberation fallen-timber opportunity must be depleted by canonical gathering",
    );
    let gathering_completed_at = state.tick().value();
    let projected_attention = project_incremental_kit_acquisition_attention(registries);

    for (commodity, required) in adze_requirements {
        craft_component(
            registries,
            &mut state,
            ComponentCraftPlan {
                raw,
                destination: parts,
                commodity,
                required,
                equipment: None,
                context: "liberation kit adze component",
            },
        );
    }
    let assembled_adze =
        validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_WOODWORKING_ADZE, parts)
            .unwrap_or_else(|error| panic!("liberation kit adze assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("liberation kit adze commit failed: {error}"));
    let adze = unique_local_equipment(&state, EQUIPMENT_STONE_WOODWORKING_ADZE, "woodworking adze");
    assert_eq!(
        adze, assembled_adze,
        "liberation assembled adze identity diverged from actor-visible local equipment"
    );
    let adze_ready_at = state.tick().value();

    for (commodity, required) in final_requirements {
        let equipment = (commodity == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)).then_some(adze);
        craft_component(
            registries,
            &mut state,
            ComponentCraftPlan {
                raw,
                destination: parts,
                commodity,
                required,
                equipment,
                context: "liberation kit component",
            },
        );
    }
    let extension_components_ready_at = state.tick().value();
    craft_riddle_panel(
        registries,
        &mut state,
        raw,
        panel_feed,
        parts,
        adze,
        required_panel,
    );
    let riddle_ready_at = state.tick().value();
    let assemble = |state: &mut AppState, definition| {
        let assembled = validate_assemble_equipment(registries, state, definition, parts)
            .unwrap_or_else(|error| panic!("liberation kit equipment assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("liberation kit equipment commit failed: {error}"));
        let observed = unique_local_equipment(state, definition, "liberation extension equipment");
        assert_eq!(
            observed, assembled,
            "liberation assembled extension identity diverged from actor-visible local equipment"
        );
        observed
    };
    let quern = assemble(&mut state, EQUIPMENT_STONE_ROTARY_QUERN);
    let screen = assemble(&mut state, EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN);

    let remaining_raw = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| {
            panic!("liberation carried raw stockpile disappeared after fabrication")
        });
    assert_eq!(
        remaining_raw,
        Mass::ZERO,
        "incremental liberation opportunity is sized to the selected extension and must be fully embodied"
    );
    let consumed_raw = raw_mass.checked_sub(remaining_raw).unwrap_or_else(|| {
        unreachable!("remaining liberation raw cannot exceed disclosed opportunity")
    });
    let carrying = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("liberation carried-custody assessment disappeared"));
    assert_eq!(carrying.position(), player_position);
    assert_eq!(carrying.stockpile(), raw);
    assert_eq!(carrying.capacity(), raw_mass);
    assert_eq!(carrying.stored(), remaining_raw);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("liberation kit final matter audit failed: {error}"))
            .total(),
        matter_before,
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("liberation kit final state invalid: {error}"));
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation kit final survival disappeared"));
    let attention = state.tick().value() - started_at;
    let fabrication_attention = state.tick().value() - gathering_completed_at;
    let adze_attention = adze_ready_at - gathering_completed_at;
    let extension_component_attention = extension_components_ready_at - adze_ready_at;
    let riddle_attention = riddle_ready_at - extension_components_ready_at;
    assert_eq!(
        adze_attention + extension_component_attention + riddle_attention,
        fabrication_attention,
        "liberation fabrication phases must account for the full post-gathering fabrication wall"
    );
    assert_eq!(
        fabrication_attention, projected_attention.fabrication_ticks,
        "liberation kit executed fabrication attention diverged from the pre-action projection"
    );
    assert_eq!(
        gathering_attention, projected_attention.gathering_ticks,
        "liberation kit executed gathering attention diverged from the pre-action projection"
    );
    assert_eq!(
        gathering_attention
            .checked_add(fabrication_attention)
            .unwrap_or_else(|| panic!("liberation acquisition attention overflowed")),
        attention,
        "liberation gathering plus fabrication must account for the complete acquisition wall"
    );
    assert_eq!(
        attention, projected_attention.total_ticks,
        "liberation complete acquisition attention diverged from the pre-action projection"
    );
    let metabolic = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("liberation kit metabolic reserve increased"));
    let hydration = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| panic!("liberation kit hydration reserve increased"));
    reviewln!(
        "LIBERATION KIT ACQUISITION seed=0x{seed:016X} evidence={} scope=progression-carryover->incremental-liberation-kit branch={} continuity=separate-episode-inherited-progression-line inherited=[provider:copper-reinforced-hand-crank crusher:copper-reinforced-stone separator:copper-reinforced-stone drive:copper-banded-stone-flywheel condition:{}..{}ppm embodied:{}mg] raw-origin=controlled-finite-surface acquisition=canonical-same-voxel-gather carried-custody=finite@voxel runtime-surface-gathering-proved=true ordinary-world-source-generation-proved=false disclosed-campaign={}batches workload-known-before-build=true raw=[stone:{}mg wood:{}mg total:{}mg] raw-use=[consumed:{}mg remaining:{}mg] built=[adze:true quern:true timber-riddle:true] incremental-attention:{}t gathering:{}t fabrication=[total:{}t adze:{}t extension-components:{}t riddle-panel:{}t] body={}nJ/{}uL copper-screen-upgrade=proved-by-progression-continuation matter=conserved",
        if selected_build {
            "actor-selected"
        } else {
            "counterfactual-only"
        },
        if selected_build {
            "selected"
        } else {
            "counterfactual"
        },
        inherited.minimum_condition_ppm,
        inherited.maximum_condition_ppm,
        inherited.embodied_mass.milligrams(),
        planned_batches,
        stone_raw.milligrams(),
        wood_raw.milligrams(),
        raw_mass.milligrams(),
        consumed_raw.milligrams(),
        remaining_raw.milligrams(),
        attention,
        gathering_attention,
        fabrication_attention,
        adze_attention,
        extension_component_attention,
        riddle_attention,
        metabolic.nanojoules(),
        hydration.microliters(),
    );
    let kit = AcquiredPrimitiveKit {
        decision_state,
        build_state: state,
        crusher,
        quern,
        screen,
        separator,
        power_provider,
        drive,
        review: RawKitAcquisitionReview {
            attention_ticks: attention,
            metabolic_cost_nj: metabolic.nanojoules(),
            hydration_cost_ul: hydration.microliters(),
        },
    };
    (kit, bootstrap)
}
