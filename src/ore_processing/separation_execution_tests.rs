//! Tests for exact constituent separation; fixtures use built-in primitive content where possible.

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, EQUIPMENT_STONE_SEPARATOR, FORM_CONCENTRATE, FORM_CRUSHED,
    FORM_NATIVE_METAL, FORM_TAILINGS, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_SLAG,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_CONCENTRATE_COPPER, PROCESS_HAND_SORT_NATIVE_COPPER,
    PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};
use crate::core::quantity::{Energy, Length, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
};
use crate::labor::PlayerWork;
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialLotSpec, ParticleSizeRange,
};
use crate::matter::calculate_matter_accounting;
use crate::ore_processing::ManualConstituentSeparationProcessDefinition;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessOutputRoute, StartProcessError, validate_start_process_routed};
use crate::simulation::advance_tick;
use crate::survival::{assess_survival, initialize_player_survival};

const TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);

fn liberated_particle_size() -> ParticleSizeRange {
    ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(10_000),
    )
    .unwrap_or_else(|error| panic!("separation particle-size fixture failed: {error}"))
}

fn hand_sortable_particle_size() -> ParticleSizeRange {
    ParticleSizeRange::new(
        Length::from_micrometers(2_000),
        Length::from_micrometers(10_000),
    )
    .unwrap_or_else(|error| panic!("hand-sortable particle-size fixture failed: {error}"))
}

fn concentration_particle_size() -> ParticleSizeRange {
    ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(2_000),
    )
    .unwrap_or_else(|error| panic!("concentration particle-size fixture failed: {error}"))
}

#[test]
fn concentration_recovery_is_invariant_to_feed_assay_lot_fragmentation() {
    let mut fixture =
        concentration_fixture(Mass::from_milligrams(1), copper_stone_composition(700_000));
    let second_input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        Mass::from_milligrams(1),
        TEMPERATURE,
        copper_stone_composition(500_000),
        concentration_particle_size(),
    )
    .unwrap_or_else(|error| panic!("second concentration input specification failed: {error}"));
    let second_lot = deposit_lot_spec_for_test(
        &fixture.registries,
        &mut fixture.state,
        fixture.source,
        second_input,
    )
    .unwrap_or_else(|error| panic!("second concentration input lot failed: {error}"));

    let resolved = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_CONCENTRATE_COPPER,
            fixture.source,
            &[
                MaterialLotSelection::new(fixture.lot, Mass::from_milligrams(1)),
                MaterialLotSelection::new(second_lot, Mass::from_milligrams(1)),
            ],
            fixture.separator,
            fixture.energy,
        ),
    )
    .unwrap_or_else(|error| panic!("fragmented concentration resolution failed: {error}"));

    assert_eq!(resolved.target_mass(), Mass::from_milligrams(1));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(1));
    let streams = resolved.process_resolution().output_streams();
    let represented_copper = streams
        .iter()
        .flat_map(|stream| stream.outputs())
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert_eq!(represented_copper, 1_200_000);
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
    fixture_with_particle_size(mass, composition, liberated_particle_size())
}

fn concentration_fixture(mass: Mass, composition: MaterialComposition) -> Fixture {
    fixture_with_particle_size(mass, composition, concentration_particle_size())
}

fn fixture_with_particle_size(
    mass: Mass,
    composition: MaterialComposition,
    particle_size: ParticleSizeRange,
) -> Fixture {
    fixture_with_host_and_particle_size(MATERIAL_COPPER, mass, composition, particle_size)
}

