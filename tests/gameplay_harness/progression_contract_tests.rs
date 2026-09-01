//! Cheap primitive-progression topology and generator contracts.

use std::collections::BTreeSet;

use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    FORM_CHIP, FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_KNAP_STONE_TOOL,
    PROCESS_REKNAP_STONE_SCRAP_TOOL, PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};
use deep_hearth::material::CommodityKey;

use super::catalog::{ProcessResolverKind, process_catalog_entries};
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::progression_probe::{
    OreOpportunityDepth, extraction_grade_premium_ppm, manual_processing::manual_processing_setup,
    ore_opportunity, varied_four_way_order,
};

#[test]
fn primitive_recovery_and_reinforcement_routes_remain_connected() {
    let registries = build_registries();
    let catalog = process_catalog_entries(&registries);

    let hand_break = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore breaking disappeared"));
    let manual_sort = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual native-copper sorting disappeared"));
    let powered_sort = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("powered native-copper separation disappeared"));
    assert_eq!(manual_sort.input_form(), hand_break.output_form());
    assert_eq!(
        manual_sort.input_particle_size_range(),
        hand_break.output_particle_size()
    );
    assert!(manual_sort.target_recovery_ppm() > 0);
    assert!(manual_sort.target_recovery_ppm() < powered_sort.target_recovery_ppm());
    assert!(!manual_sort.max_batch_mass().is_zero());
    assert!(!manual_sort.processing_rate().is_zero());

    for process in [PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER] {
        let entry = catalog
            .iter()
            .find(|entry| entry.process == process)
            .unwrap_or_else(|| {
                panic!("manual progression process is absent from process topology")
            });
        assert!(matches!(
            entry.resolver,
            ProcessResolverKind::ManualComminution | ProcessResolverKind::ManualSeparation
        ));
        assert_eq!(
            (
                entry.nominal_provider_count,
                entry.compatible_energy_store_count
            ),
            (0, 0)
        );
    }

    let fresh_stone = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("fresh stone knapping disappeared"));
    let recycled_stone = registries
        .crafting()
        .get_manual(PROCESS_REKNAP_STONE_SCRAP_TOOL)
        .unwrap_or_else(|| panic!("stone scrap reknapping disappeared"));
    assert_eq!(
        recycled_stone.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
    );
    assert!(recycled_stone.duration() > fresh_stone.duration());
    let recovered = recycled_stone
        .outputs()
        .iter()
        .filter(|output| {
            matches!(
                output.commodity(),
                commodity
                    if commodity == CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
                        || commodity == CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
            )
        })
        .map(|output| output.mass())
        .try_fold(deep_hearth::core::quantity::Mass::ZERO, |total, mass| {
            total.checked_add(mass)
        })
        .unwrap_or_else(|| panic!("stone recovery output mass overflowed"));
    assert_eq!(recovered, recycled_stone.input_mass());

    let native_work = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .unwrap_or_else(|| panic!("native-copper reinforcement work disappeared"));
    let scrap_rework = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT)
        .unwrap_or_else(|| panic!("copper scrap rework disappeared"));
    assert!(scrap_rework.duration() > native_work.duration());

    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let canonical = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("reinforced pick lost its additive upgrade route"))
        .additions()
        .inputs();
    assert_eq!(canonical.len(), 1);
    assert_eq!(canonical[0].commodity(), reinforcement);
    let canonical = &canonical[0];

    let compatible_targets = registries
        .equipment()
        .definitions()
        .filter_map(|definition| {
            let inputs = definition.upgrade_profile()?.additions().inputs();
            (inputs.len() == 1
                && inputs[0].commodity() == canonical.commodity()
                && inputs[0].mass() == canonical.mass())
            .then_some(definition.id())
        })
        .collect::<BTreeSet<_>>();
    let required_targets = BTreeSet::from([
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    ]);
    assert!(required_targets.is_subset(&compatible_targets));
    for target in required_targets {
        let definition = registries
            .equipment()
            .get_equipment(target)
            .unwrap_or_else(|| panic!("required primitive reinforcement target disappeared"));
        assert!(definition.maintenance_profile().is_some());
        assert!(definition.worn_recovery_form().is_some());
    }

    let base_flywheel = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("stone flywheel disappeared"));
    let reinforced_flywheel = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("reinforced flywheel disappeared"));
    let upgrade = reinforced_flywheel
        .upgrade_profile()
        .unwrap_or_else(|| panic!("reinforced flywheel lost its additive upgrade route"));
    assert_eq!(upgrade.from(), ENERGY_STONE_FLYWHEEL_DRIVE);
    assert_eq!(
        upgrade.additions().inputs(),
        std::slice::from_ref(canonical)
    );
    assert_eq!(base_flywheel.carrier(), reinforced_flywheel.carrier());
    assert!(reinforced_flywheel.capacity() > base_flywheel.capacity());
    assert!(reinforced_flywheel.max_input_power() >= base_flywheel.max_input_power());
    assert!(reinforced_flywheel.max_output_power() >= base_flywheel.max_output_power());
}

#[test]
fn progression_generators_cover_distinct_search_and_economic_pressures() {
    let clue_orders = (1_u64..=12)
        .map(varied_four_way_order)
        .collect::<BTreeSet<_>>();
    assert!(
        clue_orders.len() > 1,
        "progression clue ordering collapsed to one permutation"
    );
    assert!(clue_orders.iter().all(|order| {
        let mut sorted = *order;
        sorted.sort_unstable();
        sorted == [0, 1, 2, 3]
    }));

    let opportunities = (1_u64..=32)
        .map(|seed| ore_opportunity(seed, false))
        .collect::<Vec<_>>();
    assert_eq!(
        opportunities
            .iter()
            .map(|opportunity| opportunity.depth())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([OreOpportunityDepth::Shallow, OreOpportunityDepth::Deep]),
        "organic progression must exercise both finite-opportunity classes"
    );
    let shallow_max = opportunities
        .iter()
        .filter(|opportunity| opportunity.depth() == OreOpportunityDepth::Shallow)
        .map(|opportunity| opportunity.batch_budget())
        .max()
        .unwrap_or_else(|| panic!("organic progression generated no shallow opportunity"));
    let deep_min = opportunities
        .iter()
        .filter(|opportunity| opportunity.depth() == OreOpportunityDepth::Deep)
        .map(|opportunity| opportunity.batch_budget())
        .min()
        .unwrap_or_else(|| panic!("organic progression generated no deep opportunity"));
    assert!(deep_min > shallow_max);
    assert_eq!(
        ore_opportunity(1, true).depth(),
        OreOpportunityDepth::Deep,
        "maintained progression must keep a deep automation-payback opportunity"
    );

    let actor_policies = (1_u64..=16)
        .map(|behavior_seed| {
            extraction_grade_premium_ppm(FocusedProbeCase::new(
                0xCAFE,
                Some(behavior_seed),
                FocusedProbeRole::OrganicVariation,
            ))
        })
        .collect::<BTreeSet<_>>();
    assert!(
        actor_policies.len() > 1,
        "progression actor policy collapsed to one threshold"
    );

    let registries = build_registries();
    let manual_setups = (1_u64..=16)
        .map(|seed| manual_processing_setup(&registries, seed))
        .collect::<Vec<_>>();
    assert!(
        manual_setups
            .iter()
            .map(|setup| setup.ore_mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
    assert!(
        manual_setups
            .iter()
            .map(|setup| setup.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
    assert!(
        manual_setups
            .iter()
            .map(|setup| setup.clay_share_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
}
