//! Shared operating envelopes for powered and direct-labor ore preparation.

use crate::capability::CapabilityId;
use crate::core::quantity::{Mass, MassFlow, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::survival::SurvivalExertion;

/// Shared powered-throughput envelope for ore-preparation operations.
///
/// Comminution, screening, and constituent separation all consume finite work energy while an
/// equipment provider moves a bounded mass through the operation. Keeping that physical envelope in
/// one type prevents the three resolvers from drifting into subtly different rate, energy, or wear
/// contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredOreProcessProfile {
    mass_flow_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    specific_energy: MassSpecificEnergy,
    condition_wear_ppm_per_active_tick: u32,
}

/// Direct player-labor throughput envelope for low-tech ore preparation.
///
/// Manual processing deliberately carries no equipment or abstract energy input. Its cost is
/// exclusive player attention plus exact survival expenditure, while the bounded rate and batch
/// size keep machinery materially superior once infrastructure exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualOreProcessProfile {
    processing_rate: MassFlow,
    max_batch_mass: Mass,
    exertion: SurvivalExertion,
}

impl ManualOreProcessProfile {
    #[must_use]
    pub const fn new(
        processing_rate: MassFlow,
        max_batch_mass: Mass,
        exertion: SurvivalExertion,
    ) -> Self {
        assert!(
            !processing_rate.is_zero(),
            "manual ore-processing rate must be nonzero"
        );
        assert!(
            !max_batch_mass.is_zero(),
            "manual ore-processing maximum batch mass must be nonzero"
        );
        assert!(
            !exertion.energy_cost_per_tick().is_zero(),
            "manual ore-processing exertion must consume metabolic energy"
        );
        Self {
            processing_rate,
            max_batch_mass,
            exertion,
        }
    }

    #[must_use]
    pub const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }

    #[must_use]
    pub const fn max_batch_mass(self) -> Mass {
        self.max_batch_mass
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }
}

impl PoweredOreProcessProfile {
    #[must_use]
    pub const fn new(
        mass_flow_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        specific_energy: MassSpecificEnergy,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert!(
            !specific_energy.is_zero(),
            "powered ore-processing mass-specific energy must be nonzero"
        );
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            mass_flow_capability,
            max_batch_mass_capability,
            energy_carrier,
            specific_energy,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub(in crate::ore_processing) const fn mass_flow_capability(self) -> CapabilityId {
        self.mass_flow_capability
    }

    #[must_use]
    pub(in crate::ore_processing) const fn max_batch_mass_capability(self) -> CapabilityId {
        self.max_batch_mass_capability
    }

    #[must_use]
    pub(in crate::ore_processing) const fn energy_carrier(self) -> EnergyCarrier {
        self.energy_carrier
    }

    #[must_use]
    pub(in crate::ore_processing) const fn specific_energy(self) -> MassSpecificEnergy {
        self.specific_energy
    }

    #[must_use]
    pub(in crate::ore_processing) const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}
