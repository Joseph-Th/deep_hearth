//! Physical workload and inherited-copper generation for ordinary woodworking episodes.

use deep_hearth::content::{
    FORM_BOARD, MATERIAL_WOOD, PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

use super::primitive_workload::primitive_mining_cycle_mass;
use super::seed::mix64;

pub(super) fn organic_woodworking_native_copper(registries: &Registries, seed: u64) -> Mass {
    let prior_cycle = primitive_mining_cycle_mass(registries, seed);
    let scale_ppm = 50_000_u64 + mix64(seed ^ 0x574F_4F44_434F_5050) % 2_450_001;
    let milligrams = prior_cycle
        .milligrams()
        .checked_mul(scale_ppm)
        .map(|scaled| scaled / 1_000_000)
        .unwrap_or_else(|| panic!("woodworking inherited copper opportunity overflowed"));
    Mass::from_milligrams(milligrams)
}

#[derive(Clone, Copy)]
pub(super) struct WoodworkingDemandPlan {
    #[allow(
        dead_code,
        reason = "exploratory generation contracts and reports consume the demand-horizon label"
    )]
    pub(super) horizon: &'static str,
    pub(super) immediate_scale: u64,
    pub(super) immediate_boards: Mass,
    pub(super) pipeline_boards: Mass,
    pub(super) adze_batches: u64,
    pub(super) saw_batches: u64,
    pub(super) adze_input_mass: Mass,
    pub(super) saw_input_mass: Mass,
}

fn authored_output_mass(
    definition: &deep_hearth::crafting::ManualCraftDefinition,
    commodity: CommodityKey,
) -> Mass {
    definition
        .outputs()
        .iter()
        .find(|output| output.commodity() == commodity)
        .map(|output| output.mass())
        .unwrap_or_else(|| {
            panic!(
                "woodworking process {} lost authored output {}",
                definition.process().value(),
                commodity.value()
            )
        })
}

pub(super) fn plan_woodworking_demand(
    registries: &Registries,
    seed: u64,
    stratified: bool,
) -> WoodworkingDemandPlan {
    let adze_board_definition = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking adze board process disappeared"));
    let saw_board_definition = registries
        .crafting()
        .get_manual(PROCESS_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking saw board process disappeared"));
    let board_commodity = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let adze_board_mass_per_batch = authored_output_mass(adze_board_definition, board_commodity);
    let saw_board_mass_per_batch = authored_output_mass(saw_board_definition, board_commodity);
    let immediate_roll = mix64(seed ^ 0x574F_4F44_5052_4F4A);
    let queued_roll = mix64(seed ^ 0x574F_4F44_5155_4555);
    let (horizon, immediate_scale, queued_scale) = if stratified {
        match seed & 0b11 {
            0 => ("immediate-only", 1 + immediate_roll % 3, 0),
            1 => (
                "short-queue",
                2 + immediate_roll % 4,
                1 + (queued_roll >> 8) % 6,
            ),
            2 => (
                "project",
                3 + immediate_roll % 5,
                16 + (queued_roll >> 8) % 18,
            ),
            _ => (
                "project",
                8 + immediate_roll % 5,
                34 + (queued_roll >> 8) % 17,
            ),
        }
    } else {
        let project_queue = queued_roll % 51;
        match project_queue {
            0..=5 => ("immediate-only", 1 + immediate_roll % 3, 0),
            6..=15 => (
                "short-queue",
                2 + immediate_roll % 4,
                1 + (queued_roll >> 8) % 6,
            ),
            _ => ("project", 3 + immediate_roll % 10, project_queue),
        }
    };
    let pipeline_scale = immediate_scale
        .checked_add(queued_scale)
        .unwrap_or_else(|| panic!("woodworking demand horizon overflowed"));
    let immediate_boards = Mass::from_milligrams(
        adze_board_mass_per_batch
            .milligrams()
            .checked_mul(immediate_scale)
            .unwrap_or_else(|| panic!("woodworking board demand overflowed")),
    );
    let pipeline_boards = Mass::from_milligrams(
        adze_board_mass_per_batch
            .milligrams()
            .checked_mul(pipeline_scale)
            .unwrap_or_else(|| panic!("woodworking pipeline board demand overflowed")),
    );
    let adze_batches = pipeline_boards
        .milligrams()
        .div_ceil(adze_board_mass_per_batch.milligrams());
    let saw_batches = pipeline_boards
        .milligrams()
        .div_ceil(saw_board_mass_per_batch.milligrams());
    assert_eq!(adze_batches, pipeline_scale);
    WoodworkingDemandPlan {
        horizon,
        immediate_scale,
        immediate_boards,
        pipeline_boards,
        adze_batches,
        saw_batches,
        adze_input_mass: adze_board_definition.input_mass(),
        saw_input_mass: saw_board_definition.input_mass(),
    }
}