fn fixture_with_host_and_particle_size(
    host_material: crate::material::MaterialId,
    mass: Mass,
    composition: MaterialComposition,
    particle_size: ParticleSizeRange,
) -> Fixture {
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
        CommodityKey::new(host_material, FORM_CRUSHED),
        mass,
        TEMPERATURE,
        composition,
        particle_size,
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
    let energy = add_energy_store_with_initial_for_fixture(
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

struct ManualFixture {
    registries: Registries,
    state: AppState,
    source: StockpileId,
    target: StockpileId,
    residue: StockpileId,
    lot: crate::inventory::MaterialLotId,
}

fn manual_fixture(mass: Mass, composition: MaterialComposition) -> ManualFixture {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x9720_1001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual separation survival fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("manual separation source fixture failed: {error}"));
    let target = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("manual separation target fixture failed: {error}"));
    let residue = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("manual separation residue fixture failed: {error}"));
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        mass,
        TEMPERATURE,
        composition,
        hand_sortable_particle_size(),
    )
    .unwrap_or_else(|error| panic!("manual separation input specification failed: {error}"));
    let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
        .unwrap_or_else(|error| panic!("manual separation input lot failed: {error}"));
    ManualFixture {
        registries,
        state,
        source,
        target,
        residue,
        lot,
    }
}

#[test]
fn hand_sorting_rejects_fine_ground_feed_that_is_no_longer_visually_sortable() {
    let mass = Mass::from_milligrams(100_000);
    let mut fixture = manual_fixture(mass, copper_stone_composition(400_000));
    let fine = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        mass,
        TEMPERATURE,
        copper_stone_composition(400_000),
        concentration_particle_size(),
    )
    .unwrap_or_else(|error| panic!("fine hand-sorting rejection fixture failed: {error}"));
    let fine_source = add_solid_stockpile_for_test(&mut fixture.state, mass)
        .unwrap_or_else(|error| panic!("fine hand-sorting source failed: {error}"));
    let fine_lot =
        deposit_lot_spec_for_test(&fixture.registries, &mut fixture.state, fine_source, fine)
            .unwrap_or_else(|error| panic!("fine hand-sorting lot failed: {error}"));
    let before = fixture.state.clone();

    assert_eq!(
        resolve_manual_constituent_separation_process(
            &fixture.registries,
            &fixture.state,
            ManualConstituentSeparationRequest::new(
                PROCESS_HAND_SORT_NATIVE_COPPER,
                fine_source,
                &[MaterialLotSelection::new(fine_lot, mass)],
            ),
        )
        .err(),
        Some(ManualConstituentSeparationResolutionError::Batch(
            ConstituentSeparationBatchError::InputParticleSizeOutsideOperatingRange {
                required: hand_sortable_particle_size(),
                found: concentration_particle_size(),
            }
        ))
    );
    assert_eq!(fixture.state, before);
}

fn resolve_manual(fixture: &ManualFixture, mass: Mass) -> ResolvedManualConstituentSeparation {
    resolve_manual_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ManualConstituentSeparationRequest::new(
            PROCESS_HAND_SORT_NATIVE_COPPER,
            fixture.source,
            &[MaterialLotSelection::new(fixture.lot, mass)],
        ),
    )
    .unwrap_or_else(|error| panic!("manual separation resolution failed: {error}"))
}

