//! Ore/material preparation definitions and scalar throughput physics; sibling execution code resolves exact selected batches.

mod comminution_execution;
mod screening_execution;
mod separation_execution;
mod timing;

use crate::capability::{CapabilityId, CapabilityRegistry, CapabilityValueKind};
use crate::core::quantity::{Length, Mass, MassFlow, MassSpecificEnergy};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::{
    CommodityKey, FormId, MaterialId, MaterialPhase, MaterialRegistry, ParticleSizeDistribution,
    ParticleSizeRange, ParticleSizeStatePolicy,
};
use crate::production::{ProcessId, ProcessInputPolicy, ProductionRegistry};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub use comminution_execution::{
    ComminutionBatchError, ComminutionBottleneck, ComminutionJobValidationError,
    ComminutionRequest, ComminutionResolutionError, ResolvedComminution,
    resolve_comminution_process,
};

pub(crate) use comminution_execution::validate_loaded_comminution_job;

pub use separation_execution::{
    ConstituentSeparationBatchError, ConstituentSeparationBottleneck,
    ConstituentSeparationJobValidationError, ConstituentSeparationRequest,
    ConstituentSeparationResolutionError, ResolvedConstituentSeparation,
    resolve_constituent_separation_process,
};

pub(crate) use separation_execution::validate_loaded_constituent_separation_job;

pub use screening_execution::{
    ResolvedScreening, ScreeningBatchError, ScreeningBottleneck, ScreeningJobValidationError,
    ScreeningRequest, ScreeningResolutionError, resolve_screening_process,
};

pub(crate) use screening_execution::validate_loaded_screening_job;

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
}

/// Immutable declaration that one selected-batch process reduces solid material to a finer form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComminutionProcessDefinition {
    process: ProcessId,
    input_form: FormId,
    output_form: FormId,
    input_particle_size_range: Option<ParticleSizeRange>,
    output_particle_size: ParticleSizeDistribution,
    operating: PoweredOreProcessProfile,
}

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

/// Immutable declaration that one selected-batch process separates an authored target constituent
/// from physically liberated particulate feed.
///
/// A binary definition names the only admissible residue material. A concentration definition
/// accepts any non-target constituents, allowing one authored physical separation method to handle
/// variable gangue without proliferating composition-specific recipes. The resolver derives output
/// masses from exact selected composition. Whole milligrams of recovered target become the target
/// stream; all unrecovered fractional target and every non-target constituent remain represented in
/// particulate residue. Residue retains the selected feed's particle-size state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstituentSeparationProcessDefinition {
    process: ProcessId,
    input_form: FormId,
    target_material: MaterialId,
    target_output_form: FormId,
    residue_material: Option<MaterialId>,
    residue_output_form: FormId,
    operating: PoweredOreProcessProfile,
}

impl ConstituentSeparationProcessDefinition {
    pub const TARGET_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    pub const RESIDUE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    #[must_use]
    pub const fn new_binary(
        process: ProcessId,
        input_form: FormId,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_material: MaterialId,
        residue_output_form: FormId,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        assert!(
            target_material.value() != residue_material.value(),
            "constituent separation target and residue materials must differ"
        );
        Self {
            process,
            input_form,
            target_material,
            target_output_form,
            residue_material: Some(residue_material),
            residue_output_form,
            operating,
        }
    }

