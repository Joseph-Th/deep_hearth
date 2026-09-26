//! Evidence-driven fieldwork retooling and owned-tool reuse contracts.

use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
    build_registries,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::matter::calculate_matter_accounting;

use super::extraction::{FieldworkExtractionOrder, execute_fieldwork_extraction};
use super::planning::{FIELDWORK_TOOLS, fieldwork_mining_limits, multiplied_mass};
use super::preparation::{
    assemble_fieldwork_tool, assemble_sampling_hammer, upgrade_sampling_hammer,
};
use super::retooling::{
    FieldworkOreRecoveryReason, FieldworkOwnedOreRecovery, FieldworkSiteToolRequest,
    prepare_fieldwork_tool_for_site,
};
use super::survey::{
    CHANNEL_START_X, FieldworkSurveyStrategy, SECONDARY_CHANNEL_START_X, localize_target,
};
use super::world::build_fieldwork_world;

#[test]
fn carried_tool_portfolio_reuses_the_best_owned_specialization() {
    let registries = build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let requested = multiplied_mass(
        limits.base_quarry_batch,
        48,
        "owned-tool portfolio regression order",
    );
    let mut world = build_fieldwork_world(&registries, 1, requested, requested);
    let (hard_pick, _) = assemble_fieldwork_tool(
        &registries,
        &mut world.state,
        world.raw,
        world.parts,
        FIELDWORK_TOOLS[1],
    );
    let (quarry, _) = assemble_fieldwork_tool(
        &registries,
        &mut world.state,
        world.raw,
        world.parts,
        FIELDWORK_TOOLS[2],
    );
    let owned = [hard_pick, quarry];
    let recovery = FieldworkOwnedOreRecovery {
        ore_source: world.destination,
        crushed_destination: world.recovery_crushed,
        residue_destination: world.recovery_residue,
    };

    let soft = prepare_fieldwork_tool_for_site(
        &registries,
        &mut world.state,
        FieldworkSiteToolRequest::new(
            world.raw,
            world.parts,
            recovery,
            &owned,
            limits.base_quarry_hardness,
            requested,
        ),
    )
    .unwrap_or_else(|| panic!("owned soft-rock portfolio lost a feasible tool"));
    assert_eq!(soft.equipment, quarry);
    assert_eq!(soft.label, "stone-quarry");
    assert!(soft.reused_existing);
    assert_eq!(soft.preparation_ticks, 0);

    let hard_upper = Pressure::from_pascals(
        limits
            .reinforced_quarry_hardness
            .pascals()
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("bounded fieldwork hardness fits u64")),
    );
    let hard = prepare_fieldwork_tool_for_site(
        &registries,
        &mut world.state,
        FieldworkSiteToolRequest::new(
            world.raw,
            world.parts,
            recovery,
            &owned,
            hard_upper,
            limits.base_quarry_batch,
        ),
    )
    .unwrap_or_else(|| panic!("owned hard-rock portfolio lost its reinforced pick"));
    assert_eq!(hard.equipment, hard_pick);
    assert_eq!(hard.label, "copper-reinforced-hard-pick");
    assert!(hard.reused_existing);
    assert_eq!(hard.preparation_ticks, 0);
}

