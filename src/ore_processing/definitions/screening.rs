//! Screening definitions for deterministic particulate size classification.

use crate::capability::CapabilityId;
use crate::core::quantity::{Length, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::material::FormId;
use crate::production::ProcessId;

use super::PoweredOreProcessProfile;

/// Immutable declaration that one selected-batch process classifies particulate material by size.
///
/// The aperture is an exact classification boundary. Runtime resolution succeeds only when every
/// selected particle-size class lies wholly on one side of that boundary, so screening never
/// invents a mass fraction for an unresolved class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreeningProcessDefinition {
    process: ProcessId,
    input_form: FormId,
    output_form: FormId,
    aperture: Length,
    operating: PoweredOreProcessProfile,
}

impl ScreeningProcessDefinition {
    /// Stable output stream identity for material at or below the authored aperture.
    pub const UNDERSIZE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    /// Stable output stream identity for material strictly above the authored aperture.
    pub const OVERSIZE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    #[must_use]
    pub const fn new(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        aperture: Length,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        assert!(!aperture.is_zero(), "screening aperture must be nonzero");
        Self {
            process,
            input_form,
            output_form,
            aperture,
            operating,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(self) -> FormId {
        self.input_form
    }

    #[must_use]
    pub const fn output_form(self) -> FormId {
        self.output_form
    }

    #[must_use]
    pub const fn aperture(self) -> Length {
        self.aperture
    }

    #[must_use]
    pub const fn mass_flow_capability(self) -> CapabilityId {
        self.operating.mass_flow_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.operating.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.operating.energy_carrier()
    }

    #[must_use]
    pub const fn specific_energy(self) -> MassSpecificEnergy {
        self.operating.specific_energy()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.operating.condition_wear_ppm_per_active_tick()
    }

    pub(in crate::ore_processing) const fn operating_profile(self) -> PoweredOreProcessProfile {
        self.operating
    }
}
