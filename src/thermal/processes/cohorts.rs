//! Shared matter projection for temperature-only transformations.
//
// Heating and cooling may aggregate exactly matching material cohorts, but
// neither may discard composition or grain-size information as heat moves.

use std::collections::BTreeMap;

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, MaterialLotSpecError,
    ParticleSizeDistribution,
};

#[derive(Default)]
pub(super) struct PreservedMaterialCohorts {
    masses: BTreeMap<
        (
            CommodityKey,
            MaterialComposition,
            Option<ParticleSizeDistribution>,
        ),
        Mass,
    >,
}

impl PreservedMaterialCohorts {
    /// A temperature change cannot homogenize otherwise different physical material.
    pub(super) fn add(
        &mut self,
        commodity: CommodityKey,
        trace: &ConsumedMaterialTrace,
    ) -> Option<()> {
        let profile = trace.profile();
        let key = (
            commodity,
            profile.composition().clone(),
            profile.particle_size_distribution().cloned(),
        );
        let current = self.masses.get(&key).copied().unwrap_or(Mass::ZERO);
        let next = current.checked_add(trace.mass())?;
        self.masses.insert(key, next);
        Some(())
    }

    /// Convert exact coalesced masses into output specifications at the new temperature.
    pub(super) fn into_outputs(
        self,
        target: Temperature,
    ) -> Result<Vec<MaterialLotSpec>, MaterialLotSpecError> {
        self.masses
            .into_iter()
            .map(
                |((commodity, composition, particle_size), mass)| match particle_size {
                    Some(particle_size) => MaterialLotSpec::with_composition_and_particle_size(
                        commodity,
                        mass,
                        target,
                        composition,
                        particle_size,
                    ),
                    None => MaterialLotSpec::with_composition(commodity, mass, target, composition),
                },
            )
            .collect()
    }
}
