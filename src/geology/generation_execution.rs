//! Admission of finite world-generated geological matter into authoritative state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::material::{
    FormId, MaterialId, MaterialPhase, MaterialPhaseStateError, ParticleSizeStatePolicy,
    validate_material_phase_state,
};
use crate::registry::Registries;

use super::state::{
    GeneratedDepositSpec, GeologicalDepositId, GeologicalDepositLifecycle, GeologicalDepositRecord,
};

/// Failure while admitting a finite world-generated geological deposit into authoritative state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertGeneratedDepositError {
    UnknownMaterial { material: MaterialId },
    UnknownForm { form: FormId },
    UnsupportedPhase { form: FormId, phase: MaterialPhase },
    UnsupportedParticulateForm { form: FormId },
    InvalidPhaseState(MaterialPhaseStateError),
    UnknownCompositionMaterial { material: MaterialId },
    IdExhausted,
    RevisionExhausted,
}

impl Display for InsertGeneratedDepositError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMaterial { material } => write!(
                formatter,
                "generated geological deposit references unknown material {}",
                material.value()
            ),
            Self::UnknownForm { form } => write!(
                formatter,
                "generated geological deposit references unknown form {}",
                form.value()
            ),
            Self::UnsupportedPhase { form, phase } => write!(
                formatter,
                "generated geological deposit form {} is {phase:?}; finite geological deposits must be solid",
                form.value()
            ),
            Self::UnsupportedParticulateForm { form } => write!(
                formatter,
                "generated geological deposit form {} requires processed particle-size state; natural geological deposits cannot own it",
                form.value()
            ),
            Self::InvalidPhaseState(error) => write!(
                formatter,
                "generated geological deposit has invalid material phase state: {error}"
            ),
            Self::UnknownCompositionMaterial { material } => write!(
                formatter,
                "generated geological deposit composition references unknown material {}",
                material.value()
            ),
            Self::IdExhausted => {
                formatter.write_str("geological deposit identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("geology revision space is exhausted"),
        }
    }
}

impl Error for InsertGeneratedDepositError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPhaseState(error) => Some(error),
            Self::UnknownMaterial { material: _ }
            | Self::UnknownForm { form: _ }
            | Self::UnsupportedPhase { form: _, phase: _ }
            | Self::UnsupportedParticulateForm { form: _ }
            | Self::UnknownCompositionMaterial { material: _ }
            | Self::IdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

/// Inserts matter supplied by a world-generation owner, preserving its physical profile exactly.
///
/// This is not a player mining operation. It establishes finite geological matter that the mining
/// subsystem may later reserve and excavate through its tool/labor-gated transaction.
pub fn insert_generated_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> Result<GeologicalDepositId, InsertGeneratedDepositError> {
    if registries
        .materials()
        .get_material(spec.commodity().material())
        .is_none()
    {
        return Err(InsertGeneratedDepositError::UnknownMaterial {
            material: spec.commodity().material(),
        });
    }
    let Some(form) = registries.materials().get_form(spec.commodity().form()) else {
        return Err(InsertGeneratedDepositError::UnknownForm {
            form: spec.commodity().form(),
        });
    };
    if form.phase() != MaterialPhase::Solid {
        return Err(InsertGeneratedDepositError::UnsupportedPhase {
            form: spec.commodity().form(),
            phase: form.phase(),
        });
    }
    if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
        return Err(InsertGeneratedDepositError::UnsupportedParticulateForm {
            form: spec.commodity().form(),
        });
    }
    for component in spec.composition().components() {
        if registries
            .materials()
            .get_material(component.material())
            .is_none()
        {
            return Err(InsertGeneratedDepositError::UnknownCompositionMaterial {
                material: component.material(),
            });
        }
    }
    validate_material_phase_state(
        registries.materials(),
        spec.commodity(),
        spec.composition(),
        spec.temperature(),
    )
    .map_err(InsertGeneratedDepositError::InvalidPhaseState)?;

    let geology = state.geology();
    let id = GeologicalDepositId::new(geology.next_deposit_id());
    let Some(next_id) = geology.next_deposit_id().checked_add(1) else {
        return Err(InsertGeneratedDepositError::IdExhausted);
    };
    let Some(next_revision) = geology.revision().checked_add(1) else {
        return Err(InsertGeneratedDepositError::RevisionExhausted);
    };
    let generated_at = state.tick();
    let record = GeologicalDepositRecord {
        id,
        bounds: spec.bounds(),
        commodity: spec.commodity(),
        initial_mass: spec.mass(),
        remaining_mass: spec.mass(),
        temperature: spec.temperature(),
        excavation_hardness: spec.excavation_hardness(),
        composition: spec.composition().clone(),
        lifecycle: GeologicalDepositLifecycle::Available,
        generated_at,
    };

    state
        .geology_state_mut()
        .insert_deposit(record, next_id, next_revision);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{FORM_CRUSHED, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER, build_registries};
    use crate::core::quantity::{Mass, Pressure, Temperature};
    use crate::core::time::WorldSeed;
    use crate::material::{
        CommodityKey, CompositionComponent, MaterialComposition, MaterialId, MaterialPhase,
    };
    use crate::spatial::{VoxelBounds, VoxelCoord};

    fn bounds(x: i64) -> VoxelBounds {
        VoxelBounds::new(VoxelCoord::new(x, -12, 0), VoxelCoord::new(x + 4, -8, 4))
            .unwrap_or_else(|error| panic!("geological generation bounds failed: {error}"))
    }

    #[test]
    fn generated_geological_owner_rejects_liquid_material_form_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0011));
        let spec = GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(1_357_770),
            Pressure::from_pascals(350_000_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("liquid geology specification fixture failed: {error}"));
        let before = state.clone();

        assert_eq!(
            insert_generated_deposit(&registries, &mut state, spec),
            Err(InsertGeneratedDepositError::UnsupportedPhase {
                form: FORM_MOLTEN,
                phase: MaterialPhase::Liquid,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn generated_geological_owner_rejects_processed_particulate_form_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0012));
        let spec = GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(350_000_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| {
            panic!("particulate geology specification fixture failed: {error}")
        });
        let before = state.clone();

        assert_eq!(
            insert_generated_deposit(&registries, &mut state, spec),
            Err(InsertGeneratedDepositError::UnsupportedParticulateForm { form: FORM_CRUSHED })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn generated_deposit_insertion_resolves_all_material_references_before_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0000));
        let unknown = MaterialId::new(999_999);
        let unknown_host = GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(unknown, FORM_ORE),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(350_000_000),
            MaterialComposition::pure(unknown),
        )
        .unwrap_or_else(|error| {
            panic!("unknown-host deposit specification failed locally: {error}")
        });
        let before = state.clone();
        assert_eq!(
            insert_generated_deposit(&registries, &mut state, unknown_host),
            Err(InsertGeneratedDepositError::UnknownMaterial { material: unknown })
        );
        assert_eq!(state, before);

        let mixed = MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 500_000),
            CompositionComponent::new(unknown, 500_000),
        ])
        .unwrap_or_else(|error| panic!("unknown-constituent composition fixture failed: {error}"));
        let unknown_constituent = GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(350_000_000),
            mixed,
        )
        .unwrap_or_else(|error| {
            panic!("unknown-constituent deposit specification failed locally: {error}")
        });
        assert_eq!(
            insert_generated_deposit(&registries, &mut state, unknown_constituent),
            Err(InsertGeneratedDepositError::UnknownCompositionMaterial { material: unknown })
        );
        assert_eq!(state, before);
    }
}