#[test]
fn obsolete_specialization_can_be_salvaged_into_the_new_geology_tool() {
    let registries = build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let requested = limits.base_quarry_batch;
    let mut world = build_fieldwork_world(&registries, 1, requested, requested);
    assert!(world.copper_rich);
    let (hammer, _) =
        assemble_sampling_hammer(&registries, &mut world.state, world.raw, world.parts);
    let _ = upgrade_sampling_hammer(
        &registries,
        &mut world.state,
        world.raw,
        world.parts,
        hammer,
    );
    let (quarry, _) = assemble_fieldwork_tool(
        &registries,
        &mut world.state,
        world.raw,
        world.parts,
        FIELDWORK_TOOLS[3],
    );
    let hard_upper = Pressure::from_pascals(
        limits
            .reinforced_quarry_hardness
            .pascals()
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("bounded hard-rock witness fits u64")),
    );
    let matter_before = calculate_matter_accounting(&world.state)
        .unwrap_or_else(|error| panic!("salvage retooling matter setup failed: {error}"))
        .total();
    let choice = prepare_fieldwork_tool_for_site(
        &registries,
        &mut world.state,
        FieldworkSiteToolRequest::new(
            world.raw,
            world.parts,
            FieldworkOwnedOreRecovery {
                ore_source: world.recovery_residue,
                crushed_destination: world.recovery_crushed,
                residue_destination: world.recovery_residue,
            },
            &[quarry],
            hard_upper,
            requested,
        ),
    )
    .unwrap_or_else(|| panic!("obsolete quarry specialization should fund a hard-rock rebuild"));

    assert_eq!(choice.salvaged_equipment, Some(quarry));
    assert_eq!(choice.ore_recovery_ticks, 0);
    assert_eq!(choice.ore_recovery_reason, None);
    assert!(world.state.equipment().get_equipment(quarry).is_none());
    assert_eq!(
        world
            .state
            .equipment()
            .get_equipment(choice.equipment)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_PICK)
    );
    assert_eq!(
        calculate_matter_accounting(&world.state)
            .unwrap_or_else(|error| panic!("salvage retooling matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &world.state)
        .unwrap_or_else(|error| panic!("salvage retooling state invalid: {error}"));
}

#[test]
fn owned_ore_specialization_can_pay_back_before_the_current_tool_is_blocked() {
    let registries = build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let project_order = multiplied_mass(
        limits.base_quarry_batch,
        144,
        "owned-ore specialization payoff order",
    );
    let mut world = build_fieldwork_world(
        &registries,
        1,
        project_order,
        Mass::from_milligrams(28_000_000),
    );
    assert!(
        world.copper_rich,
        "specialization payoff witness requires the two initial reinforcement parcels"
    );
    let (hammer, _) =
        assemble_sampling_hammer(&registries, &mut world.state, world.raw, world.parts);
    let (hard_pick, _) = assemble_fieldwork_tool(
        &registries,
        &mut world.state,
        world.raw,
        world.parts,
        FIELDWORK_TOOLS[1],
    );
    let primary = localize_target(
        &registries,
        &mut world.state,
        hammer,
        world.channel_voxels,
        CHANNEL_START_X,
        FieldworkSurveyStrategy::PointSearch,
    );
    let extraction = execute_fieldwork_extraction(
        &registries,
        &mut world.state,
        FieldworkExtractionOrder {
            target: primary.target,
            destination: world.destination,
            equipment: hard_pick,
            requested: Mass::from_milligrams(300_000),
            batch_limit: Mass::from_milligrams(300_000),
        },
    );
    assert_eq!(extraction.extracted, Mass::from_milligrams(300_000));
    let _ = upgrade_sampling_hammer(
        &registries,
        &mut world.state,
        world.raw,
        world.parts,
        hammer,
    );

    let medium_hardness = limits.reinforced_quarry_hardness;
    let mut no_ore_state = world.state.clone();
    let no_ore_choice = prepare_fieldwork_tool_for_site(
        &registries,
        &mut no_ore_state,
        FieldworkSiteToolRequest::new(
            world.raw,
            world.parts,
            FieldworkOwnedOreRecovery {
                ore_source: world.recovery_residue,
                crushed_destination: world.recovery_crushed,
                residue_destination: world.recovery_residue,
            },
            &[hard_pick],
            medium_hardness,
            project_order,
        ),
    )
    .unwrap_or_else(|| {
        panic!("existing reinforced pick must remain a viable medium-hardness route")
    });
    assert_eq!(no_ore_choice.equipment, hard_pick);
    assert!(no_ore_choice.reused_existing);
    assert_eq!(no_ore_choice.ore_recovery_ticks, 0);

    let matter_before = calculate_matter_accounting(&world.state)
        .unwrap_or_else(|error| panic!("specialization payoff matter setup failed: {error}"))
        .total();
    let choice = prepare_fieldwork_tool_for_site(
        &registries,
        &mut world.state,
        FieldworkSiteToolRequest::new(
            world.raw,
            world.parts,
            FieldworkOwnedOreRecovery {
                ore_source: world.destination,
                crushed_destination: world.recovery_crushed,
                residue_destination: world.recovery_residue,
            },
            &[hard_pick],
            medium_hardness,
            project_order,
        ),
    )
    .unwrap_or_else(|| panic!("owned ore should fund the faster medium-hardness specialization"));

    assert_eq!(
        world
            .state
            .equipment()
            .get_equipment(choice.equipment)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK)
    );
    assert!(!choice.reused_existing);
    assert!(choice.ore_recovery_ticks > 0);
    assert_eq!(
        choice.ore_recovery_reason,
        Some(FieldworkOreRecoveryReason::Payback)
    );
    assert!(
        choice
            .preparation_ticks
            .checked_add(choice.projected_order_ticks)
            .is_some_and(|total| total < no_ore_choice.projected_order_ticks),
        "ore-funded quarry specialization must repay its full recovery and fabrication cost on the visible order"
    );
    assert_eq!(
        calculate_matter_accounting(&world.state)
            .unwrap_or_else(|error| panic!("specialization payoff matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &world.state)
        .unwrap_or_else(|error| panic!("specialization payoff state invalid: {error}"));
}

#[test]
fn owned_ore_can_fund_a_harder_site_tool_after_relocation() {
    let registries = build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let requested = multiplied_mass(
        limits.base_quarry_batch,
        40,
        "owned-ore retooling regression order",
    );
    let mut world =
        build_fieldwork_world(&registries, 2, requested, Mass::from_milligrams(28_000_000));
    assert!(
        !world.copper_rich,
        "coverage world must begin without free native copper"
    );
    let (hammer, _) =
        assemble_sampling_hammer(&registries, &mut world.state, world.raw, world.parts);
    let primary = localize_target(
        &registries,
        &mut world.state,
        hammer,
        world.channel_voxels,
        CHANNEL_START_X,
        FieldworkSurveyStrategy::PointSearch,
    );
    let planned_primary = requested.min(primary.resource_mass.upper());
    let estimate = super::planning::choose_fieldwork_tool_with_market_phase(
        &registries,
        &world.state,
        world.raw,
        world.parts,
        primary.hardness.upper(),
        planned_primary,
        "owned-ore-retooling-regression",
    )
    .unwrap_or_else(|| panic!("coverage world lost its initial extraction tool"));
    let (initial_tool, _) = assemble_fieldwork_tool(
        &registries,
        &mut world.state,
        world.raw,
        world.parts,
        estimate.tool,
    );
    let extraction = execute_fieldwork_extraction(
        &registries,
        &mut world.state,
        FieldworkExtractionOrder {
            target: primary.target,
            destination: world.destination,
            equipment: initial_tool,
            requested,
            batch_limit: estimate.batch,
        },
    );
    assert_eq!(extraction.extracted, requested);

    let secondary = localize_target(
        &registries,
        &mut world.state,
        hammer,
        world.channel_voxels,
        SECONDARY_CHANNEL_START_X,
        FieldworkSurveyStrategy::PointSearch,
    );
    let secondary_order = requested.min(secondary.resource_mass.upper());
    let matter_before = calculate_matter_accounting(&world.state)
        .unwrap_or_else(|error| panic!("retooling matter setup failed: {error}"))
        .total();
    let choice = prepare_fieldwork_tool_for_site(
        &registries,
        &mut world.state,
        FieldworkSiteToolRequest::new(
            world.raw,
            world.parts,
            FieldworkOwnedOreRecovery {
                ore_source: world.destination,
                crushed_destination: world.recovery_crushed,
                residue_destination: world.recovery_residue,
            },
            &[initial_tool],
            secondary.hardness.upper(),
            secondary_order,
        ),
    )
    .unwrap_or_else(|| panic!("owned primary ore failed to fund harder-site adaptation"));

    assert_eq!(
        world
            .state
            .equipment()
            .get_equipment(choice.equipment)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_PICK)
    );
    assert!(!choice.reused_existing);
    assert!(choice.ore_recovery_ticks > 0);
    assert_eq!(
        choice.ore_recovery_reason,
        Some(FieldworkOreRecoveryReason::RequiredAccess)
    );
    assert!(!choice.ore_feed_mass.is_zero());
    assert!(!choice.recovered_native.is_zero());
    assert_eq!(
        calculate_matter_accounting(&world.state)
            .unwrap_or_else(|error| panic!("retooling matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &world.state)
        .unwrap_or_else(|error| panic!("retooling state invalid: {error}"));
}