    /// Authors concentration of one liberated target constituent from arbitrary non-target gangue.
    #[must_use]
    pub const fn new_concentration(
        process: ProcessId,
        input_form: FormId,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_output_form: FormId,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        Self {
            process,
            input_form,
            target_material,
            target_output_form,
            residue_material: None,
            residue_output_form,
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
    pub const fn target_material(self) -> MaterialId {
        self.target_material
    }

    #[must_use]
    pub const fn target_output_form(self) -> FormId {
        self.target_output_form
    }

    #[must_use]
    pub const fn residue_material(self) -> Option<MaterialId> {
        self.residue_material
    }

    #[must_use]
    pub const fn residue_output_form(self) -> FormId {
        self.residue_output_form
    }

    #[must_use]
    pub const fn mass_flow_capability(self) -> CapabilityId {
        self.operating.mass_flow_capability
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.operating.max_batch_mass_capability
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.operating.energy_carrier
    }

    #[must_use]
    pub const fn specific_energy(self) -> MassSpecificEnergy {
        self.operating.specific_energy
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.operating.condition_wear_ppm_per_active_tick
    }
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
        self.operating.mass_flow_capability
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.operating.max_batch_mass_capability
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.operating.energy_carrier
    }

    #[must_use]
    pub const fn specific_energy(self) -> MassSpecificEnergy {
        self.operating.specific_energy
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.operating.condition_wear_ppm_per_active_tick
    }
}

impl ComminutionProcessDefinition {
    #[must_use]
    pub fn new<P>(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        output_particle_size: P,
        operating: PoweredOreProcessProfile,
    ) -> Self
    where
        P: Into<ParticleSizeDistribution>,
    {
        Self {
            process,
            input_form,
            output_form,
            input_particle_size_range: None,
            output_particle_size: output_particle_size.into(),
            operating,
        }
    }

    /// Authors a comminution operation that accepts only particulate feed whose complete envelope
    /// lies inside `input_particle_size_range`.
    ///
    /// This is an equipment/process operating constraint, not a recipe unlock. It lets physically
    /// distinct mill passes reject feed that is too coarse or too fine for the authored operation.
    #[must_use]
    pub fn new_with_input_particle_size_range<P>(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        output_particle_size: P,
        operating: PoweredOreProcessProfile,
    ) -> Self
    where
        P: Into<ParticleSizeDistribution>,
    {
        Self {
            process,
            input_form,
            output_form,
            input_particle_size_range: Some(input_particle_size_range),
            output_particle_size: output_particle_size.into(),
            operating,
        }
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(&self) -> FormId {
        self.input_form
    }

    #[must_use]
    pub const fn output_form(&self) -> FormId {
        self.output_form
    }

    /// Returns the authored admissible particulate feed envelope, when the operation has one.
    #[must_use]
    pub const fn input_particle_size_range(&self) -> Option<ParticleSizeRange> {
        self.input_particle_size_range
    }

    #[must_use]
    pub fn output_particle_size(&self) -> ParticleSizeRange {
        self.output_particle_size.envelope()
    }

    /// Returns the authored weighted size classes produced by this comminution operation.
    #[must_use]
    pub const fn output_particle_size_distribution(&self) -> &ParticleSizeDistribution {
        &self.output_particle_size
    }

    #[must_use]
    pub const fn mass_flow_capability(&self) -> CapabilityId {
        self.operating.mass_flow_capability
    }

    #[must_use]
    pub const fn max_batch_mass_capability(&self) -> CapabilityId {
        self.operating.max_batch_mass_capability
    }

    #[must_use]
    pub const fn energy_carrier(&self) -> EnergyCarrier {
        self.operating.energy_carrier
    }

    #[must_use]
    pub const fn specific_energy(&self) -> MassSpecificEnergy {
        self.operating.specific_energy
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(&self) -> u32 {
        self.operating.condition_wear_ppm_per_active_tick
    }
}

/// Immutable lookup table for ore/material-preparation process semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OreProcessingRegistry {
    comminution: BTreeMap<ProcessId, ComminutionProcessDefinition>,
    screening: BTreeMap<ProcessId, ScreeningProcessDefinition>,
    separation: BTreeMap<ProcessId, ConstituentSeparationProcessDefinition>,
}

impl OreProcessingRegistry {
    #[cfg(test)]
    pub(crate) fn new(definitions: impl IntoIterator<Item = ComminutionProcessDefinition>) -> Self {
        Self::new_with_processes(definitions, std::iter::empty(), std::iter::empty())
    }

