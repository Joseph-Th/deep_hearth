//! Lead metal can re-enter the thermal loop; impure stock and PbS may not.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE, FORM_SCRAP, MATERIAL_STONE, PROCESS_MELT_RECYCLED_LEAD,
};
use crate::equipment::validate_assemble_equipment;

#[path = "lead_recycling/primitive_batch.rs"]
mod primitive_batch;
use crate::inventory::{MaterialLotId, deposit_composed_lot_for_test};
use crate::logistics::{
    place_energy_store_for_fixture, place_equipment_for_fixture, validate_place_ground_stockpile,
};
use crate::material::CompositionComponent;
use crate::player::validate_admit_player;
use crate::production::validate_start_player_process;
use crate::thermal::{
    MeltingBatchError, MeltingRequest, MeltingResolutionError, resolve_melting_process,
};

const AT: VoxelCoord = VoxelCoord::new(0, 0, 0);
const AMBIENT: Temperature = Temperature::from_millikelvin(293_150);

fn remelt_fixture() -> (
    PrecipitationFixture,
    StockpileId,
    StockpileId,
    MaterialLotId,
    MaterialLotId,
    MaterialLotId,
) {
    let mut fixture = charge_fixture();
    let source =
        add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(150_000)).unwrap();
    let destination = add_stockpile(
        &mut fixture.state,
        Mass::from_milligrams(150_000),
        StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000))
            .unwrap(),
    )
    .unwrap();
    let ingot = deposit_lot_for_test(
        &fixture.registries,
        &mut fixture.state,
        source,
        CommodityKey::new(MATERIAL_LEAD, FORM_INGOT),
        Mass::from_milligrams(41_440),
        AMBIENT,
    )
    .unwrap();
    let scrap = deposit_lot_for_test(
        &fixture.registries,
        &mut fixture.state,
        source,
        CommodityKey::new(MATERIAL_LEAD, FORM_SCRAP),
        Mass::from_milligrams(5_000),
        AMBIENT,
    )
    .unwrap();
    let contaminated = deposit_composed_lot_for_test(
        &fixture.registries,
        &mut fixture.state,
        source,
        CommodityKey::new(MATERIAL_LEAD, FORM_INGOT),
        Mass::from_milligrams(10_000),
        AMBIENT,
        MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_LEAD, 900_000),
            CompositionComponent::new(MATERIAL_STONE, 100_000),
        ])
        .unwrap(),
    )
    .unwrap();
    (fixture, source, destination, ingot, scrap, contaminated)
}

#[test]
fn lead_remelting_rejects_compositionally_mixed_ingots_and_unreduced_sulfide() {
    let (fixture, source, _destination, _ingot, _scrap, contaminated) = remelt_fixture();
    let before = fixture.state.clone();
    let impure = [MaterialLotSelection::new(
        contaminated,
        Mass::from_milligrams(10_000),
    )];
    assert_eq!(
        resolve_melting_process(
            &fixture.registries,
            &fixture.state,
            MeltingRequest::new(
                PROCESS_MELT_RECYCLED_LEAD,
                source,
                &impure,
                fixture.furnace,
                fixture.electricity,
            )
        ),
        Err(MeltingResolutionError::Batch(
            MeltingBatchError::ImpureInput {
                commodity: CommodityKey::new(MATERIAL_LEAD, FORM_INGOT),
            }
        )),
    );
    assert_eq!(
        resolve_melting_process(
            &fixture.registries,
            &fixture.state,
            MeltingRequest::new(
                PROCESS_MELT_RECYCLED_LEAD,
                fixture.source,
                &[MaterialLotSelection::new(
                    fixture.galena,
                    Mass::from_milligrams(GALENA_MG)
                )],
                fixture.furnace,
                fixture.electricity,
            )
        ),
        Err(MeltingResolutionError::Batch(
            MeltingBatchError::InputFormNotAccepted {
                found: FORM_CRUSHED,
            }
        )),
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn admitted_player_locally_remelts_two_real_lead_forms_with_finite_work_and_save_replay() {
    let (mut fixture, source, destination, ingot, scrap, _contaminated) = remelt_fixture();
    for stockpile in [source, destination] {
        validate_place_ground_stockpile(&fixture.state, stockpile, AT)
            .unwrap()
            .commit(&mut fixture.state)
            .unwrap();
    }
    place_equipment_for_fixture(&mut fixture.state, fixture.furnace, AT);
    place_energy_store_for_fixture(&mut fixture.state, fixture.electricity, AT);
    validate_admit_player(&fixture.registries, &fixture.state, AT)
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();

    let mass_before = calculate_matter_accounting(&fixture.state).unwrap().total();
    let stored_before = fixture
        .state
        .energy()
        .get_store(fixture.electricity)
        .unwrap()
        .stored();
    let selected = [
        MaterialLotSelection::new(ingot, Mass::from_milligrams(41_440)),
        MaterialLotSelection::new(scrap, Mass::from_milligrams(5_000)),
    ];
    let melting = resolve_melting_process(
        &fixture.registries,
        &fixture.state,
        MeltingRequest::new(
            PROCESS_MELT_RECYCLED_LEAD,
            source,
            &selected,
            fixture.furnace,
            fixture.electricity,
        ),
    )
    .unwrap();
    assert!(melting.required_energy() > Energy::ZERO);
    let output = &melting.process_resolution().outputs()[0];
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN)
    );
    assert_eq!(output.mass().milligrams(), 46_440);
    assert_eq!(output.temperature().millikelvin(), 600_610);
    let duration = melting.process_resolution().duration().value();
    validate_start_player_process(
        &fixture.registries,
        &fixture.state,
        melting.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    let saved =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    for _ in 0..duration {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
        let _ = advance_tick(&fixture.registries, &mut resumed).unwrap();
    }
    assert_eq!(fixture.state, resumed);
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN),)
            .milligrams(),
        46_440,
    );
    assert!(
        fixture
            .state
            .energy()
            .get_store(fixture.electricity)
            .unwrap()
            .stored()
            < stored_before
    );
    assert_eq!(
        calculate_matter_accounting(&fixture.state).unwrap().total(),
        mass_before
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}
