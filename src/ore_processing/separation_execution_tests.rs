//! Tests for exact constituent separation; fixtures use built-in primitive content where possible.

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, EQUIPMENT_STONE_SEPARATOR, FORM_CONCENTRATE, FORM_CRUSHED,
    FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_STONE, PROCESS_CONCENTRATE_COPPER,
    PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};
use crate::core::quantity::{Energy, Length, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::add_energy_store_with_initial_for_test;
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
};
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialLotSpec, ParticleSizeRange,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessOutputRoute, validate_start_process_routed};
use crate::simulation::advance_tick;

const TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);

fn liberated_particle_size() -> ParticleSizeRange {
    ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(10_000),
    )
    .unwrap_or_else(|error| panic!("separation particle-size fixture failed: {error}"))
}

fn copper_stone_composition(copper_ppm: u32) -> MaterialComposition {
    MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - copper_ppm),
    ])
    .unwrap_or_else(|error| panic!("separation composition fixture failed: {error}"))
}

struct Fixture {
    registries: Registries,
    state: AppState,
    source: StockpileId,
    target: StockpileId,
    residue: StockpileId,
    lot: crate::inventory::MaterialLotId,
    separator: EquipmentId,
    energy: EnergyStoreId,
}

fn fixture(mass: Mass, composition: MaterialComposition) -> Fixture {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x9720_0001));
    let source_capacity = mass
        .checked_add(Mass::from_milligrams(1))
        .unwrap_or_else(|| panic!("separation source fixture capacity overflowed"));
    let source = add_solid_stockpile_for_test(&mut state, source_capacity)
        .unwrap_or_else(|error| panic!("separation source fixture failed: {error}"));
    let target = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("separation target fixture failed: {error}"));
    let residue = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("separation residue fixture failed: {error}"));
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        mass,
        TEMPERATURE,
        composition,
        liberated_particle_size(),
    )
    .unwrap_or_else(|error| panic!("separation input specification failed: {error}"));
    let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
        .unwrap_or_else(|error| panic!("separation lot fixture failed: {error}"));
    let assembly_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_SEPARATOR)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("separation equipment lost its authored assembly profile"));
    let assembly_mass = assembly_profile
        .inputs()
        .iter()
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("separation assembly mass overflowed"));
    let assembly = add_solid_stockpile_for_test(&mut state, assembly_mass)
        .unwrap_or_else(|error| panic!("separation assembly stockpile failed: {error}"));
    for input in assembly_profile.inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            assembly,
            input.commodity(),
            input.mass(),
            TEMPERATURE,
        )
        .unwrap_or_else(|error| panic!("separation assembly material failed: {error}"));
    }
    let separator =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_SEPARATOR, assembly)
            .unwrap_or_else(|error| panic!("separation equipment assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("separation equipment assembly commit failed: {error}"));
    let energy = add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(500_000_000_000),
    )
    .unwrap_or_else(|error| panic!("separation energy fixture failed: {error}"));
    Fixture {
        registries,
        state,
        source,
        target,
        residue,
        lot,
        separator,
        energy,
    }
}

fn resolve_process(
    fixture: &Fixture,
    process: ProcessId,
    mass: Mass,
) -> ResolvedConstituentSeparation {
    resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            process,
            fixture.source,
            &[MaterialLotSelection::new(fixture.lot, mass)],
            fixture.separator,
            fixture.energy,
        ),
    )
    .unwrap_or_else(|error| panic!("separation resolution failed: {error}"))
}

fn resolve(fixture: &Fixture, mass: Mass) -> ResolvedConstituentSeparation {
    resolve_process(fixture, PROCESS_SEPARATE_NATIVE_COPPER, mass)
}

