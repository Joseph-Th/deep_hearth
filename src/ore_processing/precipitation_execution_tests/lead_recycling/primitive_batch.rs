//! The first-foundry crucible melts lead but cannot bypass its small batch limit.

use super::*;

#[test]
fn first_foundry_lead_recycling_respects_small_crucible_batch_limit() {
    let (mut fixture, source, _destination, ingot, _scrap, _contaminated) = remelt_fixture();
    let profile = fixture
        .registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE)
        .unwrap()
        .assembly_profile()
        .unwrap();
    let assembly = add_solid_stockpile_for_test(&mut fixture.state, profile.input_mass()).unwrap();
    for input in profile.inputs() {
        deposit_lot_for_test(
            &fixture.registries,
            &mut fixture.state,
            assembly,
            input.commodity(),
            input.mass(),
            AMBIENT,
        )
        .unwrap();
    }
    let furnace = validate_assemble_equipment(
        &fixture.registries,
        &fixture.state,
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        assembly,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    mount_machine_fixture(&fixture.registries, &mut fixture.state, furnace, 2);
    let too_large = [MaterialLotSelection::new(
        ingot,
        Mass::from_milligrams(41_440),
    )];
    let before = fixture.state.clone();
    assert_eq!(
        resolve_melting_process(
            &fixture.registries,
            &fixture.state,
            MeltingRequest::new(
                PROCESS_MELT_RECYCLED_LEAD,
                source,
                &too_large,
                furnace,
                fixture.electricity,
            ),
        ),
        Err(MeltingResolutionError::BatchMassExceedsEquipmentCapacity {
            selected: Mass::from_milligrams(41_440),
            maximum: Mass::from_milligrams(20_000),
        }),
    );
    assert_eq!(fixture.state, before);

    let partial = [MaterialLotSelection::new(
        ingot,
        Mass::from_milligrams(15_000),
    )];
    let admitted = resolve_melting_process(
        &fixture.registries,
        &fixture.state,
        MeltingRequest::new(
            PROCESS_MELT_RECYCLED_LEAD,
            source,
            &partial,
            furnace,
            fixture.electricity,
        ),
    )
    .unwrap();
    assert_eq!(
        admitted.process_resolution().outputs()[0]
            .mass()
            .milligrams(),
        15_000
    );
    assert!(admitted.required_energy() > Energy::ZERO);
}
