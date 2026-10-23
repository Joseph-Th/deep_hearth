//! Primitive lead-sulfide recovery before the no-offgas iron displacement reaction.

use super::*;
use crate::content::{MATERIAL_GALENA, PROCESS_HAND_PICK_GALENA};

#[test]
fn coarse_galena_hand_picking_prepares_pure_furnace_charge_and_preserves_lost_sulfide() {
    let mass = Mass::from_milligrams(100_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_GALENA, 800_000),
        CompositionComponent::new(MATERIAL_STONE, 200_000),
    ])
    .unwrap();
    let mut fixture = unadmitted_manual_fixture_with_host(MATERIAL_GALENA, mass, composition);
    admit_manual_separation_fixture(&mut fixture, &[]);
    let definition = fixture
        .registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_PICK_GALENA)
        .unwrap();
    assert_eq!(definition.target_recovery_ppm(), 650_000);
    assert_eq!(
        definition.maximum_homogeneous_target_recovery_from_feed(mass, 800_000),
        Some(Mass::from_milligrams(52_000)),
    );
    let selection = [MaterialLotSelection::new(fixture.lot, mass)];
    let resolved = resolve_manual_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ManualConstituentSeparationRequest::new(
            PROCESS_HAND_PICK_GALENA,
            fixture.source,
            &selection,
        ),
    )
    .unwrap();
    assert_eq!(resolved.target_mass().milligrams(), 52_000);
    assert_eq!(resolved.residue_mass().milligrams(), 48_000);
    let streams = resolved.process_resolution().output_streams();
    assert_eq!(streams.len(), 2);
    assert_eq!(streams[0].outputs().len(), 1);
    let prills = &streams[0].outputs()[0];
    assert_eq!(
        prills.commodity(),
        CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)
    );
    assert_eq!(
        prills.composition(),
        &MaterialComposition::pure(MATERIAL_GALENA)
    );
    assert_eq!(prills.mass().milligrams(), 52_000);
    let tailings = streams[1].outputs();
    assert!(
        tailings
            .iter()
            .all(|lot| { lot.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS) })
    );
    for (material, constituent_mg) in [
        (MATERIAL_GALENA, 28_000_u128),
        (MATERIAL_STONE, 20_000_u128),
    ] {
        let total = tailings
            .iter()
            .map(|lot| {
                u128::from(lot.mass().milligrams())
                    * u128::from(lot.composition().parts_per_million(material))
            })
            .sum::<u128>();
        assert_eq!(total, constituent_mg * 1_000_000);
    }
    let before = calculate_matter_accounting(&fixture.state).unwrap().total();
    let survival_before = assess_survival(&fixture.registries, &fixture.state).unwrap();
    validate_start_manual_constituent_separation(
        &fixture.registries,
        &fixture.state,
        &resolved,
        fixture.source,
        fixture.target,
        fixture.residue,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    for _ in 0..14 {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    }
    let save = serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    for _ in 14..resolved.duration().value() {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
        let _ = advance_tick(&fixture.registries, &mut resumed).unwrap();
    }
    assert_eq!(fixture.state, resumed);
    assert_eq!(fixture.state.player_work().active(), None);
    let selected = fixture
        .state
        .inventory()
        .lot_ids(fixture.target)
        .next()
        .unwrap();
    let galena = fixture.state.inventory().get_lot(selected).unwrap();
    assert_eq!(
        galena.commodity(),
        CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)
    );
    assert_eq!(
        galena.composition(),
        &MaterialComposition::pure(MATERIAL_GALENA)
    );
    assert_eq!(galena.mass().milligrams(), 52_000);
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .unwrap()
            .stored_mass()
            .milligrams(),
        48_000
    );
    assert_eq!(
        calculate_matter_accounting(&fixture.state).unwrap().total(),
        before
    );
    let survival_after = assess_survival(&fixture.registries, &fixture.state).unwrap();
    assert!(survival_after.metabolic_energy() < survival_before.metabolic_energy());
    assert!(survival_after.hydration() < survival_before.hydration());
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}
