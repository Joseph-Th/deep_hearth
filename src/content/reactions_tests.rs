//! Stoichiometric reference checks; these equations are not executable furnace recipes.

use super::*;

#[test]
fn all_ten_mineral_reaction_equations_conserve_atoms_and_exact_mass() {
    for reaction in REACTIONS {
        assert!(
            reaction.is_balanced(),
            "unbalanced reaction: {}",
            reaction.description
        );
        assert_eq!(
            reaction.mass_milligrams(true),
            reaction.mass_milligrams(false)
        );
    }
    assert_eq!(REACTIONS[0].mass_milligrams(true), 195_720);
    assert_eq!(REACTIONS[2].mass_milligrams(true), 174_730);
    assert_eq!(REACTIONS[3].mass_milligrams(true), 290_874);
    assert_eq!(REACTIONS[8].mass_milligrams(true), 84_105);
    assert_eq!(REACTIONS[9].mass_milligrams(true), 295_105);
}

#[test]
fn chemical_formulas_match_all_eight_registered_mineral_identities() {
    let registries = crate::content::build_registries();
    for (id, expected) in [
        (MATERIAL_HEMATITE, HEMATITE),
        (MATERIAL_MAGNETITE, MAGNETITE),
        (MATERIAL_CASSITERITE, CASSITERITE),
        (MATERIAL_SPHALERITE, SPHALERITE),
        (MATERIAL_GALENA, GALENA),
        (MATERIAL_PYRITE, PYRITE),
        (MATERIAL_GRAPHITE, CARBON),
        (MATERIAL_QUARTZ, QUARTZ),
    ] {
        assert!(registries.materials().get_material(id).is_some());
        assert_eq!(mineral_formula(id), Some(expected));
        assert!(expected.molar_mass_milligrams() > 0);
    }
    assert_eq!(mineral_formula(crate::content::MATERIAL_IRON), None);
}

#[test]
fn omitting_exhaust_or_reductant_is_detected_even_when_metals_match() {
    const IRON_ONLY: &[ReactionTerm] = &[ReactionTerm::new(IRON, 2)];
    const SULFIDE_ONLY: &[ReactionTerm] = &[ReactionTerm::new(SPHALERITE, 1)];
    const ZINC_ONLY: &[ReactionTerm] = &[ReactionTerm::new(ZINC, 1)];
    let impossible = ReactionEquation {
        description: "invalid iron without carbon monoxide",
        inputs: REACTIONS[0].inputs,
        outputs: IRON_ONLY,
    };
    assert!(!impossible.is_balanced());
    assert_eq!(impossible.element_count(Element::Oxygen, true), 3);
    assert_eq!(impossible.element_count(Element::Oxygen, false), 0);
    let impossible_sulfide = ReactionEquation {
        description: "invalid direct sulfide to zinc",
        inputs: SULFIDE_ONLY,
        outputs: ZINC_ONLY,
    };
    assert!(!impossible_sulfide.is_balanced());
}
