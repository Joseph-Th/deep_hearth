//! Atomic mass and atom-balance constraints for future mineral-reduction chemistry.
//!
//! These are chemical *reference* definitions, not registered production processes.
//! A runtime resolver must first account for reductant custody, gas transport,
//! heat of reaction, kinetics, oxygen supply and sealed output destinations.
//! Describing a balanced formula does not create metal in the running world.

use super::materials::{
    MATERIAL_CASSITERITE, MATERIAL_GALENA, MATERIAL_GRAPHITE, MATERIAL_HEMATITE,
    MATERIAL_MAGNETITE, MATERIAL_PYRITE, MATERIAL_QUARTZ, MATERIAL_SPHALERITE,
};
use crate::material::MaterialId;

/// Element identity with an explicit standard-mass approximation in milligrams
/// per mole of atoms; all arithmetic is integer and independent of floating point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Element {
    Iron,
    Tin,
    Zinc,
    Lead,
    Oxygen,
    Carbon,
    Sulfur,
    Silicon,
}

impl Element {
    #[must_use]
    pub const fn milligrams_per_mole(self) -> u64 {
        match self {
            Self::Iron => 55_845,
            Self::Tin => 118_710,
            Self::Zinc => 65_380,
            Self::Lead => 207_200,
            Self::Oxygen => 15_999,
            Self::Carbon => 12_011,
            Self::Sulfur => 32_060,
            Self::Silicon => 28_085,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormulaAtom {
    pub element: Element,
    pub count: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChemicalSpecies {
    pub formula: &'static str,
    pub atoms: &'static [FormulaAtom],
}

impl ChemicalSpecies {
    #[must_use]
    pub fn molar_mass_milligrams(self) -> u64 {
        self.atoms
            .iter()
            .map(|atom| atom.element.milligrams_per_mole() * u64::from(atom.count))
            .sum()
    }
}

macro_rules! atom {
    ($element:ident, $count:literal) => {
        FormulaAtom {
            element: Element::$element,
            count: $count,
        }
    };
}

pub const HEMATITE: ChemicalSpecies = ChemicalSpecies {
    formula: "Fe2O3",
    atoms: &[atom!(Iron, 2), atom!(Oxygen, 3)],
};
pub const MAGNETITE: ChemicalSpecies = ChemicalSpecies {
    formula: "Fe3O4",
    atoms: &[atom!(Iron, 3), atom!(Oxygen, 4)],
};
pub const CASSITERITE: ChemicalSpecies = ChemicalSpecies {
    formula: "SnO2",
    atoms: &[atom!(Tin, 1), atom!(Oxygen, 2)],
};
pub const SPHALERITE: ChemicalSpecies = ChemicalSpecies {
    formula: "ZnS",
    atoms: &[atom!(Zinc, 1), atom!(Sulfur, 1)],
};
pub const GALENA: ChemicalSpecies = ChemicalSpecies {
    formula: "PbS",
    atoms: &[atom!(Lead, 1), atom!(Sulfur, 1)],
};
pub const PYRITE: ChemicalSpecies = ChemicalSpecies {
    formula: "FeS2",
    atoms: &[atom!(Iron, 1), atom!(Sulfur, 2)],
};
pub const QUARTZ: ChemicalSpecies = ChemicalSpecies {
    formula: "SiO2",
    atoms: &[atom!(Silicon, 1), atom!(Oxygen, 2)],
};
pub const CARBON: ChemicalSpecies = ChemicalSpecies {
    formula: "C",
    atoms: &[atom!(Carbon, 1)],
};
pub const OXYGEN: ChemicalSpecies = ChemicalSpecies {
    formula: "O2",
    atoms: &[atom!(Oxygen, 2)],
};
pub const IRON: ChemicalSpecies = ChemicalSpecies {
    formula: "Fe",
    atoms: &[atom!(Iron, 1)],
};
pub const TIN: ChemicalSpecies = ChemicalSpecies {
    formula: "Sn",
    atoms: &[atom!(Tin, 1)],
};
pub const ZINC: ChemicalSpecies = ChemicalSpecies {
    formula: "Zn",
    atoms: &[atom!(Zinc, 1)],
};
pub const LEAD: ChemicalSpecies = ChemicalSpecies {
    formula: "Pb",
    atoms: &[atom!(Lead, 1)],
};
pub const IRON_SULFIDE: ChemicalSpecies = ChemicalSpecies {
    formula: "FeS",
    atoms: &[atom!(Iron, 1), atom!(Sulfur, 1)],
};
pub const SILICON: ChemicalSpecies = ChemicalSpecies {
    formula: "Si",
    atoms: &[atom!(Silicon, 1)],
};

/// Links the registered mined mineral to its atomic formula, without implying
/// that ore can be reduced by melting it.
#[must_use]
pub fn mineral_formula(material: MaterialId) -> Option<ChemicalSpecies> {
    match material {
        MATERIAL_HEMATITE => Some(HEMATITE),
        MATERIAL_MAGNETITE => Some(MAGNETITE),
        MATERIAL_CASSITERITE => Some(CASSITERITE),
        MATERIAL_SPHALERITE => Some(SPHALERITE),
        MATERIAL_GALENA => Some(GALENA),
        MATERIAL_PYRITE => Some(PYRITE),
        MATERIAL_GRAPHITE => Some(CARBON),
        MATERIAL_QUARTZ => Some(QUARTZ),
        _ => None,
    }
}
pub const ZINC_OXIDE: ChemicalSpecies = ChemicalSpecies {
    formula: "ZnO",
    atoms: &[atom!(Zinc, 1), atom!(Oxygen, 1)],
};
pub const LEAD_OXIDE: ChemicalSpecies = ChemicalSpecies {
    formula: "PbO",
    atoms: &[atom!(Lead, 1), atom!(Oxygen, 1)],
};
pub const CARBON_MONOXIDE: ChemicalSpecies = ChemicalSpecies {
    formula: "CO",
    atoms: &[atom!(Carbon, 1), atom!(Oxygen, 1)],
};
pub const SULFUR_DIOXIDE: ChemicalSpecies = ChemicalSpecies {
    formula: "SO2",
    atoms: &[atom!(Sulfur, 1), atom!(Oxygen, 2)],
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionTerm {
    pub species: ChemicalSpecies,
    pub moles: u8,
}

impl ReactionTerm {
    pub const fn new(species: ChemicalSpecies, moles: u8) -> Self {
        assert!(moles > 0, "reaction coefficients must be positive");
        Self { species, moles }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionEquation {
    pub description: &'static str,
    pub inputs: &'static [ReactionTerm],
    pub outputs: &'static [ReactionTerm],
}

impl ReactionEquation {
    /// Stoichiometric batch mass, in milligrams for the displayed molar coefficients.
    #[must_use]
    pub fn mass_milligrams(self, input: bool) -> u64 {
        let terms = if input { self.inputs } else { self.outputs };
        terms
            .iter()
            .map(|term| term.species.molar_mass_milligrams() * u64::from(term.moles))
            .sum()
    }

    /// Atom conservation is stricter than total mass conservation. A recipe
    /// cannot turn an oxide into metal without explicitly putting oxygen
    /// somewhere, even when its output total mass would appear plausible.
    #[must_use]
    pub fn element_count(self, element: Element, input: bool) -> u64 {
        let terms = if input { self.inputs } else { self.outputs };
        terms
            .iter()
            .flat_map(|term| {
                term.species.atoms.iter().filter_map(move |atom| {
                    (atom.element == element)
                        .then_some(u64::from(atom.count) * u64::from(term.moles))
                })
            })
            .sum()
    }

    #[must_use]
    pub fn is_balanced(self) -> bool {
        const ELEMENTS: [Element; 8] = [
            Element::Iron,
            Element::Tin,
            Element::Zinc,
            Element::Lead,
            Element::Oxygen,
            Element::Carbon,
            Element::Sulfur,
            Element::Silicon,
        ];
        !self.inputs.is_empty()
            && !self.outputs.is_empty()
            && self.inputs.iter().chain(self.outputs).all(|term| {
                term.moles != 0
                    && !term.species.atoms.is_empty()
                    && term.species.atoms.iter().all(|atom| atom.count != 0)
            })
            && ELEMENTS.iter().all(|&element| {
                self.element_count(element, true) == self.element_count(element, false)
            })
            && self.mass_milligrams(true) == self.mass_milligrams(false)
    }
}

/// Ten atom-balanced reactions. The galena/iron precipitation is the first
/// executable no-offgas chemistry; the nine gas-forming equations remain
/// reference data until finite atmosphere and exhaust handling exist.
pub const REACTIONS: [ReactionEquation; 10] = [
    ReactionEquation {
        description: "hematite carbon reduction",
        inputs: &[ReactionTerm::new(HEMATITE, 1), ReactionTerm::new(CARBON, 3)],
        outputs: &[
            ReactionTerm::new(IRON, 2),
            ReactionTerm::new(CARBON_MONOXIDE, 3),
        ],
    },
    ReactionEquation {
        description: "magnetite carbon reduction",
        inputs: &[
            ReactionTerm::new(MAGNETITE, 1),
            ReactionTerm::new(CARBON, 4),
        ],
        outputs: &[
            ReactionTerm::new(IRON, 3),
            ReactionTerm::new(CARBON_MONOXIDE, 4),
        ],
    },
    ReactionEquation {
        description: "cassiterite carbon reduction",
        inputs: &[
            ReactionTerm::new(CASSITERITE, 1),
            ReactionTerm::new(CARBON, 2),
        ],
        outputs: &[
            ReactionTerm::new(TIN, 1),
            ReactionTerm::new(CARBON_MONOXIDE, 2),
        ],
    },
    ReactionEquation {
        description: "sphalerite oxidative roasting",
        inputs: &[
            ReactionTerm::new(SPHALERITE, 2),
            ReactionTerm::new(OXYGEN, 3),
        ],
        outputs: &[
            ReactionTerm::new(ZINC_OXIDE, 2),
            ReactionTerm::new(SULFUR_DIOXIDE, 2),
        ],
    },
    ReactionEquation {
        description: "zinc oxide carbon reduction",
        inputs: &[
            ReactionTerm::new(ZINC_OXIDE, 1),
            ReactionTerm::new(CARBON, 1),
        ],
        outputs: &[
            ReactionTerm::new(ZINC, 1),
            ReactionTerm::new(CARBON_MONOXIDE, 1),
        ],
    },
    ReactionEquation {
        description: "pyrite oxidative roasting",
        inputs: &[ReactionTerm::new(PYRITE, 4), ReactionTerm::new(OXYGEN, 11)],
        outputs: &[
            ReactionTerm::new(HEMATITE, 2),
            ReactionTerm::new(SULFUR_DIOXIDE, 8),
        ],
    },
    ReactionEquation {
        description: "galena oxidative roasting",
        inputs: &[ReactionTerm::new(GALENA, 2), ReactionTerm::new(OXYGEN, 3)],
        outputs: &[
            ReactionTerm::new(LEAD_OXIDE, 2),
            ReactionTerm::new(SULFUR_DIOXIDE, 2),
        ],
    },
    ReactionEquation {
        description: "lead oxide carbon reduction",
        inputs: &[
            ReactionTerm::new(LEAD_OXIDE, 1),
            ReactionTerm::new(CARBON, 1),
        ],
        outputs: &[
            ReactionTerm::new(LEAD, 1),
            ReactionTerm::new(CARBON_MONOXIDE, 1),
        ],
    },
    ReactionEquation {
        description: "silica carbon reduction",
        inputs: &[ReactionTerm::new(QUARTZ, 1), ReactionTerm::new(CARBON, 2)],
        outputs: &[
            ReactionTerm::new(SILICON, 1),
            ReactionTerm::new(CARBON_MONOXIDE, 2),
        ],
    },
    ReactionEquation {
        description: "galena iron precipitation",
        inputs: &[ReactionTerm::new(GALENA, 1), ReactionTerm::new(IRON, 1)],
        outputs: &[
            ReactionTerm::new(LEAD, 1),
            ReactionTerm::new(IRON_SULFIDE, 1),
        ],
    },
];

#[cfg(test)]
#[path = "reactions_tests.rs"]
mod tests;
