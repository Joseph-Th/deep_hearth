//! GregTech-inspired mineral dressing with physical gangue and distinct energy carriers.

use super::*;

use crate::capability::CapabilityEvaluationError;
use crate::content::{
    EQUIPMENT_COPPER_WOUND_MAGNETIC_SEPARATOR, MATERIAL_CASSITERITE, MATERIAL_MAGNETITE,
    PROCESS_CONCENTRATE_CASSITERITE, PROCESS_MAGNETIC_CONCENTRATE_MAGNETITE,
};
use crate::ore_processing::ConstituentSeparationProcessDefinition;

fn mixed_mineral(mineral: crate::material::MaterialId) -> MaterialComposition {
    MaterialComposition::new(vec![
        CompositionComponent::new(mineral, 400_000),
        CompositionComponent::new(MATERIAL_STONE, 600_000),
    ])
    .unwrap()
}

fn assemble_magnetic_separator(fixture: &mut Fixture) -> EquipmentId {
    let definition = fixture
        .registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_WOUND_MAGNETIC_SEPARATOR)
        .unwrap();
    let assembly_profile = definition.assembly_profile().unwrap();
    let assembly =
        add_solid_stockpile_for_test(&mut fixture.state, assembly_profile.input_mass()).unwrap();
    for input in assembly_profile.inputs() {
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
    validate_assemble_equipment(
        &fixture.registries,
        &fixture.state,
        EQUIPMENT_COPPER_WOUND_MAGNETIC_SEPARATOR,
        assembly,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap()
}

#[test]
fn magnetic_separation_needs_a_real_wound_machine_and_electricity_and_preserves_minerals() {
    let batch = Mass::from_milligrams(100_000);
    let mut fixture = fixture_with_host_and_particle_size(
        MATERIAL_MAGNETITE,
        batch,
        mixed_mineral(MATERIAL_MAGNETITE),
        concentration_particle_size(),
    );
    let machine = assemble_magnetic_separator(&mut fixture);
    let magnetic_flow = fixture
        .registries
        .ore_processing()
        .get_constituent_separation(PROCESS_MAGNETIC_CONCENTRATE_MAGNETITE)
        .unwrap()
        .mass_flow_capability();
    let selection = [MaterialLotSelection::new(fixture.lot, batch)];
    let request = |machine, energy_store| {
        ConstituentSeparationRequest::new(
            PROCESS_MAGNETIC_CONCENTRATE_MAGNETITE,
            fixture.source,
            &selection,
            machine,
            energy_store,
        )
    };
    // A rocking/gravity separator cannot secretly execute magnetic beneficiation.
    assert_eq!(
        resolve_constituent_separation_process(
            &fixture.registries,
            &fixture.state,
            request(fixture.separator, fixture.energy),
        )
        .err(),
        Some(ConstituentSeparationResolutionError::Capability(
            CapabilityEvaluationError::MissingCapability {
                capability: magnetic_flow,
            }
        )),
    );
    let before = fixture.state.clone();
    assert_eq!(
        resolve_constituent_separation_process(
            &fixture.registries,
            &fixture.state,
            request(machine, fixture.energy),
        )
        .err(),
        Some(ConstituentSeparationResolutionError::WrongEnergyCarrier {
            required: crate::energy::EnergyCarrier::Electrical,
            provided: crate::energy::EnergyCarrier::Mechanical,
        })
    );
    assert_eq!(fixture.state, before);

    let electricity = add_energy_store_with_initial_for_fixture(
        &fixture.registries,
        &mut fixture.state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::from_nanojoules(500_000_000_000),
    )
    .unwrap();
    let resolved = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_MAGNETIC_CONCENTRATE_MAGNETITE,
            fixture.source,
            &[MaterialLotSelection::new(fixture.lot, batch)],
            machine,
            electricity,
        ),
    )
    .unwrap();
    assert_eq!(resolved.target_mass(), Mass::from_milligrams(41_000));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(59_000));
    let streams = resolved.process_resolution().output_streams();
    assert_eq!(
        streams[0].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_MAGNETITE, FORM_CONCENTRATE)
    );
    assert_eq!(
        streams[1].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS)
    );
    // The input's 40,000 mg magnetite and 60,000 mg stone remain separate
    // constituent masses across the two enriched/depleted output streams.
    for material in [MATERIAL_MAGNETITE, MATERIAL_STONE] {
        let represented = streams
            .iter()
            .flat_map(|stream| stream.outputs())
            .map(|output| {
                u128::from(output.mass().milligrams())
                    * u128::from(output.composition().parts_per_million(material))
            })
            .sum::<u128>();
        let expected = if material == MATERIAL_MAGNETITE {
            40_000
        } else {
            60_000
        };
        assert_eq!(represented, expected * 1_000_000);
    }
    let matter_before = calculate_matter_accounting(&fixture.state).unwrap().total();
    let duration = resolved.process_resolution().duration();
    let _job = validate_start_process_routed(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
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
    let bytes =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    for _ in 0..duration.value() {
        assert_eq!(
            advance_tick(&fixture.registries, &mut fixture.state).unwrap(),
            advance_tick(&fixture.registries, &mut restored).unwrap(),
        );
    }
    assert_eq!(fixture.state, restored);
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.target)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(41_000)
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(59_000)
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

#[test]
fn gravity_dressing_makes_cassiterite_concentrate_not_elemental_tin() {
    let batch = Mass::from_milligrams(100_000);
    let fixture = fixture_with_host_and_particle_size(
        MATERIAL_CASSITERITE,
        batch,
        mixed_mineral(MATERIAL_CASSITERITE),
        concentration_particle_size(),
    );
    let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_CASSITERITE, batch);
    assert_eq!(resolved.target_mass(), Mass::from_milligrams(43_000));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(57_000));
    assert_eq!(
        resolved.process_resolution().output_streams()[0].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_CASSITERITE, FORM_CONCENTRATE),
    );
    assert_eq!(
        resolved.process_resolution().output_streams()[1].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS),
    );
}