    pub(crate) fn new_with_processes(
        comminution_definitions: impl IntoIterator<Item = ComminutionProcessDefinition>,
        screening_definitions: impl IntoIterator<Item = ScreeningProcessDefinition>,
        separation_definitions: impl IntoIterator<Item = ConstituentSeparationProcessDefinition>,
    ) -> Self {
        let mut comminution = BTreeMap::new();
        for definition in comminution_definitions {
            let process = definition.process();
            assert!(
                comminution.insert(process, definition).is_none(),
                "duplicate comminution definition for process {}",
                process.value()
            );
        }
        let mut screening = BTreeMap::new();
        for definition in screening_definitions {
            let process = definition.process();
            assert!(
                !comminution.contains_key(&process),
                "process {} cannot own both comminution and screening semantics",
                process.value()
            );
            assert!(
                screening.insert(process, definition).is_none(),
                "duplicate screening definition for process {}",
                process.value()
            );
        }
        let mut separation = BTreeMap::new();
        for definition in separation_definitions {
            let process = definition.process();
            assert!(
                !comminution.contains_key(&process) && !screening.contains_key(&process),
                "process {} cannot own multiple ore-processing resolver semantics",
                process.value()
            );
            assert!(
                separation.insert(process, definition).is_none(),
                "duplicate constituent-separation definition for process {}",
                process.value()
            );
        }
        Self {
            comminution,
            screening,
            separation,
        }
    }

    #[must_use]
    pub fn get_comminution(&self, process: ProcessId) -> Option<&ComminutionProcessDefinition> {
        self.comminution.get(&process)
    }

    #[must_use]
    pub fn get_screening(&self, process: ProcessId) -> Option<ScreeningProcessDefinition> {
        self.screening.get(&process).copied()
    }

    #[must_use]
    pub fn get_constituent_separation(
        &self,
        process: ProcessId,
    ) -> Option<ConstituentSeparationProcessDefinition> {
        self.separation.get(&process).copied()
    }

    pub(crate) fn process_ids(&self) -> impl Iterator<Item = ProcessId> + '_ {
        self.comminution
            .keys()
            .chain(self.screening.keys())
            .chain(self.separation.keys())
            .copied()
    }

