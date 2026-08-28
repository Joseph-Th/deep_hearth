//! Fixed-point structural damage closure and final assessment construction.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::Force;
use crate::material::MaterialRegistry;

use super::{
    LoadProjection, StructuralAnalysis, StructuralAnalysisError, StructuralAnalysisOverlay,
    StructuralAssessment, StructuralDamageEvent, StructuralFailureCause, StructuralStage,
    calculate_structural_utilization_ppm, expand_unsupported_failures, is_at_or_above_fraction,
    pristine_capacity, project_loads, scale_capacity,
};
use crate::structural::definitions::{StructuralProfileDefinition, StructuralRegistry};
use crate::structural::state::{StructuralElementId, StructuralLifecycle, StructureState};

struct StructuralDamageState {
    failed: BTreeSet<StructuralElementId>,
    cracked: BTreeSet<StructuralElementId>,
    initially_failed: BTreeSet<StructuralElementId>,
    initially_cracked: BTreeSet<StructuralElementId>,
    failure_causes: BTreeMap<StructuralElementId, StructuralFailureCause>,
    crack_context: BTreeMap<StructuralElementId, (Force, Force)>,
}

struct StructuralDamageDelta {
    failures: BTreeMap<StructuralElementId, StructuralFailureCause>,
    cracks: BTreeMap<StructuralElementId, (Force, Force)>,
}

impl StructuralDamageDelta {
    fn is_empty(&self) -> bool {
        self.failures.is_empty() && self.cracks.is_empty()
    }
}

struct StructuralAnalysisContext<'a> {
    profiles: &'a StructuralRegistry,
    materials: &'a MaterialRegistry,
    state: &'a StructureState,
    overlay: &'a StructuralAnalysisOverlay,
    scope: &'a BTreeSet<StructuralElementId>,
}

impl StructuralAnalysisContext<'_> {
    fn validate_scope_loads(&self) -> Result<(), StructuralAnalysisError> {
        for element in self.scope {
            let Some(record) = self.state.element_map().get(element) else {
                continue;
            };
            if self.overlay.sum_applied_load(record).is_none() {
                return Err(StructuralAnalysisError::AppliedLoadOverflow { element: record.id });
            }
        }
        Ok(())
    }

    fn initial_damage_state(&self) -> StructuralDamageState {
        let mut failed = BTreeSet::new();
        let mut cracked = BTreeSet::new();
        for element in self.scope {
            let Some(record) = self.state.element_map().get(element) else {
                continue;
            };
            if self.overlay.is_removed(record.id) {
                continue;
            }
            if self.overlay.lifecycle(record) == StructuralLifecycle::Failed {
                failed.insert(record.id);
            }
            if record.is_cracked {
                cracked.insert(record.id);
            }
        }
        StructuralDamageState {
            initially_failed: failed.clone(),
            initially_cracked: cracked.clone(),
            failed,
            cracked,
            failure_causes: BTreeMap::new(),
            crack_context: BTreeMap::new(),
        }
    }

    fn assess_damage_delta(
        &self,
        damage: &StructuralDamageState,
        projection: &LoadProjection,
    ) -> Result<StructuralDamageDelta, StructuralAnalysisError> {
        let mut failures = BTreeMap::new();
        let mut cracks = BTreeMap::new();
        for (element, load) in &projection.carried {
            let record = &self.state.element_map()[element];
            let profile = self.profiles.get_profile(record.profile()).ok_or(
                StructuralAnalysisError::UnknownProfile {
                    element: *element,
                    profile: record.profile(),
                },
            )?;
            let pristine = pristine_capacity(self.profiles, self.materials, self.state, *element)?;
            let effective = effective_capacity(pristine, profile, damage.cracked.contains(element));
            if *load > effective {
                failures.insert(
                    *element,
                    StructuralFailureCause::Overloaded {
                        carried_load: *load,
                        effective_capacity: effective,
                    },
                );
                continue;
            }
            if !damage.cracked.contains(element)
                && is_at_or_above_fraction(*load, pristine, profile.cracking_at_ppm())
            {
                cracks.insert(*element, (*load, pristine));
            }
        }
        failures.retain(|element, _| !damage.failed.contains(element));
        cracks.retain(|element, _| {
            !damage.cracked.contains(element) && !failures.contains_key(element)
        });
        Ok(StructuralDamageDelta { failures, cracks })
    }

    fn resolve_damage_closure(
        &self,
        mut damage: StructuralDamageState,
    ) -> Result<(StructuralDamageState, LoadProjection), StructuralAnalysisError> {
        loop {
            damage.add_unsupported(expand_unsupported_failures(
                self.state,
                &damage.failed,
                self.overlay,
                self.scope,
            ));
            let projection = project_loads(self.state, &damage.failed, self.overlay, self.scope)?;
            let delta = self.assess_damage_delta(&damage, &projection)?;
            if delta.is_empty() {
                return Ok((damage, projection));
            }
            damage.apply_delta(delta);
        }
    }

    fn build_assessments(
        &self,
        damage: &StructuralDamageState,
        final_projection: &LoadProjection,
    ) -> Result<Vec<StructuralAssessment>, StructuralAnalysisError> {
        let mut assessments = Vec::new();
        for element in self.scope {
            let Some(record) = self.state.element_map().get(element) else {
                continue;
            };
            if self.overlay.is_removed(record.id)
                || self.overlay.lifecycle(record) == StructuralLifecycle::Planned
            {
                continue;
            }
            let pristine = pristine_capacity(self.profiles, self.materials, self.state, record.id)?;
            let profile = self.profiles.get_profile(record.profile()).ok_or(
                StructuralAnalysisError::UnknownProfile {
                    element: record.id,
                    profile: record.profile(),
                },
            )?;
            let is_failed = damage.failed.contains(&record.id);
            let is_cracked = damage.cracked.contains(&record.id);
            let effective = effective_capacity(pristine, profile, is_cracked);
            let carried_load = assessment_carried_load(record.id, damage, final_projection);
            let stage = if is_failed {
                StructuralStage::Failed
            } else if is_cracked {
                StructuralStage::Cracking
            } else if is_at_or_above_fraction(carried_load, pristine, profile.strained_at_ppm()) {
                StructuralStage::Strained
            } else {
                StructuralStage::Stable
            };
            assessments.push(StructuralAssessment {
                element: record.id,
                carried_load,
                pristine_capacity: pristine,
                effective_capacity: effective,
                utilization_ppm: calculate_structural_utilization_ppm(carried_load, effective),
                stage,
            });
        }
        Ok(assessments)
    }
}

