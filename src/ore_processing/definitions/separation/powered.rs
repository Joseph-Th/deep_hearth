//! Powered constituent-separation process definitions.

use crate::capability::CapabilityId;
use crate::core::quantity::{Mass, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::material::{CommodityKey, FormId, MaterialId, ParticleSizeRange};
use crate::production::ProcessId;

use super::{
    ConstituentRecoveryProfile, ConstituentSeparationPhysics,
    minimum_homogeneous_feed_mass_for_target_recovery,
};
use crate::ore_processing::definitions::PoweredOreProcessProfile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstituentSeparationProcessDefinition {
    process: ProcessId,
    physics: ConstituentSeparationPhysics,
    operating: PoweredOreProcessProfile,
}

impl ConstituentSeparationProcessDefinition {
    pub const TARGET_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    pub const RESIDUE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    /// Authors finite-recovery sorting of an already liberated target constituent from arbitrary gangue.
    ///
    /// Every non-target constituent remains physically represented in the particulate residue
    /// stream. The residue commodity host is derived from its dominant non-target material rather
    /// than baking one gangue identity into the process definition.
    #[must_use]
    pub const fn new_sorting(
        process: ProcessId,
        input_form: FormId,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_output_form: FormId,
        target_recovery_ppm: u32,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        Self {
            process,
            physics: ConstituentSeparationPhysics::new_sorting(
                input_form,
                target_material,
                target_output_form,
                residue_output_form,
                target_recovery_ppm,
            ),
            operating,
        }
    }

    /// Authors selective finite-recovery concentration of one liberated target constituent from
    /// arbitrary non-target gangue.
    #[must_use]
    pub const fn new_concentration(
        process: ProcessId,
        input_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        target_output: CommodityKey,
        residue_output_form: FormId,
        recovery: ConstituentRecoveryProfile,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        Self {
            process,
            physics: ConstituentSeparationPhysics::new_concentration(
                input_form,
                input_particle_size_range,
                target_output,
                residue_output_form,
                recovery,
            ),
            operating,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(self) -> FormId {
        self.physics.input_form()
    }

    /// Complete particulate feed envelope that is physically liberated enough for this separation.
    /// Sorting may omit this when the target occurs as independently sortable coarse pieces.
    #[must_use]
    pub const fn input_particle_size_range(self) -> Option<ParticleSizeRange> {
        self.physics.input_particle_size_range()
    }

    #[must_use]
    pub const fn target_material(self) -> MaterialId {
        self.physics.target_material()
    }

    #[must_use]
    pub const fn target_output_form(self) -> FormId {
        self.physics.target_output_form()
    }

    #[must_use]
    pub const fn residue_output_form(self) -> FormId {
        self.physics.residue_output_form()
    }

    /// Whether feed must use the target material as its commodity host.
    ///
    /// Sorting represents recognizable target pieces and therefore requires that host identity.
    /// Concentration can accept another host when exact composition contains the target constituent.
    #[must_use]
    pub const fn requires_target_host(self) -> bool {
        self.physics.is_sorting()
    }

    /// Returns the authored fraction of exact target content recovered to the target stream.
    /// Recovery is conservatively floored at the whole-milligram output boundary for each
    /// temperature/particle-size recovery group. The unresolved fractional target and all
    /// intentionally unrecovered target remain explicit residue matter.
    #[must_use]
    pub const fn target_recovery_ppm(self) -> u32 {
        self.physics.target_recovery_ppm()
    }

    /// Minimum one-profile feed mass whose exact target-constituent share can recover `target`
    /// whole milligrams under this process's authored target recovery.
    ///
    /// `constituent_ppm` is the target-material share of that homogeneous candidate feed. Runtime
    /// recovery preserves temperature and particle-size identity and therefore rounds each distinct
    /// recovery group independently. Callers planning a heterogeneous selection must resolve the
    /// exact selection rather than treating its aggregate assay as one recovery group.
    #[must_use]
    pub fn minimum_homogeneous_feed_mass_for_target_recovery(
        self,
        target: Mass,
        constituent_ppm: u32,
    ) -> Option<Mass> {
        minimum_homogeneous_feed_mass_for_target_recovery(
            target,
            constituent_ppm,
            self.target_recovery_ppm(),
        )
    }

    /// Returns the fraction of each non-target constituent carried into a concentration target
    /// stream. Sorting always returns zero here.
    #[must_use]
    pub const fn non_target_recovery_ppm(self) -> u32 {
        self.physics.non_target_recovery_ppm()
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

    pub(in crate::ore_processing) const fn physics(self) -> ConstituentSeparationPhysics {
        self.physics
    }
}