#[test]
fn hand_sorting_is_a_conserved_survival_costed_fallback_that_powered_sorting_materially_improves() {
    let mass = Mass::from_milligrams(100_000);
    let composition = copper_stone_composition(400_000);
    let mut manual = manual_fixture(mass, composition.clone());
    let powered = fixture(mass, composition);
    let resolved = resolve_manual(&manual, mass);
    let powered_resolved = resolve(&powered, mass);
    let manual_definition = manual
        .registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual native-copper sorting definition disappeared"));
    let powered_definition = manual
        .registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("powered native-copper sorting definition disappeared"));

    assert_eq!(manual_definition.target_recovery_ppm(), 650_000);
    assert_eq!(powered_definition.target_recovery_ppm(), 900_000);
    assert_eq!(resolved.target_mass(), Mass::from_milligrams(26_000));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(74_000));
    assert_eq!(
        powered_resolved.target_mass(),
        Mass::from_milligrams(36_000)
    );
    assert!(powered_resolved.processing_rate() > resolved.processing_rate());
    assert_eq!(resolved.duration(), TickSpan::new(56));

    let routes = [
        ProcessOutputRoute::new(
            ManualConstituentSeparationProcessDefinition::TARGET_STREAM,
            manual.target,
        ),
        ProcessOutputRoute::new(
            ManualConstituentSeparationProcessDefinition::RESIDUE_STREAM,
            manual.residue,
        ),
    ];
    assert_eq!(
        validate_start_process_routed(
            &manual.registries,
            &manual.state,
            resolved.process_resolution(),
            manual.source,
            &routes,
        )
        .err(),
        Some(StartProcessError::ManualProcessRequiresPlayerWork {
            process: PROCESS_HAND_SORT_NATIVE_COPPER,
        })
    );
    let matter_before = calculate_matter_accounting(&manual.state)
        .unwrap_or_else(|error| panic!("manual separation initial matter audit failed: {error}"))
        .total();
    let survival_before = assess_survival(&manual.registries, &manual.state)
        .unwrap_or_else(|| panic!("manual separation survival state disappeared"));
    let duration = resolved.duration();
    let job = validate_start_manual_constituent_separation(
        &manual.registries,
        &manual.state,
        &resolved,
        manual.source,
        manual.target,
        manual.residue,
    )
    .unwrap_or_else(|error| panic!("manual separation start failed: {error}"))
    .commit(&mut manual.state)
    .unwrap_or_else(|error| panic!("manual separation commit failed: {error}"));
    assert_eq!(
        manual.state.player_work().active(),
        Some(PlayerWork::ManualProduction { job })
    );
    let job_record = manual
        .state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("manual separation job disappeared after start"));
    assert_eq!(job_record.consumed_energy(), None);
    assert_eq!(job_record.released_energy(), None);
    assert_eq!(job_record.equipment_provider(), None);
    assert_eq!(
        manual
            .state
            .inventory()
            .get_stockpile(manual.target)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );

    for _ in 0..duration.value() {
        advance_tick(&manual.registries, &mut manual.state)
            .unwrap_or_else(|error| panic!("manual separation tick failed: {error}"));
    }
    assert_eq!(manual.state.player_work().active(), None);
    assert_eq!(
        manual
            .state
            .inventory()
            .get_stockpile(manual.target)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(26_000))
    );
    assert_eq!(
        manual
            .state
            .inventory()
            .get_stockpile(manual.residue)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(74_000))
    );
    assert_eq!(
        calculate_matter_accounting(&manual.state)
            .unwrap_or_else(|error| panic!("manual separation final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    let survival_after = assess_survival(&manual.registries, &manual.state)
        .unwrap_or_else(|| panic!("manual separation final survival state disappeared"));
    assert!(survival_after.metabolic_energy() < survival_before.metabolic_energy());
    assert!(survival_after.hydration() < survival_before.hydration());
    validate_loaded_state(&manual.registries, &manual.state)
        .unwrap_or_else(|error| panic!("manual separation final state audit failed: {error}"));
}

#[test]
fn hand_sorting_enforces_a_small_attention_bounded_batch() {
    let mass = Mass::from_milligrams(200_001);
    let fixture = manual_fixture(mass, copper_stone_composition(400_000));
    assert_eq!(
        resolve_manual_constituent_separation_process(
            &fixture.registries,
            &fixture.state,
            ManualConstituentSeparationRequest::new(
                PROCESS_HAND_SORT_NATIVE_COPPER,
                fixture.source,
                &[MaterialLotSelection::new(fixture.lot, mass)],
            ),
        )
        .err(),
        Some(
            ManualConstituentSeparationResolutionError::BatchMassExceeded {
                selected: mass,
                maximum: Mass::from_milligrams(200_000),
            }
        )
    );
}

#[test]
fn in_progress_hand_sorting_round_trip_preserves_deterministic_continuation() {
    let mass = Mass::from_milligrams(100_000);
    let mut fixture = manual_fixture(mass, copper_stone_composition(400_000));
    let resolved = resolve_manual(&fixture, mass);
    let duration = resolved.duration();
    validate_start_manual_constituent_separation(
        &fixture.registries,
        &fixture.state,
        &resolved,
        fixture.source,
        fixture.target,
        fixture.residue,
    )
    .unwrap_or_else(|error| panic!("round-trip manual separation start failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("round-trip manual separation commit failed: {error}"));

    let pre_save_ticks = 10;
    assert!(pre_save_ticks < duration.value());
    for _ in 0..pre_save_ticks {
        advance_tick(&fixture.registries, &mut fixture.state).unwrap_or_else(|error| {
            panic!("round-trip manual separation pre-save tick failed: {error}")
        });
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| {
            panic!("round-trip manual separation serialization failed: {error}")
        });
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("round-trip manual separation decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&fixture.registries)
        .unwrap_or_else(|error| panic!("round-trip manual separation load failed: {error}"));
    assert_eq!(loaded, fixture.state);

    for _ in pre_save_ticks..duration.value() {
        let expected =
            advance_tick(&fixture.registries, &mut fixture.state).unwrap_or_else(|error| {
                panic!("round-trip manual separation source tick failed: {error}")
            });
        let actual = advance_tick(&fixture.registries, &mut loaded).unwrap_or_else(|error| {
            panic!("round-trip manual separation loaded tick failed: {error}")
        });
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, fixture.state);
    validate_loaded_state(&fixture.registries, &loaded)
        .unwrap_or_else(|error| panic!("round-trip manual separation final audit failed: {error}"));
}

#[test]
fn persisted_hand_sorting_rejects_forged_fine_feed_trace() {
    let mass = Mass::from_milligrams(100_000);
    let mut fixture = manual_fixture(mass, copper_stone_composition(400_000));
    let resolved = resolve_manual(&fixture, mass);
    let job = validate_start_manual_constituent_separation(
        &fixture.registries,
        &fixture.state,
        &resolved,
        fixture.source,
        fixture.target,
        fixture.residue,
    )
    .unwrap_or_else(|error| panic!("manual sorting tamper start failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("manual sorting tamper commit failed: {error}"));
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| panic!("manual sorting tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_inputs"]
        [0]["profile"]["particle_size"]["classes"][0]["range"]["minimum_diameter"] =
        serde_json::json!(500_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("manual sorting tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&fixture.registries),
        Err(LoadError::InvalidState(
            StateValidationError::ConstituentSeparationJob(
                ConstituentSeparationJobValidationError::Batch {
                    job,
                    error:
                        ConstituentSeparationBatchError::InputParticleSizeOutsideOperatingRange {
                            required: hand_sortable_particle_size(),
                            found: liberated_particle_size(),
                        },
                }
            )
        ))
    );
}

#[test]
fn constituent_separation_recovers_pure_target_and_leaves_unrecovered_copper_in_residue() {
    let mass = Mass::from_milligrams(100_000);
    let mut fixture = fixture(mass, copper_stone_composition(600_000));
    let matter_before = calculate_matter_accounting(&fixture.state)
        .unwrap_or_else(|error| panic!("separation matter-before audit failed: {error}"));
    let resolved = resolve(&fixture, mass);

    assert_eq!(resolved.target_mass(), Mass::from_milligrams(54_000));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(46_000));
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
    assert!(
        streams[1].outputs().iter().all(|output| {
            output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED)
                && output.particle_size() == Some(liberated_particle_size())
        }),
        "sorting residue must retain its dominant gangue host and crushed particle state"
    );
    let residue_copper_ppm_mg = streams[1]
        .outputs()
        .iter()
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert_eq!(
        residue_copper_ppm_mg,
        6_000_u128 * 1_000_000_u128,
        "the authored 10% target loss must remain as recoverable copper in physical residue"
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
        Some(Mass::from_milligrams(54_000))
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(46_000))
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
fn primitive_sorting_tailings_cannot_be_repeatedly_resorted_for_asymptotic_recovery() {
    let mass = Mass::from_milligrams(100_000);
    let mut fixture = fixture(mass, copper_stone_composition(600_000));
    let resolved = resolve(&fixture, mass);
    let duration = resolved.process_resolution().duration();
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
    .unwrap_or_else(|error| panic!("tailings-loop separation start failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("tailings-loop separation commit failed: {error}"));
    for _ in 0..duration.value() {
        advance_tick(&fixture.registries, &mut fixture.state)
            .unwrap_or_else(|error| panic!("tailings-loop separation completion failed: {error}"));
    }

    let residue_lots = fixture
        .state
        .inventory()
        .lot_ids(fixture.residue)
        .collect::<Vec<_>>();
    assert!(
        !residue_lots.is_empty(),
        "finite-recovery primitive sorting must leave physical tailings"
    );
    let represented_tailings_copper = residue_lots
        .iter()
        .map(|lot| {
            let record = fixture
                .state
                .inventory()
                .get_lot(*lot)
                .unwrap_or_else(|| panic!("primitive sorting tailings lot disappeared"));
            u128::from(record.mass().milligrams())
                * u128::from(record.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert!(
        represented_tailings_copper > 0,
        "finite-recovery tailings must still contain physically represented copper"
    );
    for residue_lot in residue_lots {
        let residue_record = fixture
            .state
            .inventory()
            .get_lot(residue_lot)
            .unwrap_or_else(|| panic!("primitive sorting tailings lot disappeared"));
        assert_eq!(residue_record.commodity().material(), MATERIAL_STONE);
        assert_eq!(
            resolve_constituent_separation_process(
                &fixture.registries,
                &fixture.state,
                ConstituentSeparationRequest::new(
                    PROCESS_SEPARATE_NATIVE_COPPER,
                    fixture.residue,
                    &[MaterialLotSelection::new(
                        residue_lot,
                        residue_record.mass()
                    )],
                    fixture.separator,
                    fixture.energy,
                ),
            )
            .err(),
            Some(ConstituentSeparationResolutionError::Batch(
                ConstituentSeparationBatchError::SortingInputHostMaterialMismatch {
                    expected: MATERIAL_COPPER,
                    found: MATERIAL_STONE,
                }
            ))
        );
    }
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
    let residue_copper_ppm_mg = residue_outputs
        .iter()
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    let residue_stone_ppm_mg = residue_outputs
        .iter()
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_STONE))
        })
        .sum::<u128>();
    assert_eq!(residue_copper_ppm_mg, 500_000);
    assert_eq!(residue_stone_ppm_mg, 1_500_000);
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
    let primary_mass = Mass::from_milligrams(3);
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
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(3));
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
    assert_eq!(represented_copper_ppm_mg, 4_u128 * 500_000_u128);
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
fn constituent_sorting_preserves_mixed_gangue_as_one_physical_residue_stream() {
    let mass = Mass::from_milligrams(100_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 600_000),
        CompositionComponent::new(MATERIAL_STONE, 300_000),
        CompositionComponent::new(MATERIAL_CLAY, 100_000),
    ])
    .unwrap_or_else(|error| panic!("mixed-gangue sorting fixture failed: {error}"));
    let fixture = fixture(mass, composition.clone());
    let resolved = resolve(&fixture, mass);

    assert_eq!(
        fixture
            .registries
            .ore_processing()
            .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
            .map(ConstituentSeparationProcessDefinition::target_recovery_ppm),
        Some(900_000)
    );
    assert_eq!(resolved.target_mass(), Mass::from_milligrams(54_000));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(46_000));
    let streams = resolved.process_resolution().output_streams();
    let target = streams
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::TARGET_STREAM)
        .unwrap_or_else(|| panic!("mixed-gangue target stream disappeared"));
    let residue = streams
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::RESIDUE_STREAM)
        .unwrap_or_else(|| panic!("mixed-gangue residue stream disappeared"));
    assert_eq!(target.outputs().len(), 1);
    assert_eq!(
        target.outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)
    );
    assert!(
        residue.outputs().iter().all(|output| {
            output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED)
                && output.particle_size() == Some(liberated_particle_size())
        }),
        "mixed residue must use its dominant gangue as host while retaining crushed state"
    );
    let residue_copper_ppm_mg = residue
        .outputs()
        .iter()
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert_eq!(residue_copper_ppm_mg, 6_000_u128 * 1_000_000_u128);
    let all_outputs = target.outputs().iter().chain(residue.outputs());
    for material in [MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_CLAY] {
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
            "sorting must conserve exact represented constituent content"
        );
    }
}

