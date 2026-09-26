//! Cross-owner trusted-load replay for acquired geological evidence.

use crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;
use crate::geology::GeologyState;
use crate::geology::coverage::VoxelCoverage;
use crate::geology::state::GeologicalDepositRecord;
use crate::labor::{LaborRegistry, ProspectingDefinition};
use crate::material::MaterialId;

use super::super::{
    ExcavationHardnessEstimate, GeologicalKnowledgeState, GeologicalObservationId,
    GeologicalObservationRecord, MaterialAbundanceEstimate, ResourceMassEstimate,
};
use super::GeologicalKnowledgeValidationError;
use crate::geology::prospecting_action::{
    excavation_hardness_band_matches_resolution, resource_mass_band_matches_resolution,
};

/// Validates persisted acquired evidence against authored methods and geological bodies that
/// could have existed when each observation was acquired.
///
/// Authored method replay prevents persistence from inventing a footprint or precision that no
/// current method can produce. Bodies that were available at the observation tick constrain
/// abundance and hardness conservatively. Historical resource mass is time-varying, so its replay
/// accepts only an exact-footprint body whose observation-time remaining mass could have fallen
/// between immutable initial mass and current remaining mass.
pub(crate) fn validate_loaded_geological_evidence_against_world(
    labor: &LaborRegistry,
    geology: &GeologyState,
    knowledge: &GeologicalKnowledgeState,
) -> Result<(), GeologicalKnowledgeValidationError> {
    for (observation, record) in &knowledge.observations {
        let [finding] = record.findings.as_slice() else {
            return Err(
                GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                    observation: *observation,
                    evidence: record.evidence,
                },
            );
        };
        if !labor
            .prospecting_definitions()
            .copied()
            .any(|method| authored_method_can_emit_observation(method, geology, record, *finding))
        {
            return Err(
                GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                    observation: *observation,
                    evidence: record.evidence,
                },
            );
        }
        let material = finding.material();
        if let Some(hardness) = record.excavation_hardness {
            validate_loaded_hardness_against_geology(
                geology,
                *observation,
                record,
                material,
                hardness,
            )?;
        }
        if let Some(resource_mass) = record.resource_mass {
            validate_loaded_resource_mass_against_geology(
                geology,
                *observation,
                record,
                material,
                resource_mass,
            )?;
        }
    }
    Ok(())
}

fn authored_method_can_emit_observation(
    method: ProspectingDefinition,
    geology: &GeologyState,
    record: &GeologicalObservationRecord,
    finding: MaterialAbundanceEstimate,
) -> bool {
    if method.evidence() != record.evidence
        || method.resolve_region_observation_count(record.region) != Ok(1)
        || !abundance_band_matches_uncertainty(finding, method.abundance_uncertainty_ppm())
        || !abundance_band_is_conservative_for_world(method, geology, record, finding)
    {
        return false;
    }
    match (
        record.excavation_hardness,
        method.excavation_hardness_resolution(),
    ) {
        (Some(hardness), Some(resolution)) => {
            if !excavation_hardness_band_matches_resolution(hardness, resolution) {
                return false;
            }
        }
        (Some(_), None) => return false,
        (None, Some(_)) if finding.lower_ppm() > 0 => return false,
        (None, Some(_) | None) => {}
    }
    if let Some(resource_mass) = record.resource_mass {
        let Some(resolution) = method.resource_mass_resolution() else {
            return false;
        };
        if !resource_mass_band_matches_resolution(resource_mass, resolution) {
            return false;
        }
    }
    true
}

fn abundance_band_matches_uncertainty(
    finding: MaterialAbundanceEstimate,
    uncertainty_ppm: u32,
) -> bool {
    let lower = finding.lower_ppm();
    let upper = finding.upper_ppm();
    let uncertainty = u64::from(uncertainty_ppm);
    if lower == 0 {
        return u64::from(upper) >= uncertainty;
    }
    if upper == NORMALIZED_PARTS_PER_MILLION {
        return u64::from(lower) + uncertainty <= u64::from(NORMALIZED_PARTS_PER_MILLION);
    }
    u64::from(upper - lower) >= uncertainty.saturating_mul(2)
}

