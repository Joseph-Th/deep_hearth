//! Prepared PbS from primitive sorting enters the existing closed iron precipitation reaction.

use super::*;
use crate::content::{MATERIAL_STONE, PROCESS_HAND_PICK_GALENA};
use crate::logistics::{
    place_energy_store_for_fixture, place_equipment_for_fixture, validate_place_ground_stockpile,
};
use crate::material::CompositionComponent;
use crate::ore_processing::{
    ManualConstituentSeparationRequest, resolve_manual_constituent_separation_process,
    validate_start_manual_constituent_separation,
};
use crate::player::validate_admit_player;
use crate::production::validate_start_player_process_routed;

#[test]
fn sorted_galena_and_iron_produce_molten_lead_with_finite_energy_and_conserved_matte() {
    let mut fixture = charge_fixture();
    let batch = Mass::from_milligrams(100_000);
    let ore_source = add_solid_stockpile_for_test(
        &mut fixture.state,
        batch.checked_add(Mass::from_milligrams(IRON_MG)).unwrap(),
    )
    .unwrap();
    let tailings = add_solid_stockpile_for_test(&mut fixture.state, batch).unwrap();
    let assay = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_GALENA, 800_000),
        CompositionComponent::new(MATERIAL_STONE, 200_000),
    ])
    .unwrap();
    let coarse = ParticleSizeRange::new(
        Length::from_micrometers(2_000),
        Length::from_micrometers(10_000),
    )
    .unwrap();
    let feed = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED),
        batch,
        Temperature::from_millikelvin(293_150),
        assay,
        coarse,
    )
    .unwrap();
    let ore = deposit_lot_spec_for_test(&fixture.registries, &mut fixture.state, ore_source, feed)
        .unwrap();
    let unsorted_iron = deposit_lot_for_test(
        &fixture.registries,
        &mut fixture.state,
        ore_source,
        CommodityKey::new(MATERIAL_IRON, FORM_SCRAP),
        Mass::from_milligrams(IRON_MG),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let invalid_charge = [
        MaterialLotSelection::new(ore, Mass::from_milligrams(GALENA_MG)),
        MaterialLotSelection::new(unsorted_iron, Mass::from_milligrams(IRON_MG)),
    ];
    let before_invalid = fixture.state.clone();
    assert_eq!(
        resolve_precipitation_process(
            &fixture.registries,
            &fixture.state,
            PrecipitationRequest::new(
                PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
                ore_source,
                &invalid_charge,
                fixture.furnace,
                fixture.electricity,
            ),
        )
        .err(),
        Some(PrecipitationResolutionError::Batch(
            PrecipitationBatchError::ImpureInput {
                commodity: CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED),
            },
        )),
    );
    assert_eq!(
        fixture.state, before_invalid,
        "impure charge rejection must be nonmutating"
    );

    // A supported furnace and salvaged iron exist as controlled infrastructure.
    // The reacted PbS batch must instead come from the authored manual separation job.
    let player_position = VoxelCoord::new(0, 0, 0);
    place_equipment_for_fixture(&mut fixture.state, fixture.furnace, player_position);
    place_energy_store_for_fixture(&mut fixture.state, fixture.electricity, player_position);
    for stockpile in [
        ore_source,
        fixture.source,
        tailings,
        fixture.lead_destination,
        fixture.matte_destination,
    ] {
        validate_place_ground_stockpile(&fixture.state, stockpile, player_position)
            .unwrap()
            .commit(&mut fixture.state)
            .unwrap();
    }
    validate_admit_player(&fixture.registries, &fixture.state, player_position)
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
    let selection = [MaterialLotSelection::new(ore, batch)];
    let picking = resolve_manual_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ManualConstituentSeparationRequest::new(PROCESS_HAND_PICK_GALENA, ore_source, &selection),
    )
    .unwrap();
    assert_eq!(picking.target_mass().milligrams(), 52_000);
    let initial_matter = calculate_matter_accounting(&fixture.state).unwrap().total();
    validate_start_manual_constituent_separation(
        &fixture.registries,
        &fixture.state,
        &picking,
        ore_source,
        fixture.source,
        tailings,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    for _ in 0..picking.duration().value() {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    }

    let galena = fixture
        .state
        .inventory()
        .lot_ids(fixture.source)
        .find(|&id| {
            fixture.state.inventory().get_lot(id).is_some_and(|lot| {
                lot.commodity() == CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)
                    && lot.mass().milligrams() == 52_000
            })
        })
        .expect("hand-picked PbS must arrive as its own coarse material lot");
    assert_ne!(
        galena, fixture.galena,
        "original prepared fixture lot must stay unselected"
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_lot(galena)
            .unwrap()
            .composition(),
        &MaterialComposition::pure(MATERIAL_GALENA),
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(tailings)
            .unwrap()
            .stored_mass()
            .milligrams(),
        48_000,
    );
    let charge = [
        MaterialLotSelection::new(galena, Mass::from_milligrams(GALENA_MG)),
        MaterialLotSelection::new(fixture.iron, Mass::from_milligrams(IRON_MG)),
    ];
    let smelt = resolve_precipitation_process(
        &fixture.registries,
        &fixture.state,
        PrecipitationRequest::new(
            PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
            fixture.source,
            &charge,
            fixture.furnace,
            fixture.electricity,
        ),
    )
    .expect("physically sorted galena must be compatible with the balanced furnace charge");
    assert!(smelt.required_energy() > Energy::ZERO);
    let outputs = smelt.process_resolution().output_streams();
    assert_eq!(
        outputs[0].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN)
    );
    assert_eq!(outputs[0].outputs()[0].mass().milligrams(), LEAD_MG);
    assert_eq!(
        outputs[1].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_IRON_SULFIDE, FORM_MATTE)
    );
    assert_eq!(outputs[1].outputs()[0].mass().milligrams(), MATTE_MG);
    let duration = smelt.process_resolution().duration();
    validate_start_player_process_routed(
        &fixture.registries,
        &fixture.state,
        smelt.process_resolution(),
        fixture.source,
        &[
            ProcessOutputRoute::new(ProcessOutputStreamId::PRIMARY, fixture.lead_destination),
            ProcessOutputRoute::new(ProcessOutputStreamId::RESIDUE, fixture.matte_destination),
        ],
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    let save = serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    for _ in 0..duration.value() {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
        let _ = advance_tick(&fixture.registries, &mut resumed).unwrap();
    }
    assert_eq!(fixture.state, resumed);
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.lead_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN))
            .milligrams(),
        LEAD_MG,
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.matte_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_IRON_SULFIDE, FORM_MATTE))
            .milligrams(),
        MATTE_MG,
    );
    assert_eq!(
        calculate_matter_accounting(&fixture.state).unwrap().total(),
        initial_matter
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}