#[test]
fn constituent_sorting_rejects_gangue_without_an_authored_residue_form() {
    let mass = Mass::from_milligrams(100_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 600_000),
        CompositionComponent::new(MATERIAL_WOOD, 400_000),
    ])
    .unwrap_or_else(|error| panic!("unsupported-residue fixture failed: {error}"));
    let fixture = fixture(mass, composition);
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
            ConstituentSeparationBatchError::UnsupportedResidueForm {
                material: MATERIAL_WOOD,
                form: FORM_CRUSHED,
            }
        ))
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn concentration_accepts_gangue_hosted_prepared_tailings_and_recovers_target() {
    let mass = Mass::from_milligrams(46_000);
    let composition = copper_stone_composition(130_435);
    let mut fixture = fixture_with_host_and_particle_size(
        MATERIAL_STONE,
        mass,
        composition,
        concentration_particle_size(),
    );
    let matter_before = calculate_matter_accounting(&fixture.state)
        .unwrap_or_else(|error| panic!("gangue-hosted concentration matter audit failed: {error}"))
        .total();
    let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_COPPER, mass);

    assert!(resolved.target_mass() > Mass::ZERO);
    assert!(resolved.residue_mass() > Mass::ZERO);
    let target = resolved
        .process_resolution()
        .output_streams()
        .iter()
        .find(|stream| stream.id() == ConstituentSeparationProcessDefinition::TARGET_STREAM)
        .unwrap_or_else(|| panic!("gangue-hosted concentration target stream disappeared"));
    assert!(target.outputs().iter().all(|output| {
        output.commodity().material() == MATERIAL_COPPER
            && output.commodity().form() == FORM_CONCENTRATE
    }));
    let recovered_copper_ppm_mg = target
        .outputs()
        .iter()
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert!(
        recovered_copper_ppm_mg > 0,
        "prepared gangue-hosted tailings must yield physically represented target concentrate"
    );

    let duration = resolved.process_resolution().duration();
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
    .unwrap_or_else(|error| panic!("gangue-hosted concentration start failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("gangue-hosted concentration commit failed: {error}"));
    for _ in 0..duration.value() {
        advance_tick(&fixture.registries, &mut fixture.state)
            .unwrap_or_else(|error| panic!("gangue-hosted concentration tick failed: {error}"));
    }
    assert_eq!(
        calculate_matter_accounting(&fixture.state)
            .unwrap_or_else(|error| panic!(
                "gangue-hosted concentration final audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    validate_loaded_state(&fixture.registries, &fixture.state)
        .unwrap_or_else(|error| panic!("gangue-hosted concentration state audit failed: {error}"));

    let tailings_lot = fixture
        .state
        .inventory()
        .lot_ids(fixture.residue)
        .next()
        .unwrap_or_else(|| panic!("gangue-hosted concentration produced no tailings lot"));
    let tailings = fixture
        .state
        .inventory()
        .get_lot(tailings_lot)
        .unwrap_or_else(|| panic!("gangue-hosted concentration tailings disappeared"));
    assert_eq!(tailings.commodity().form(), FORM_TAILINGS);
    assert_eq!(
        resolve_constituent_separation_process(
            &fixture.registries,
            &fixture.state,
            ConstituentSeparationRequest::new(
                PROCESS_CONCENTRATE_COPPER,
                fixture.residue,
                &[MaterialLotSelection::new(tailings_lot, tailings.mass())],
                fixture.separator,
                fixture.energy,
            ),
        )
        .err(),
        Some(ConstituentSeparationResolutionError::Batch(
            ConstituentSeparationBatchError::InputFormMismatch {
                expected: FORM_CRUSHED,
                found: FORM_TAILINGS,
            }
        )),
        "one concentration pass must produce terminal current-tier tailings instead of another identical-process feed"
    );
}

#[test]
fn concentration_rejects_coarse_crusher_feed_until_liberation_is_authored() {
    let mass = Mass::from_milligrams(1_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 400_000),
        CompositionComponent::new(MATERIAL_STONE, 350_000),
        CompositionComponent::new(MATERIAL_SLAG, 250_000),
    ])
    .unwrap_or_else(|error| panic!("coarse concentration composition failed: {error}"));
    let fixture = fixture_with_host_and_particle_size(
        MATERIAL_STONE,
        mass,
        composition,
        liberated_particle_size(),
    );
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
            ConstituentSeparationBatchError::InputParticleSizeOutsideOperatingRange {
                required: concentration_particle_size(),
                found: liberated_particle_size(),
            }
        ))
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn concentration_applies_authored_selectivity_without_losing_constituent_composition() {
    let mass = Mass::from_milligrams(1_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 400_000),
        CompositionComponent::new(MATERIAL_STONE, 350_000),
        CompositionComponent::new(MATERIAL_SLAG, 250_000),
    ])
    .unwrap_or_else(|error| panic!("concentration composition fixture failed: {error}"));
    let fixture = concentration_fixture(mass, composition.clone());
    let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_COPPER, mass);

    assert_eq!(
        fixture
            .registries
            .ore_processing()
            .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
            .map(ConstituentSeparationProcessDefinition::target_recovery_ppm),
        Some(900_000)
    );
    assert_eq!(
        fixture
            .registries
            .ore_processing()
            .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
            .map(ConstituentSeparationProcessDefinition::non_target_recovery_ppm),
        Some(200_000)
    );
    assert_eq!(resolved.target_mass(), Mass::from_milligrams(480));
    assert_eq!(resolved.residue_mass(), Mass::from_milligrams(520));
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
    assert!(target.outputs().iter().all(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE)
            && output.particle_size() == Some(concentration_particle_size())
    }));
    assert!(
        target
            .outputs()
            .iter()
            .all(|output| output.composition().pure_material().is_none()),
        "finite separator selectivity must carry physical gangue into concentrate instead of fabricating pure target"
    );
    let target_copper_ppm_mg = target
        .outputs()
        .iter()
        .map(|output| {
            u128::from(output.mass().milligrams())
                * u128::from(output.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum::<u128>();
    assert_eq!(target_copper_ppm_mg, 360_000_000);
    assert_eq!(
        target_copper_ppm_mg / u128::from(resolved.target_mass().milligrams()),
        750_000,
        "concentrate grade must emerge from feed assay and target/gangue recovery"
    );
    assert!(
        residue.outputs().iter().all(|output| {
            output.commodity().form() == FORM_TAILINGS
                && output.commodity().material() == MATERIAL_STONE
                && output.particle_size() == Some(concentration_particle_size())
        }),
        "concentration tailings commodity host must follow the dominant physical gangue instead of material-ID ordering"
    );
    assert!(
        residue
            .outputs()
            .iter()
            .all(|output| output.composition().pure_material().is_none()),
        "concentration tailings must remain blended instead of fabricating freely selectable pure gangue lots"
    );
    assert!(
        residue
            .outputs()
            .iter()
            .all(|output| output.composition().parts_per_million(MATERIAL_COPPER) != 0),
        "finite recovery must leave copper distributed through every represented tailings assay profile"
    );
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
    let fixture = concentration_fixture(mass, MaterialComposition::pure(MATERIAL_COPPER));
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
    let mut fixture = fixture_with_host_and_particle_size(
        MATERIAL_STONE,
        mass,
        composition,
        concentration_particle_size(),
    );
    let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_COPPER, mass);
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

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| panic!("concentration tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_inputs"]
        [0]["profile"]["particle_size"]["classes"][0]["range"]["maximum_diameter"] =
        serde_json::json!(10_000_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("concentration tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&fixture.registries),
        Err(LoadError::InvalidState(
            StateValidationError::ConstituentSeparationJob(
                ConstituentSeparationJobValidationError::Batch {
                    job,
                    error:
                        ConstituentSeparationBatchError::InputParticleSizeOutsideOperatingRange {
                            required: concentration_particle_size(),
                            found: liberated_particle_size(),
                        },
                }
            )
        ))
    );
}

#[cfg(feature = "test-soak")]
fn run_concentration_soak() -> AppState {
    const OPERATIONS: u64 = 200;
    const BATCH_MILLIGRAMS: u64 = 1_000;

    let total_mass = Mass::from_milligrams(OPERATIONS * BATCH_MILLIGRAMS);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 410_000),
        CompositionComponent::new(MATERIAL_STONE, 370_000),
        CompositionComponent::new(MATERIAL_SLAG, 220_000),
    ])
    .unwrap_or_else(|error| panic!("concentration soak composition failed: {error}"));
    let mut fixture = concentration_fixture(total_mass, composition);
    let initial_matter = calculate_matter_accounting(&fixture.state)
        .unwrap_or_else(|error| panic!("concentration soak matter accounting failed: {error}"))
        .total();
    let initial_energy = fixture
        .state
        .energy()
        .get_store(fixture.energy)
        .unwrap_or_else(|| panic!("concentration soak energy store disappeared"))
        .stored();
    let mut expected_target = Mass::ZERO;
    let mut expected_residue = Mass::ZERO;
    let mut expected_energy = Energy::ZERO;

    for operation in 0..OPERATIONS {
        let batch = Mass::from_milligrams(BATCH_MILLIGRAMS);
        let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_COPPER, batch);
        expected_target = expected_target
            .checked_add(resolved.target_mass())
            .unwrap_or_else(|| panic!("concentration soak target mass overflowed"));
        expected_residue = expected_residue
            .checked_add(resolved.residue_mass())
            .unwrap_or_else(|| panic!("concentration soak residue mass overflowed"));
        expected_energy = expected_energy
            .checked_add(resolved.required_energy())
            .unwrap_or_else(|| panic!("concentration soak energy accounting overflowed"));
        let duration = resolved.process_resolution().duration();
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
        .unwrap_or_else(|error| panic!("concentration soak start failed: {error}"))
        .commit(&mut fixture.state)
        .unwrap_or_else(|error| panic!("concentration soak commit failed: {error}"));

        if operation == OPERATIONS / 2 {
            let encoded =
                serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
                    .unwrap_or_else(|error| {
                        panic!("concentration soak serialization failed: {error}")
                    });
            let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                .unwrap_or_else(|error| panic!("concentration soak decode failed: {error}"));
            fixture.state = decoded
                .into_state(&fixture.registries)
                .unwrap_or_else(|error| panic!("concentration soak resume failed: {error}"));
        }

        for _ in 0..duration.value() {
            advance_tick(&fixture.registries, &mut fixture.state)
                .unwrap_or_else(|error| panic!("concentration soak completion failed: {error}"));
        }
        if operation.is_multiple_of(25) {
            validate_loaded_state(&fixture.registries, &fixture.state)
                .unwrap_or_else(|error| panic!("concentration soak audit failed: {error}"));
        }
    }

    validate_loaded_state(&fixture.registries, &fixture.state)
        .unwrap_or_else(|error| panic!("concentration soak final audit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&fixture.state)
            .unwrap_or_else(|error| panic!("concentration soak final matter failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.source)
            .unwrap_or_else(|| panic!("concentration soak source disappeared"))
            .stored_mass(),
        Mass::ZERO
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.target)
            .unwrap_or_else(|| panic!("concentration soak target disappeared"))
            .stored_mass(),
        expected_target
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .unwrap_or_else(|| panic!("concentration soak residue disappeared"))
            .stored_mass(),
        expected_residue
    );
    assert_eq!(
        expected_target.checked_add(expected_residue),
        Some(total_mass)
    );
    assert_eq!(
        fixture
            .state
            .energy()
            .get_store(fixture.energy)
            .unwrap_or_else(|| panic!("concentration soak energy store disappeared"))
            .stored(),
        initial_energy
            .checked_sub(expected_energy)
            .unwrap_or_else(|| panic!("concentration soak consumed more energy than available"))
    );
    fixture.state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn concentration_soak_preserves_selectivity_conservation_persistence_and_replay() {
    let first = run_concentration_soak();
    let second = run_concentration_soak();
    assert_eq!(first, second);
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
        [0]["outputs"][0]["temperature"] = serde_json::json!(300_001_u64);
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
