//! Per-lot material identity must survive exact ppm remainder reconstruction.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::content::{
    FORM_CRUSHED, FORM_TAILINGS, MATERIAL_COPPER, MATERIAL_HEMATITE, MATERIAL_SLAG, MATERIAL_STONE,
    PROCESS_CONCENTRATE_COPPER, PROCESS_HAND_SORT_NATIVE_COPPER, build_registries,
};
use crate::core::quantity::Length;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_spec_for_test,
};
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialLotSpec, ParticleSizeRange,
};
use crate::production::validate_process_inputs;

#[test]
fn residue_host_is_present_in_every_rounded_lot_without_losing_constituents() {
    let registries = build_registries();
    let definition = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .expect("canonical powered copper concentration definition")
        .physics();
    assert_eq!(definition.target_material(), MATERIAL_COPPER);
    assert_eq!(definition.residue_output_form(), FORM_TAILINGS);

    // Distinct one-ppm impurities each average below one ppm after blending.
    // A single aggregate host can disappear from some rounded output lots.
    let numerators = BTreeMap::from([
        (MATERIAL_COPPER, 1_999_998_u128),
        (MATERIAL_SLAG, 1_u128),
        (MATERIAL_STONE, 1_u128),
    ]);
    let particle_size = ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(2_000),
    )
    .expect("valid particulate size")
    .into();
    let mut grouped = BTreeMap::new();
    add_blended_residue(
        &mut grouped,
        definition,
        Temperature::from_millikelvin(300_000),
        particle_size,
        numerators.clone(),
        Mass::from_milligrams(2),
    )
    .expect("the exact constituent stream must be reconstructable");
    let lots = build_particulate_outputs(grouped)
        .expect("all residue output hosts must be present after rounding");
    assert_eq!(lots.len(), 2);
    assert_eq!(
        lots.iter().map(|lot| lot.mass().milligrams()).sum::<u64>(),
        2
    );
    for lot in &lots {
        assert_eq!(lot.commodity().form(), FORM_TAILINGS);
        assert!(registries.materials().has_commodity(lot.commodity()));
        assert!(
            lot.composition()
                .parts_per_million(lot.commodity().material())
                > 0
        );
    }
    for (material, exact_numerator) in numerators {
        let reconstructed = lots
            .iter()
            .map(|lot| {
                u128::from(lot.mass().milligrams())
                    * u128::from(lot.composition().parts_per_million(material))
            })
            .sum::<u128>();
        assert_eq!(reconstructed, exact_numerator, "constituent {material:?}");
    }
}

#[test]
fn mixed_gangue_feed_resolves_to_valid_residue_lots_with_exact_constituents() {
    let registries = build_registries();
    let manual_definition = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .expect("registered manual copper sorting");
    let definition = manual_definition.physics();
    let particle_size = definition
        .input_particle_size_range()
        .expect("manual sorting authors a supported particle size");
    let mass_each = Mass::from_milligrams(manual_definition.max_batch_mass().milligrams() / 3);
    let feed_mass = mass_each
        .checked_add(mass_each)
        .and_then(|total| total.checked_add(mass_each))
        .expect("three bounded cohorts fit the authored manual batch");
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, feed_mass)
        .expect("source has room for all three trace-gangue cohorts");
    let mut selections = Vec::new();
    let mut expected_constituents = BTreeMap::<MaterialId, u128>::new();
    for gangue in [MATERIAL_STONE, MATERIAL_SLAG, MATERIAL_HEMATITE] {
        let composition = MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 999_999),
            CompositionComponent::new(gangue, 1),
        ])
        .expect("legitimate single-ppm gangue in copper feed");
        for component in composition.components() {
            let numerator =
                u128::from(mass_each.milligrams()) * u128::from(component.parts_per_million());
            let total = expected_constituents
                .entry(component.material())
                .or_default();
            *total = total
                .checked_add(numerator)
                .expect("bounded input constituent sum");
        }
        let input = MaterialLotSpec::with_composition_and_particle_size(
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            mass_each,
            Temperature::from_millikelvin(300_000),
            composition,
            particle_size,
        )
        .expect("valid prepared ore lot");
        let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
            .expect("each distinct gangue cohort has valid inventory custody");
        selections.push(MaterialLotSelection::new(lot, mass_each));
    }

    // Canonical validated input selection groups equal temperature and size,
    // then exact recovery and remainder reconstruction produce distinct lots.
    let inputs =
        validate_process_inputs(&state, PROCESS_HAND_SORT_NATIVE_COPPER, source, &selections)
            .expect("each selected crushed copper lot is available");
    let output_form = registries
        .materials()
        .get_form(definition.target_output_form())
        .expect("registered sorting output form");
    let outputs = super::super::resolve_separation_outputs(
        registries.materials(),
        definition,
        output_form.particle_size_policy(),
        inputs.consumed_inputs(),
    )
    .expect("individually valid mixed-gangue feed must resolve");
    assert_eq!(
        outputs.target_mass.checked_add(outputs.residue_mass),
        Some(feed_mass)
    );
    let residue_hosts = outputs
        .residue
        .iter()
        .map(|lot| lot.commodity().material())
        .collect::<BTreeSet<_>>();
    assert!(
        residue_hosts.len() > 1,
        "quantized residues require different valid hosts"
    );
    for lot in outputs.target.iter().chain(&outputs.residue) {
        assert!(registries.materials().has_commodity(lot.commodity()));
        assert!(
            lot.composition()
                .parts_per_million(lot.commodity().material())
                > 0
        );
    }
    let all_outputs = outputs.target.iter().chain(&outputs.residue);
    for (material, numerator) in expected_constituents {
        let actual = all_outputs
            .clone()
            .map(|lot| {
                u128::from(lot.mass().milligrams())
                    * u128::from(lot.composition().parts_per_million(material))
            })
            .sum::<u128>();
        assert_eq!(
            actual, numerator,
            "selected material {material:?} must be conserved"
        );
    }
}