    pub(crate) fn has_process(&self, process: ProcessId) -> bool {
        self.comminution.contains_key(&process)
            || self.screening.contains_key(&process)
            || self.separation.contains_key(&process)
    }

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        capabilities: &CapabilityRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.comminution.values() {
            let process = match production.get_process(definition.process()) {
                Some(process) => process,
                None => panic!(
                    "comminution definition references missing process {}",
                    definition.process().value()
                ),
            };
            assert!(
                matches!(process.input_policy(), ProcessInputPolicy::SelectedBatch),
                "comminution process {} must use selected-batch input policy",
                definition.process().value()
            );
            let rate = match capabilities.get_capability(definition.mass_flow_capability()) {
                Some(capability) => capability,
                None => panic!(
                    "comminution process {} references missing mass-flow capability {}",
                    definition.process().value(),
                    definition.mass_flow_capability().value()
                ),
            };
            assert_eq!(
                rate.kind(),
                CapabilityValueKind::MassFlow,
                "comminution process {} throughput capability must be MassFlow",
                definition.process().value()
            );
            let maximum = match capabilities.get_capability(definition.max_batch_mass_capability())
            {
                Some(capability) => capability,
                None => panic!(
                    "comminution process {} references missing maximum-batch capability {}",
                    definition.process().value(),
                    definition.max_batch_mass_capability().value()
                ),
            };
            assert_eq!(
                maximum.kind(),
                CapabilityValueKind::Mass,
                "comminution process {} maximum-batch capability must be Mass",
                definition.process().value()
            );
            for (form, role) in [
                (definition.input_form(), "input"),
                (definition.output_form(), "output"),
            ] {
                let authored = match materials.get_form(form) {
                    Some(authored) => authored,
                    None => panic!(
                        "comminution process {} references missing {role} form {}",
                        definition.process().value(),
                        form.value()
                    ),
                };
                assert_eq!(
                    authored.phase(),
                    MaterialPhase::Solid,
                    "comminution process {} {role} form {} must be solid",
                    definition.process().value(),
                    form.value()
                );
            }
            let output_form = match materials.get_form(definition.output_form()) {
                Some(output_form) => output_form,
                None => unreachable!("comminution output form was resolved above"),
            };
            assert_eq!(
                output_form.particle_size_policy(),
                ParticleSizeStatePolicy::Required,
                "comminution process {} output form {} must require particle-size state",
                definition.process().value(),
                definition.output_form().value()
            );
            if let Some(input_range) = definition.input_particle_size_range() {
                let input_form = match materials.get_form(definition.input_form()) {
                    Some(input_form) => input_form,
                    None => unreachable!("comminution input form was resolved above"),
                };
                assert_eq!(
                    input_form.particle_size_policy(),
                    ParticleSizeStatePolicy::Required,
                    "comminution process {} with a feed-size range requires particulate input form {}",
                    definition.process().value(),
                    definition.input_form().value()
                );
                let output_range = definition.output_particle_size();
                assert!(
                    output_range.minimum_diameter() <= input_range.minimum_diameter()
                        && output_range.maximum_diameter() < input_range.maximum_diameter(),
                    "comminution process {} feed-size range {}..={} um cannot admit a strictly reducing output {}..={} um",
                    definition.process().value(),
                    input_range.minimum_diameter().micrometers(),
                    input_range.maximum_diameter().micrometers(),
                    output_range.minimum_diameter().micrometers(),
                    output_range.maximum_diameter().micrometers()
                );
            }
        }
        for definition in self.screening.values().copied() {
            let process = match production.get_process(definition.process()) {
                Some(process) => process,
                None => panic!(
                    "screening definition references missing process {}",
                    definition.process().value()
                ),
            };
            assert!(
                matches!(process.input_policy(), ProcessInputPolicy::SelectedBatch),
                "screening process {} must use selected-batch input policy",
                definition.process().value()
            );
            let rate = capabilities
                .get_capability(definition.mass_flow_capability())
                .unwrap_or_else(|| {
                    panic!(
                        "screening process {} references missing mass-flow capability {}",
                        definition.process().value(),
                        definition.mass_flow_capability().value()
                    )
                });
            assert_eq!(
                rate.kind(),
                CapabilityValueKind::MassFlow,
                "screening process {} throughput capability must be MassFlow",
                definition.process().value()
            );
            let maximum = capabilities
                .get_capability(definition.max_batch_mass_capability())
                .unwrap_or_else(|| {
                    panic!(
                        "screening process {} references missing maximum-batch capability {}",
                        definition.process().value(),
                        definition.max_batch_mass_capability().value()
                    )
                });
            assert_eq!(
                maximum.kind(),
                CapabilityValueKind::Mass,
                "screening process {} maximum-batch capability must be Mass",
                definition.process().value()
            );
            for (form, role) in [
                (definition.input_form(), "input"),
                (definition.output_form(), "output"),
            ] {
                let authored = materials.get_form(form).unwrap_or_else(|| {
                    panic!(
                        "screening process {} references missing {role} form {}",
                        definition.process().value(),
                        form.value()
                    )
                });
                assert_eq!(
                    authored.phase(),
                    MaterialPhase::Solid,
                    "screening process {} {role} form {} must be solid",
                    definition.process().value(),
                    form.value()
                );
                assert_eq!(
                    authored.particle_size_policy(),
                    ParticleSizeStatePolicy::Required,
                    "screening process {} {role} form {} must require particle-size state",
                    definition.process().value(),
                    form.value()
                );
            }
        }
        for definition in self.separation.values().copied() {
            let process = production
                .get_process(definition.process())
                .unwrap_or_else(|| {
                    panic!(
                        "constituent-separation definition references missing process {}",
                        definition.process().value()
                    )
                });
            assert!(
                matches!(process.input_policy(), ProcessInputPolicy::SelectedBatch),
                "constituent-separation process {} must use selected-batch input policy",
                definition.process().value()
            );
            for (capability, kind, role) in [
                (
                    definition.mass_flow_capability(),
                    CapabilityValueKind::MassFlow,
                    "throughput",
                ),
                (
                    definition.max_batch_mass_capability(),
                    CapabilityValueKind::Mass,
                    "maximum-batch",
                ),
            ] {
                let authored = capabilities.get_capability(capability).unwrap_or_else(|| {
                    panic!(
                        "constituent-separation process {} references missing {role} capability {}",
                        definition.process().value(),
                        capability.value()
                    )
                });
                assert_eq!(
                    authored.kind(),
                    kind,
                    "constituent-separation process {} {role} capability has wrong physical kind",
                    definition.process().value()
                );
            }
            let input_form = materials
                .get_form(definition.input_form())
                .unwrap_or_else(|| {
                    panic!(
                        "constituent-separation process {} references missing input form {}",
                        definition.process().value(),
                        definition.input_form().value()
                    )
                });
            assert_eq!(
                input_form.phase(),
                MaterialPhase::Solid,
                "constituent-separation process {} input must be solid",
                definition.process().value()
            );
            assert_eq!(
                input_form.particle_size_policy(),
                ParticleSizeStatePolicy::Required,
                "constituent-separation process {} requires liberated particulate feed",
                definition.process().value()
            );
            let target_material = definition.target_material();
            let target_form = definition.target_output_form();
            assert!(
                materials.get_material(target_material).is_some(),
                "constituent-separation process {} references missing target material {}",
                definition.process().value(),
                target_material.value()
            );
            assert!(
                materials.has_commodity(CommodityKey::new(target_material, target_form)),
                "constituent-separation process {} references invalid target material/form {}:{}",
                definition.process().value(),
                target_material.value(),
                target_form.value()
            );
            let target_output = materials
                .get_form(target_form)
                .unwrap_or_else(|| unreachable!("validated target commodity requires its form"));
            assert_eq!(target_output.phase(), MaterialPhase::Solid);
            assert_eq!(
                target_output.particle_size_policy(),
                ParticleSizeStatePolicy::Untracked,
                "constituent-separation process {} target output has incompatible particle-state policy",
                definition.process().value()
            );

            let residue_form = definition.residue_output_form();
            let residue_output = materials.get_form(residue_form).unwrap_or_else(|| {
                panic!(
                    "constituent-separation process {} references missing residue form {}",
                    definition.process().value(),
                    residue_form.value()
                )
            });
            assert_eq!(residue_output.phase(), MaterialPhase::Solid);
            assert_eq!(
                residue_output.particle_size_policy(),
                ParticleSizeStatePolicy::Required,
                "constituent-separation process {} residue output must retain particle-size state",
                definition.process().value()
            );
            if let Some(residue_material) = definition.residue_material() {
                assert!(
                    materials.has_commodity(CommodityKey::new(residue_material, residue_form)),
                    "constituent-separation process {} references invalid residue material/form {}:{}",
                    definition.process().value(),
                    residue_material.value(),
                    residue_form.value()
                );
                assert_ne!(
                    residue_material, target_material,
                    "binary constituent separation must use a residue material distinct from its target"
                );
            }
        }
    }
}