impl StructuralDamageState {
    fn add_unsupported(&mut self, unsupported: BTreeSet<StructuralElementId>) {
        for element in unsupported {
            self.failed.insert(element);
            self.cracked.insert(element);
            self.failure_causes
                .insert(element, StructuralFailureCause::Unsupported);
        }
    }

    fn apply_delta(&mut self, delta: StructuralDamageDelta) {
        for (element, cause) in delta.failures {
            self.failed.insert(element);
            self.cracked.insert(element);
            self.failure_causes.insert(element, cause);
        }
        for (element, context) in delta.cracks {
            self.cracked.insert(element);
            self.crack_context.insert(element, context);
        }
    }
}

fn effective_capacity(
    pristine: Force,
    profile: &StructuralProfileDefinition,
    is_cracked: bool,
) -> Force {
    if is_cracked {
        scale_capacity(pristine, profile.cracked_capacity_ppm())
    } else {
        pristine
    }
}

fn assessment_carried_load(
    element: StructuralElementId,
    damage: &StructuralDamageState,
    final_projection: &LoadProjection,
) -> Force {
    if damage.failed.contains(&element) {
        return match damage.failure_causes.get(&element) {
            Some(StructuralFailureCause::Overloaded { carried_load, .. }) => *carried_load,
            Some(StructuralFailureCause::Unsupported) | None => Force::ZERO,
        };
    }
    final_projection
        .carried
        .get(&element)
        .copied()
        .unwrap_or(Force::ZERO)
}

fn build_damage_events(damage: &StructuralDamageState) -> Vec<StructuralDamageEvent> {
    let mut damage_events = Vec::new();
    for element in damage.cracked.difference(&damage.initially_cracked) {
        if damage.failed.contains(element) {
            continue;
        }
        let (carried_load, pristine_capacity) = damage.crack_context[element];
        damage_events.push(StructuralDamageEvent::Cracked {
            element: *element,
            carried_load,
            pristine_capacity,
        });
    }
    for element in damage.failed.difference(&damage.initially_failed) {
        damage_events.push(StructuralDamageEvent::Failed {
            element: *element,
            cause: damage.failure_causes[element],
        });
    }
    damage_events.sort_by_key(|event| event.element());
    damage_events
}

pub(super) fn analyze_structure_scoped(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    overlay: &StructuralAnalysisOverlay,
    scope: &BTreeSet<StructuralElementId>,
) -> Result<StructuralAnalysis, StructuralAnalysisError> {
    let context = StructuralAnalysisContext {
        profiles,
        materials,
        state,
        overlay,
        scope,
    };
    context.validate_scope_loads()?;
    let (damage, final_projection) =
        context.resolve_damage_closure(context.initial_damage_state())?;
    Ok(StructuralAnalysis {
        assessments: context.build_assessments(&damage, &final_projection)?,
        damage_events: build_damage_events(&damage),
    })
}