#[test]
fn constituent_separation_turns_liberated_mixed_ore_into_exact_pure_streams() {
    let mass = Mass::from_milligrams(100_000);
    let mut fixture = fixture(mass, copper_stone_composition(600_000));
    let matter_before = calculate_matter_accounting(&fixture.state)
        .unwrap_or_else(|error| panic!("separation matter-before audit failed: {error}"));
    let resolved = resolve(&fixture, mass);

    assert_eq!(resolved.target_mass(), Mass::from_milligrams(60_000));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(40_000));
    let duration = resolved.process_resolution().duration();
    assert_eq!(duration, TickSpan::new(10));
    assert_eq!(resolved.condition_before(), Condition::PRISTINE);
    assert_eq!(
        resolved.condition_after(),
        Condition::new(998_500)
            .unwrap_or_else(|error| panic!("separation expected condition failed: {error}"))
    );
    let streams = resolved.process_resolution().output_streams();
    assert_eq!(streams.len(), 2);
    assert_eq!(
        streams[0].id(),
        ConstituentSeparationProcessDefinition::TARGET_STREAM
    );
    assert_eq!(
        streams[0].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)
    );
    assert_eq!(
        streams[0].outputs()[0].composition(),
        &MaterialComposition::pure(MATERIAL_COPPER)
    );
    assert_eq!(
        streams[1].id(),
        ConstituentSeparationProcessDefinition::RESIDUE_STREAM
    );
    assert_eq!(
        streams[1].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED)
    );
    assert_eq!(
        streams[1].outputs()[0].composition(),
        &MaterialComposition::pure(MATERIAL_STONE)
    );
    assert_eq!(
        streams[1].outputs()[0].particle_size(),
        Some(liberated_particle_size()),
        "separation residue must retain crushed feed particle size instead of re-agglomerating for free"
    );

    validate_start_process_routed(
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
    .unwrap_or_else(|error| panic!("separation start validation failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("separation start commit failed: {error}"));
    for _ in 0..duration.value() {
        advance_tick(&fixture.registries, &mut fixture.state)
            .unwrap_or_else(|error| panic!("separation completion tick failed: {error}"));
    }

    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.target)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
            }),
        Some(Mass::from_milligrams(60_000))
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED))),
        Some(Mass::from_milligrams(40_000))
    );
    assert_eq!(
        calculate_matter_accounting(&fixture.state)
            .unwrap_or_else(|error| panic!("separation matter-after audit failed: {error}"))
            .total(),
        matter_before.total()
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}

#[test]
fn constituent_separation_preserves_fractional_target_in_mixed_residue_boundary() {
    let mass = Mass::from_milligrams(3);
    let fixture = fixture(mass, copper_stone_composition(500_000));
    let resolved = resolve(&fixture, mass);

    assert_eq!(resolved.target_mass(), Mass::from_milligrams(1));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(2));
    let streams = resolved.process_resolution().output_streams();
    let target_outputs = streams
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::TARGET_STREAM)
        .unwrap_or_else(|| panic!("fractional separation target stream disappeared"))
        .outputs();
    let residue_outputs = streams
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::RESIDUE_STREAM)
        .unwrap_or_else(|| panic!("fractional separation residue stream disappeared"))
        .outputs();
    assert_eq!(target_outputs.len(), 1);
    assert_eq!(target_outputs[0].mass(), Mass::from_milligrams(1));
    assert_eq!(
        residue_outputs
            .iter()
            .map(|output| output.mass().milligrams())
            .sum::<u64>(),
        2
    );
    assert!(residue_outputs.iter().any(|output| {
        output.mass() == Mass::from_milligrams(1)
            && output.composition().parts_per_million(MATERIAL_COPPER) == 500_000
            && output.composition().parts_per_million(MATERIAL_STONE) == 500_000
    }));
    let represented_copper_ppm_mg = target_outputs
        .iter()
        .chain(residue_outputs)
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert_eq!(represented_copper_ppm_mg, 3_u128 * 500_000_u128);
}

