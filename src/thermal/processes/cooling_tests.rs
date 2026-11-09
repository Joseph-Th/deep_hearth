//! Cooling changes sensible heat, not the physical milling grade.

use super::*;
use crate::capability::CapabilityId;
use crate::content::{FORM_CRUSHED, MATERIAL_COPPER, build_registries};
use crate::core::quantity::{Length, Mass};
use crate::energy::EnergyCarrier;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_spec_for_test};
use crate::material::{
    MaterialComposition, ParticleSizeClass, ParticleSizeDistribution, ParticleSizeRange,
};
use crate::production::validate_process_inputs;
use crate::thermal::PhaseChangeProcessProfile;

#[test]
fn cooling_retains_distinct_particulate_cohorts_and_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000)).unwrap();
    let commodity = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);
    let hot = Temperature::from_millikelvin(350_000);
    let target = Temperature::from_millikelvin(300_000);
    let fine = ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(2_000),
    )
    .unwrap();
    let coarse = ParticleSizeRange::new(
        Length::from_micrometers(2_001),
        Length::from_micrometers(10_000),
    )
    .unwrap();
    let grade = |fine_weight, coarse_weight| {
        ParticleSizeDistribution::new(vec![
            ParticleSizeClass::new(fine, fine_weight).unwrap(),
            ParticleSizeClass::new(coarse, coarse_weight).unwrap(),
        ])
        .unwrap()
    };
    let grades = [grade(3, 1), grade(1, 3)];
    assert_eq!(grades[0].envelope(), grades[1].envelope());
    let selections = grades
        .iter()
        .cloned()
        .map(|size| {
            let mass = Mass::from_milligrams(1_000_000);
            let spec = MaterialLotSpec::with_composition_and_particle_size(
                commodity,
                mass,
                hot,
                MaterialComposition::pure(MATERIAL_COPPER),
                size,
            )
            .unwrap();
            let lot = deposit_lot_spec_for_test(&registries, &mut state, source, spec).unwrap();
            MaterialLotSelection::new(lot, mass)
        })
        .collect::<Vec<_>>();
    let process = ProcessId::new(980_001);
    let input = validate_process_inputs(&state, process, source, &selections).unwrap();
    let definition = SensibleCoolingProcessDefinition::new(
        process,
        commodity,
        target,
        PhaseChangeProcessProfile::new(
            CapabilityId::new(980_001),
            CapabilityId::new(980_002),
            CapabilityId::new(980_003),
            EnergyCarrier::Thermal,
            1,
        ),
    );
    let batch =
        resolve_sensible_cooling_batch(registries.materials(), definition, input.consumed_inputs())
            .unwrap();
    assert!(!batch.energy.is_zero());
    assert_eq!(
        batch.outputs.len(),
        2,
        "cooling merged distinct milling grades"
    );
    assert_eq!(
        batch
            .outputs
            .iter()
            .map(MaterialLotSpec::mass)
            .fold(Mass::ZERO, |total, mass| {
                total.checked_add(mass).unwrap()
            }),
        Mass::from_milligrams(2_000_000)
    );
    for distribution in grades {
        let output = batch
            .outputs
            .iter()
            .find(|output| output.particle_size_distribution() == Some(&distribution))
            .unwrap_or_else(|| panic!("cooling erased milling grade {distribution:?}"));
        assert_eq!(output.commodity(), commodity);
        assert_eq!(output.mass(), Mass::from_milligrams(1_000_000));
        assert_eq!(output.temperature(), target);
        assert_eq!(
            output.composition(),
            &MaterialComposition::pure(MATERIAL_COPPER)
        );
    }
}