fn abundance_band_is_conservative_for_world(
    method: ProspectingDefinition,
    geology: &GeologyState,
    record: &GeologicalObservationRecord,
    finding: MaterialAbundanceEstimate,
) -> bool {
    let uncertainty = method.abundance_uncertainty_ppm();
    let material = finding.material();
    let historical_bounds_are_contained = geology
        .deposits()
        .filter(|deposit| {
            deposit.was_available_at(record.observed_at)
                && deposit.bounds().has_intersection(record.region)
        })
        .all(|deposit| {
            let abundance = deposit.composition().parts_per_million(material);
            finding.lower_ppm() <= abundance.saturating_sub(uncertainty)
                && finding.upper_ppm()
                    >= abundance
                        .saturating_add(uncertainty)
                        .min(NORMALIZED_PARTS_PER_MILLION)
        });
    if !historical_bounds_are_contained {
        return false;
    }
    finding.lower_ppm() == 0
        || record.excavation_hardness.is_some()
        || record.resource_mass.is_some()
        || region_could_have_been_fully_covered(geology, record)
}

fn region_could_have_been_fully_covered(
    geology: &GeologyState,
    record: &GeologicalObservationRecord,
) -> bool {
    let mut coverage = VoxelCoverage::new(record.region);
    for deposit in geology.deposits().filter(|deposit| {
        deposit.was_available_at(record.observed_at)
            && deposit.bounds().has_intersection(record.region)
    }) {
        coverage.cover(deposit.bounds());
        if coverage.is_complete() {
            return true;
        }
    }
    coverage.is_complete()
}

fn validate_loaded_hardness_against_geology(
    geology: &GeologyState,
    observation: GeologicalObservationId,
    record: &GeologicalObservationRecord,
    material: MaterialId,
    hardness: ExcavationHardnessEstimate,
) -> Result<(), GeologicalKnowledgeValidationError> {
    let mut plausible_historical_match = false;
    for deposit in geology.deposits().filter(|deposit| {
        deposit.was_available_at(record.observed_at)
            && deposit.bounds().has_intersection(record.region)
            && deposit.composition().parts_per_million(material) > 0
    }) {
        let actual = deposit.excavation_hardness();
        let inside_band = actual >= hardness.lower() && actual <= hardness.upper();
        if !inside_band {
            return Err(
                GeologicalKnowledgeValidationError::ExcavationHardnessContradictsHistoricalDeposit {
                    observation,
                    deposit: deposit.id(),
                    lower: hardness.lower(),
                    upper: hardness.upper(),
                    actual,
                },
            );
        }
        plausible_historical_match |= inside_band;
    }
    if plausible_historical_match {
        return Ok(());
    }
    Err(
        GeologicalKnowledgeValidationError::ExcavationHardnessCannotMatchHistoricalDeposit {
            observation,
            material,
            lower: hardness.lower(),
            upper: hardness.upper(),
        },
    )
}

fn validate_loaded_resource_mass_against_geology(
    geology: &GeologyState,
    observation: GeologicalObservationId,
    record: &GeologicalObservationRecord,
    material: MaterialId,
    resource_mass: ResourceMassEstimate,
) -> Result<(), GeologicalKnowledgeValidationError> {
    let can_match_mass = |deposit: &GeologicalDepositRecord| {
        let candidate = resource_mass.lower().max(deposit.remaining_mass());
        candidate <= deposit.initial_mass()
            && (candidate < resource_mass.upper()
                || (candidate.milligrams() == u64::MAX
                    && resource_mass.upper().milligrams() == u64::MAX))
    };
    let available_at_observation = |deposit: &GeologicalDepositRecord| {
        deposit.was_available_at(record.observed_at)
            && deposit.composition().parts_per_million(material) > 0
    };

    let mut historical_matches = geology.deposits().filter(|deposit| {
        available_at_observation(deposit) && deposit.bounds().has_intersection(record.region)
    });
    if let Some(historical) = historical_matches.next() {
        let unique = historical_matches.next().is_none();
        if unique && historical.bounds() == record.region && can_match_mass(historical) {
            return Ok(());
        }
    }

    Err(
        GeologicalKnowledgeValidationError::ResourceMassCannotMatchHistoricalDeposit {
            observation,
            material,
            lower: resource_mass.lower(),
            upper: resource_mass.upper(),
        },
    )
}