#[test]
fn constituent_separation_keeps_sub_resolution_group_target_in_residue_without_blocking_batch() {
    let primary_mass = Mass::from_milligrams(2);
    let composition = copper_stone_composition(500_000);
    let mut fixture = fixture(primary_mass, composition.clone());
    let trace_mass = Mass::from_milligrams(1);
    let trace_temperature = Temperature::from_millikelvin(310_000);
    let trace = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        trace_mass,
        trace_temperature,
        composition.clone(),
        liberated_particle_size(),
    )
    .unwrap_or_else(|error| panic!("separation trace input specification failed: {error}"));
    let trace_lot = deposit_lot_spec_for_test(
        &fixture.registries,
        &mut fixture.state,
        fixture.source,
        trace,
    )
    .unwrap_or_else(|error| panic!("separation trace lot fixture failed: {error}"));

    let resolved = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_SEPARATE_NATIVE_COPPER,
            fixture.source,
            &[
                MaterialLotSelection::new(fixture.lot, primary_mass),
                MaterialLotSelection::new(trace_lot, trace_mass),
            ],
            fixture.separator,
            fixture.energy,
        ),
    )
    .unwrap_or_else(|error| panic!("mixed-resolution separation should resolve: {error}"));

    assert_eq!(resolved.target_mass(), Mass::from_milligrams(1));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(2));
    let streams = resolved.process_resolution().output_streams();
    let target = streams
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::TARGET_STREAM)
        .unwrap_or_else(|| panic!("mixed-resolution separation target stream disappeared"));
    let residue = streams
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::RESIDUE_STREAM)
        .unwrap_or_else(|| panic!("mixed-resolution separation residue stream disappeared"));
    assert_eq!(target.outputs().len(), 1);
    assert!(residue.outputs().iter().any(|output| {
        output.temperature() == trace_temperature
            && output.mass() == trace_mass
            && output.composition() == &composition
    }));
    let represented_copper_ppm_mg = target
        .outputs()
        .iter()
        .chain(residue.outputs())
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert_eq!(represented_copper_ppm_mg, 3_u128 * 500_000_u128);
}

#[test]
fn constituent_separation_rejects_batch_with_no_whole_milligram_target_recovery() {
    let mass = Mass::from_milligrams(1);
    let fixture = fixture(mass, copper_stone_composition(500_000));
    let before = fixture.state.clone();

    assert_eq!(
        resolve_constituent_separation_process(
            &fixture.registries,
            &fixture.state,
            ConstituentSeparationRequest::new(
                PROCESS_SEPARATE_NATIVE_COPPER,
                fixture.source,
                &[MaterialLotSelection::new(fixture.lot, mass)],
                fixture.separator,
                fixture.energy,
            ),
        )
        .err(),
        Some(ConstituentSeparationResolutionError::Batch(
            ConstituentSeparationBatchError::TargetBelowMassResolution {
                material: MATERIAL_COPPER,
                selected: mass,
            }
        ))
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn constituent_separation_rejects_unmodeled_third_constituent_without_mutation() {
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 600_000),
        CompositionComponent::new(MATERIAL_STONE, 300_000),
        CompositionComponent::new(MATERIAL_SLAG, 100_000),
    ])
    .unwrap_or_else(|error| panic!("third-constituent fixture failed: {error}"));
    let fixture = fixture(Mass::from_milligrams(100_000), composition);
    let before = fixture.state.clone();

    let error = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_SEPARATE_NATIVE_COPPER,
            fixture.source,
            &[MaterialLotSelection::new(
                fixture.lot,
                Mass::from_milligrams(100_000),
            )],
            fixture.separator,
            fixture.energy,
        ),
    )
    .err()
    .unwrap_or_else(|| panic!("unmodeled third constituent unexpectedly resolved"));
    assert_eq!(
        error,
        ConstituentSeparationResolutionError::Batch(
            ConstituentSeparationBatchError::UnsupportedConstituent {
                material: MATERIAL_SLAG,
            }
        )
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn concentration_accepts_multiple_gangue_constituents_without_losing_composition() {
    let mass = Mass::from_milligrams(7);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 400_000),
        CompositionComponent::new(MATERIAL_STONE, 350_000),
        CompositionComponent::new(MATERIAL_SLAG, 250_000),
    ])
    .unwrap_or_else(|error| panic!("concentration composition fixture failed: {error}"));
    let fixture = fixture(mass, composition.clone());
    let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_COPPER, mass);

    assert_eq!(resolved.target_mass(), Mass::from_milligrams(2));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(5));
    let target = resolved
        .process_resolution()
        .output_streams()
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::TARGET_STREAM)
        .unwrap_or_else(|| panic!("concentration target stream disappeared"));
    let residue = resolved
        .process_resolution()
        .output_streams()
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::RESIDUE_STREAM)
        .unwrap_or_else(|| panic!("concentration residue stream disappeared"));
    assert_eq!(target.outputs().len(), 1);
    assert_eq!(
        target.outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE)
    );
    assert_eq!(
        target.outputs()[0].composition(),
        &MaterialComposition::pure(MATERIAL_COPPER)
    );
    assert!(residue.outputs().iter().all(|output| {
        output.commodity().form() == FORM_CRUSHED
            && output.commodity().material() != MATERIAL_COPPER
            && output.particle_size() == Some(liberated_particle_size())
    }));
    let all_outputs = target.outputs().iter().chain(residue.outputs());
    for material in [MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_SLAG] {
        let represented = all_outputs
            .clone()
            .map(|output| {
                u128::from(output.mass().milligrams())
                    * u128::from(output.composition().parts_per_million(material))
            })
            .sum::<u128>();
        assert_eq!(
            represented,
            u128::from(mass.milligrams()) * u128::from(composition.parts_per_million(material)),
            "concentration must conserve exact represented constituent content"
        );
    }
}