/// Failure to convert material throughput into a whole authoritative tick span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassFlowDurationError {
    ZeroRate,
    TickRangeExceeded,
}

impl Display for MassFlowDurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroRate => formatter.write_str("material processing rate must be nonzero"),
            Self::TickRangeExceeded => {
                formatter.write_str("material processing duration exceeds authoritative tick range")
            }
        }
    }
}

impl Error for MassFlowDurationError {}

/// Returns the minimum whole tick span required to process an exact mass at a constant mass flow.
pub fn calculate_mass_flow_duration_ceiling(
    rate: MassFlow,
    mass: Mass,
    physical_tick_duration: PhysicalTickDuration,
) -> Result<TickSpan, MassFlowDurationError> {
    if rate.is_zero() {
        return Err(MassFlowDurationError::ZeroRate);
    }
    if mass.is_zero() {
        return Ok(TickSpan::ZERO);
    }
    let numerator = u128::from(mass.milligrams()) * 1_000_000;
    let denominator = u128::from(rate.milligrams_per_second())
        * u128::from(physical_tick_duration.microseconds());
    let ticks = numerator.div_ceil(denominator);
    let ticks = u64::try_from(ticks).map_err(|_| MassFlowDurationError::TickRangeExceeded)?;
    Ok(TickSpan::new(ticks))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
