//! Direct-labor constituent-separation process definitions.

use crate::core::quantity::{Mass, MassFlow};
use crate::material::{CommodityKey, FormId, MaterialId, ParticleSizeRange};
use crate::production::ProcessId;
use crate::survival::SurvivalExertion;

use super::{ConstituentSeparationPhysics, minimum_homogeneous_feed_mass_for_target_recovery};
use crate::ore_processing::definitions::ManualOreProcessProfile;

/// Immutable selected-batch constituent separation performed directly by player labor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualConstituentSeparationProcessDefinition {
    process: ProcessId,
    physics: ConstituentSeparationPhysics,
    operating: ManualOreProcessProfile,
}

impl ManualConstituentSeparationProcessDefinition {
    pub const TARGET_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    pub const RESIDUE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    /// Authors deterministic hand sorting of liberated target pieces from gangue.
    #[must_use]
    pub const fn new_sorting(
        process: ProcessId,
        input_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        target_output: CommodityKey,
        residue_output_form: FormId,
        target_recovery_ppm: u32,
        operating: ManualOreProcessProfile,
    ) -> Self {
        Self {
            process,
            physics: ConstituentSeparationPhysics::new_sorting_with_input_particle_size_range(
                input_form,
                input_particle_size_range,
                target_output.material(),
                target_output.form(),
                residue_output_form,
                target_recovery_ppm,
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

    /// Complete particulate feed envelope that remains individually hand-sortable.
    #[must_use]
    pub fn input_particle_size_range(self) -> ParticleSizeRange {
        self.physics
            .input_particle_size_range()
            .unwrap_or_else(|| unreachable!("manual sorting always authors a visible-piece range"))
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

    #[must_use]
    pub const fn target_recovery_ppm(self) -> u32 {
        self.physics.target_recovery_ppm()
    }

    /// Minimum one-profile feed mass whose exact target-constituent share can recover `target`
    /// whole milligrams under this manual process's authored recovery.
    ///
    /// Runtime recovery preserves temperature and particle-size identity and therefore rounds each
    /// distinct recovery group independently. Callers planning a heterogeneous selection must
    /// resolve that exact selection instead of treating an aggregate assay as one homogeneous lot.
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

    #[must_use]
    pub const fn processing_rate(self) -> MassFlow {
        self.operating.processing_rate()
    }

    #[must_use]
    pub const fn max_batch_mass(self) -> Mass {
        self.operating.max_batch_mass()
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.operating.exertion()
    }

    pub(in crate::ore_processing) const fn physics(self) -> ConstituentSeparationPhysics {
        self.physics
    }
}