#[test]
fn concentration_requires_actual_gangue_instead_of_relabeling_pure_target() {
    let mass = Mass::from_milligrams(10);
    let fixture = fixture(mass, MaterialComposition::pure(MATERIAL_COPPER));
    let before = fixture.state.clone();

    assert_eq!(
        resolve_constituent_separation_process(
            &fixture.registries,
            &fixture.state,
            ConstituentSeparationRequest::new(
                PROCESS_CONCENTRATE_COPPER,
                fixture.source,
                &[MaterialLotSelection::new(fixture.lot, mass)],
                fixture.separator,
                fixture.energy,
            ),
        )
        .err(),
        Some(ConstituentSeparationResolutionError::Batch(
            ConstituentSeparationBatchError::MissingNonTargetConstituent
        ))
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn persisted_concentration_replays_multi_gangue_outputs() {
    let mass = Mass::from_milligrams(100_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 410_000),
        CompositionComponent::new(MATERIAL_STONE, 370_000),
        CompositionComponent::new(MATERIAL_SLAG, 220_000),
    ])
    .unwrap_or_else(|error| panic!("persisted concentration composition failed: {error}"));
    let mut fixture = fixture(mass, composition);
    let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_COPPER, mass);
    validate_start_process_routed(
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
    .unwrap_or_else(|error| panic!("persisted concentration start validation failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("persisted concentration start commit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| panic!("concentration serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("concentration deserialization failed: {error}"));
    let loaded = decoded
        .into_state(&fixture.registries)
        .unwrap_or_else(|error| panic!("concentration load validation failed: {error}"));
    assert_eq!(loaded, fixture.state);
}

#[test]
fn persisted_constituent_separation_replays_outputs_and_rejects_forgery() {
    let mass = Mass::from_milligrams(100_000);
    let mut fixture = fixture(mass, copper_stone_composition(600_000));
    let resolved = resolve(&fixture, mass);
    let job = validate_start_process_routed(
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
    .unwrap_or_else(|error| panic!("persisted separation start validation failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("persisted separation start commit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| panic!("separation serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("separation deserialization failed: {error}"));
    let loaded = decoded
        .into_state(&fixture.registries)
        .unwrap_or_else(|error| panic!("separation load validation failed: {error}"));
    assert_eq!(loaded, fixture.state);

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| panic!("separation tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [0]["outputs"][0]["mass"] = serde_json::json!(59_999_u64);
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [1]["outputs"][0]["mass"] = serde_json::json!(40_001_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("separation tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&fixture.registries),
        Err(LoadError::InvalidState(
            StateValidationError::ConstituentSeparationJob(
                ConstituentSeparationJobValidationError::OutputMismatch { job }
            )
        ))
    );
}
