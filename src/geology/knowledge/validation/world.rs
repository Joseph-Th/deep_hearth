//! Cross-owner trusted-load replay for acquired geological evidence.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::geology::state::GeologicalDepositRecord;
use crate::geology::{GeologicalDepositId, GeologyState};
use crate::labor::{LaborRegistry, ProspectingDefinition};
use crate::material::MaterialId;

use super::super::{
    ExcavationHardnessEstimate, GeologicalKnowledgeState, GeologicalObservationId,
    GeologicalObservationRecord, MaterialAbundanceEstimate, ResourceMassEstimate,
};
use super::GeologicalKnowledgeValidationError;
use crate::geology::prospecting_action::{
    localized_resource_body, maximum_historical_resource_mass,
    resolve_region_abundance_bounds_from_deposits,
    resolve_region_excavation_hardness_from_deposits,
};

/// Validates persisted acquired evidence against authored methods and geological bodies that
/// could have existed when each observation was acquired.
///
/// Authored method replay prevents persistence from inventing a footprint or precision that no
/// current method can produce. Abundance and excavation-hardness evidence is recomputed with the
/// same quantization used by live prospecting over bodies available at the observation tick.
/// Historical resource mass is time-varying. Require the measurement exactly when the authored
/// method and localized body would emit it, and require a canonical bucket for some positive
/// mass between immutable initial mass and current remaining mass. Each
/// subsequent sample must also be possible without regrowing the same body.
pub(crate) fn validate_loaded_geological_evidence_against_world(
    labor: &LaborRegistry,
    geology: &GeologyState,
    knowledge: &GeologicalKnowledgeState,
) -> Result<(), GeologicalKnowledgeValidationError> {
    // The knowledge owner's load validation establishes nondecreasing
    // acquisition ticks in persistent observation-ID order. Keep the largest
    // feasible mass at the latest sample of each physical body: choosing any
    // smaller mass could only make its subsequent history harder to satisfy.
    let mut mass_history = BTreeMap::<GeologicalDepositId, (GeologicalObservationId, Mass)>::new();
    for (observation, record) in &knowledge.observations {
        let [finding] = record.findings.as_slice() else {
            return Err(
                GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                    observation: *observation,
                    evidence: record.evidence,
                },
            );
        };
        let material = finding.material();
        let historical_deposits: Vec<_> = geology
            .deposits_intersecting(record.region)
            .filter(|deposit| deposit.was_available_at(record.observed_at))
            .collect();
        let localized_body =
            localized_resource_body(historical_deposits.iter().copied(), record.region, material);
        if let Some(hardness) = record.excavation_hardness {
            validate_loaded_hardness_against_geology(
                &historical_deposits,
                *observation,
                material,
                hardness,
            )?;
        }
        if let Some(resource_mass) = record.resource_mass {
            validate_loaded_resource_mass_against_geology(
                localized_body,
                *observation,
                material,
                resource_mass,
            )?;
        }
        if !labor.prospecting_definitions().copied().any(|method| {
            authored_method_can_emit_observation(
                method,
                &historical_deposits,
                localized_body,
                record,
                *finding,
            )
        }) {
            return Err(
                GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                    observation: *observation,
                    evidence: record.evidence,
                },
            );
        }
        if let (Some(resource_mass), Some(deposit)) = (record.resource_mass, localized_body) {
            validate_resource_mass_history(
                &mut mass_history,
                *observation,
                deposit,
                resource_mass,
            )?;
        }
    }
    Ok(())
}

fn validate_resource_mass_history(
    history: &mut BTreeMap<GeologicalDepositId, (GeologicalObservationId, Mass)>,
    observation: GeologicalObservationId,
    deposit: &GeologicalDepositRecord,
    resource_mass: ResourceMassEstimate,
) -> Result<(), GeologicalKnowledgeValidationError> {
    // A canonical bucket always has exactly its authored resolution's width,
    // including the widened ceiling bucket. The individual method replay
    // has already proved that this resolution is applicable.
    let bound = history
        .entry(deposit.id())
        .or_insert((observation, deposit.initial_mass()));
    let (earlier, prior_maximum) = *bound;
    let maximum = maximum_historical_resource_mass(
        resource_mass,
        resource_mass.width(),
        deposit.remaining_mass(),
        prior_maximum,
    )
    .ok_or(
        GeologicalKnowledgeValidationError::ResourceMassIncreasesAcrossObservations {
            deposit: deposit.id(),
            earlier,
            observation,
        },
    )?;
    // Keep the observation that actually imposed the limiting bound, not a
    // newer, wider measurement that happens to include it.
    if maximum < prior_maximum {
        *bound = (observation, maximum);
    }
    Ok(())
}

fn authored_method_can_emit_observation(
    method: ProspectingDefinition,
    historical_deposits: &[&GeologicalDepositRecord],
    localized_body: Option<&GeologicalDepositRecord>,
    record: &GeologicalObservationRecord,
    finding: MaterialAbundanceEstimate,
) -> bool {
    if method.evidence() != record.evidence
        || method.resolve_region_observation_count(record.region) != Ok(1)
    {
        return false;
    }
    let expected_abundance = resolve_region_abundance_bounds_from_deposits(
        historical_deposits.iter().copied(),
        record.region,
        finding.material(),
        method.abundance_uncertainty_ppm(),
    );
    if (finding.lower_ppm(), finding.upper_ppm()) != expected_abundance {
        return false;
    }
    let expected_hardness = method
        .excavation_hardness_resolution()
        .filter(|_| finding.lower_ppm() > 0)
        .and_then(|resolution| {
            resolve_region_excavation_hardness_from_deposits(
                historical_deposits.iter().copied(),
                record.region,
                finding.material(),
                resolution,
            )
        });
    if record.excavation_hardness != expected_hardness {
        return false;
    }
    let expected_measurement = method
        .resource_mass_resolution()
        .filter(|_| finding.lower_ppm() > 0)
        .zip(localized_body);
    match (record.resource_mass, expected_measurement) {
        (Some(resource_mass), Some((resolution, deposit))) => maximum_historical_resource_mass(
            resource_mass,
            resolution,
            deposit.remaining_mass(),
            deposit.initial_mass(),
        )
        .is_some(),
        (None, None) => true,
        _ => false,
    }
}

fn validate_loaded_hardness_against_geology(
    historical_deposits: &[&GeologicalDepositRecord],
    observation: GeologicalObservationId,
    material: MaterialId,
    hardness: ExcavationHardnessEstimate,
) -> Result<(), GeologicalKnowledgeValidationError> {
    let mut plausible_historical_match = false;
    for deposit in historical_deposits
        .iter()
        .copied()
        .filter(|deposit| deposit.composition().parts_per_million(material) > 0)
    {
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
    localized_body: Option<&GeologicalDepositRecord>,
    observation: GeologicalObservationId,
    material: MaterialId,
    resource_mass: ResourceMassEstimate,
) -> Result<(), GeologicalKnowledgeValidationError> {
    let can_match_mass = |deposit: &GeologicalDepositRecord| {
        let candidate = resource_mass
            .lower()
            .max(deposit.remaining_mass())
            .max(Mass::from_milligrams(1));
        candidate <= deposit.initial_mass()
            && (candidate < resource_mass.upper()
                || (candidate.milligrams() == u64::MAX
                    && resource_mass.upper().milligrams() == u64::MAX))
    };
    if localized_body.is_some_and(can_match_mass) {
        return Ok(());
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
