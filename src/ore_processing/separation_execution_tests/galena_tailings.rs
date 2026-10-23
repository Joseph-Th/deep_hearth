//! Physically complete dry PbS tailings recovery through assembled mechanical equipment.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_ROTARY_QUERN, MATERIAL_GALENA, PROCESS_CLEAN_GALENA_CONCENTRATE,
    PROCESS_REGRIND_GALENA_PICKING_TAILINGS, PROCESS_SCAVENGE_GALENA_TAILINGS,
};
use crate::ore_processing::{ComminutionRequest, resolve_comminution_process};
use crate::production::validate_start_process;

#[test]
fn galena_picking_tailings_are_reground_scavenged_and_cleaned_into_pure_pbs() {
    let mass = Mass::from_milligrams(48_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_GALENA, 500_000),
        CompositionComponent::new(MATERIAL_STONE, 500_000),
    ])
    .unwrap();
    let coarse = ParticleSizeRange::new(
        Length::from_micrometers(2_000),
        Length::from_micrometers(10_000),
    )
    .unwrap();
    let mut fixture = fixture_with_host_form_and_particle_size(
        MATERIAL_STONE,
        FORM_TAILINGS,
        mass,
        composition.clone(),
        coarse,
    );
    let definition = fixture
        .registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_ROTARY_QUERN)
        .unwrap();
    let profile = definition.assembly_profile().unwrap();
    let assembly = add_solid_stockpile_for_test(&mut fixture.state, profile.input_mass()).unwrap();
    for input in profile.inputs() {
        deposit_lot_for_test(
            &fixture.registries,
            &mut fixture.state,
            assembly,
            input.commodity(),
            input.mass(),
            TEMPERATURE,
        )
        .unwrap();
    }
    let quern = validate_assemble_equipment(
        &fixture.registries,
        &fixture.state,
        EQUIPMENT_STONE_ROTARY_QUERN,
        assembly,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    // Coarse tailings cannot bypass the quern, and the cleaner cannot turn
    // unsorted tailings directly into a pure chemical-furnace charge.
    let selected = [MaterialLotSelection::new(fixture.lot, mass)];
    let before_rejection = fixture.state.clone();
    let coarse_scavenging = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_SCAVENGE_GALENA_TAILINGS,
            fixture.source,
            &selected,
            fixture.separator,
            fixture.energy,
        ),
    );
    assert_eq!(
        coarse_scavenging.err(),
        Some(ConstituentSeparationResolutionError::Batch(
            ConstituentSeparationBatchError::InputParticleSizeOutsideOperatingRange {
                required: concentration_particle_size(),
                found: coarse,
            },
        )),
    );
    let unsorted_cleaning = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_CLEAN_GALENA_CONCENTRATE,
            fixture.source,
            &selected,
            fixture.separator,
            fixture.energy,
        ),
    );
    assert_eq!(
        unsorted_cleaning.err(),
        Some(ConstituentSeparationResolutionError::Batch(
            ConstituentSeparationBatchError::InputFormMismatch {
                expected: FORM_CONCENTRATE,
                found: FORM_TAILINGS,
            },
        )),
    );
    assert_eq!(fixture.state, before_rejection);
    let matter_before = calculate_matter_accounting(&fixture.state).unwrap().total();

    let selection = [MaterialLotSelection::new(fixture.lot, mass)];
    let reground = resolve_comminution_process(
        &fixture.registries,
        &fixture.state,
        ComminutionRequest::new(
            PROCESS_REGRIND_GALENA_PICKING_TAILINGS,
            fixture.source,
            &selection,
            quern,
            fixture.energy,
        ),
    )
    .unwrap();
    assert!(reground.required_energy() > Energy::ZERO);
    let feed = &reground.process_resolution().outputs()[0];
    assert_eq!(feed.mass(), mass);
    assert_eq!(feed.composition(), &composition);
    assert_eq!(
        feed.commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS)
    );
    assert_eq!(feed.particle_size(), Some(concentration_particle_size()));
    let ticks = reground.process_resolution().duration().value();
    validate_start_process(
        &fixture.registries,
        &fixture.state,
        reground.process_resolution(),
        fixture.source,
        fixture.target,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    for _ in 0..ticks {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    }

    let scavenger_selection = fixture
        .state
        .inventory()
        .lot_ids(fixture.target)
        .map(|id| {
            let lot = fixture.state.inventory().get_lot(id).unwrap();
            MaterialLotSelection::new(id, lot.mass())
        })
        .collect::<Vec<_>>();
    let scavenged = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_SCAVENGE_GALENA_TAILINGS,
            fixture.target,
            &scavenger_selection,
            fixture.separator,
            fixture.energy,
        ),
    )
    .unwrap();
    assert_eq!(scavenged.target_mass().milligrams(), 21_600);
    assert_eq!(scavenged.residue_mass().milligrams(), 26_400);
    let concentrate = &scavenged.process_resolution().output_streams()[0];
    assert!(
        concentrate
            .outputs()
            .iter()
            .all(|lot| { lot.commodity() == CommodityKey::new(MATERIAL_GALENA, FORM_CONCENTRATE) })
    );
    // Gravity alone leaves 2.4 g stone in 21.6 g concentrate, so this output
    // cannot yet be mistaken for pure PbS ready for precipitation.
    let stone_in_concentrate = concentrate
        .outputs()
        .iter()
        .map(|lot| {
            u128::from(lot.mass().milligrams())
                * u128::from(lot.composition().parts_per_million(MATERIAL_STONE))
        })
        .sum::<u128>();
    assert_eq!(stone_in_concentrate, 2_400_u128 * 1_000_000);
    let ticks = scavenged.process_resolution().duration().value();
    validate_start_process_routed(
        &fixture.registries,
        &fixture.state,
        scavenged.process_resolution(),
        fixture.target,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                fixture.source,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                fixture.residue,
            ),
        ],
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    for _ in 0..ticks {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    }

    let cleaning_selection = fixture
        .state
        .inventory()
        .lot_ids(fixture.source)
        .map(|id| {
            let lot = fixture.state.inventory().get_lot(id).unwrap();
            MaterialLotSelection::new(id, lot.mass())
        })
        .collect::<Vec<_>>();
    let cleaned = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_CLEAN_GALENA_CONCENTRATE,
            fixture.source,
            &cleaning_selection,
            fixture.separator,
            fixture.energy,
        ),
    )
    .unwrap();
    assert_eq!(cleaned.target_mass().milligrams(), 17_280);
    assert_eq!(cleaned.residue_mass().milligrams(), 4_320);
    for lot in cleaned.process_resolution().output_streams()[0].outputs() {
        assert_eq!(
            lot.commodity(),
            CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)
        );
        assert_eq!(
            lot.composition(),
            &MaterialComposition::pure(MATERIAL_GALENA)
        );
        assert_eq!(lot.particle_size(), Some(concentration_particle_size()));
    }
    let ticks = cleaned.process_resolution().duration().value();
    validate_start_process_routed(
        &fixture.registries,
        &fixture.state,
        cleaned.process_resolution(),
        fixture.source,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                fixture.target,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                fixture.residue,
            ),
        ],
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
    for _ in 0..ticks {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
        let _ = advance_tick(&fixture.registries, &mut resumed).unwrap();
    }
    assert_eq!(fixture.state, resumed);
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.target)
            .unwrap()
            .stored_mass()
            .milligrams(),
        17_280
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .unwrap()
            .stored_mass()
            .milligrams(),
        30_720
    );
    assert_eq!(
        calculate_matter_accounting(&fixture.state).unwrap().total(),
        matter_before
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}
