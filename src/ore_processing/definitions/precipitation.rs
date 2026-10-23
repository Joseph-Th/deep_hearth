//! Authored two-feed, no-offgas lead/iron sulfide displacement.
//! PbS + Fe -> Pb + FeS, with exact atomic-equation feed masses.

use super::PoweredOreProcessProfile;
use crate::core::quantity::Temperature;
use crate::energy::EnergyCarrier;
use crate::production::ProcessId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrecipitationProcessDefinition {
    process: ProcessId,
    profile: PoweredOreProcessProfile,
    operating_temperature: Temperature,
}

impl PrecipitationProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        profile: PoweredOreProcessProfile,
        operating_temperature: Temperature,
    ) -> Self {
        Self {
            process,
            profile,
            operating_temperature,
        }
    }
    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }
    #[must_use]
    pub const fn operating_profile(self) -> PoweredOreProcessProfile {
        self.profile
    }
    #[must_use]
    pub const fn operating_temperature(self) -> Temperature {
        self.operating_temperature
    }
    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.profile.energy_carrier()
    }
}
