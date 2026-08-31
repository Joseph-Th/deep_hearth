//! Crusher feasibility, adaptive batching, and power-choice planning for workshop scenarios.

use super::*;

pub(super) struct CrushOption {
    pub(super) name: &'static str,
    pub(super) store: EnergyStoreId,
    pub(super) stored_before: Energy,
    pub(super) resolved: ResolvedComminution,
}

pub(super) struct CrushBatchPlan {
    pub(super) mass: Mass,
    pub(super) small: Option<CrushOption>,
    pub(super) large: Option<CrushOption>,
    pub(super) energy_limited: bool,
    pub(super) equipment_capacity_limited: bool,
    pub(super) condition_lifetime_limited: bool,
    pub(super) maintenance_limited: bool,
}

pub(super) enum CrushBatchSearch {
    Available(Box<CrushBatchPlan>),
    EnergyUnavailable,
    MaintenanceBlocked,
}

struct CrushOptions {
    small: Option<CrushOption>,
    large: Option<CrushOption>,
}

impl CrushOptions {
    fn has_viable_option(&self) -> bool {
        self.small.is_some() || self.large.is_some()
    }

    fn into_options(self) -> (Option<CrushOption>, Option<CrushOption>) {
        (self.small, self.large)
    }
}

#[derive(Clone, Copy, Default)]
struct CrushConstraintFlags {
    stored_energy: bool,
    equipment_capacity: bool,
    condition_lifetime: bool,
}

impl CrushConstraintFlags {
    fn from_envelopes(envelopes: CrushEnvelopes, desired: Mass) -> Self {
        let mut flags = Self::default();
        for constraint in [
            envelopes.small.constraint_for(desired),
            envelopes.large.constraint_for(desired),
        ]
        .into_iter()
        .flatten()
        {
            match constraint {
                PoweredOreMassConstraint::EquipmentCapacity => flags.equipment_capacity = true,
                PoweredOreMassConstraint::StoredEnergy => flags.stored_energy = true,
                PoweredOreMassConstraint::ConditionLifetime => flags.condition_lifetime = true,
            }
        }
        flags
    }
}

#[derive(Clone, Copy)]
struct CrushEnvelopes {
    small: PoweredOreMassEnvelope,
    large: PoweredOreMassEnvelope,
}

impl CrushEnvelopes {
    fn maximum_mass(self) -> Mass {
        std::cmp::max(self.small.maximum_mass(), self.large.maximum_mass())
    }

    fn maximum_mass_preserving_condition_above(self, floor: Condition) -> Mass {
        std::cmp::max(
            self.small.maximum_mass_preserving_condition_above(floor),
            self.large.maximum_mass_preserving_condition_above(floor),
        )
    }

    fn minimum_is_maintenance_only(self) -> bool {
        let minimum = Mass::from_milligrams(1);
        [
            self.small.constraint_for(minimum),
            self.large.constraint_for(minimum),
        ]
        .into_iter()
        .all(|constraint| {
            matches!(
                constraint,
                Some(
                    PoweredOreMassConstraint::EquipmentCapacity
                        | PoweredOreMassConstraint::ConditionLifetime
                )
            )
        })
    }
}

#[cfg(test)]
#[path = "crush_planning_tests.rs"]
mod tests;

struct ResolvableCrushBatch {
    mass: Mass,
    options: CrushOptions,
    desired_constraints: CrushConstraintFlags,
}

#[derive(Clone, Copy)]
pub(super) struct CrushChoiceContext {
    pub(super) thresholds: deep_hearth::maintenance::MaintenanceThresholds,
    pub(super) preference: PowerPreference,
}

fn resolve_crush_option(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
    name: &'static str,
    store: EnergyStoreId,
) -> Result<CrushOption, PoweredOreMassConstraint> {
    let stored_before = state
        .energy()
        .get_store(store)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("gameplay harness {name} drive disappeared"));
    let selection = [MaterialLotSelection::new(ids.ore_lot, mass)];
    match resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ids.ore_source,
            &selection,
            ids.crusher,
            store,
        ),
    ) {
        Ok(resolved) => Ok(CrushOption {
            name,
            store,
            stored_before,
            resolved,
        }),
        Err(ComminutionResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            ..
        })) => Err(PoweredOreMassConstraint::StoredEnergy),
        Err(ComminutionResolutionError::BatchMassExceeded { .. }) => {
            Err(PoweredOreMassConstraint::EquipmentCapacity)
        }
        Err(ComminutionResolutionError::ConditionDuration(_)) => {
            Err(PoweredOreMassConstraint::ConditionLifetime)
        }
        Err(error) => panic!("gameplay harness {name} drive resolution failed: {error}"),
    }
}

fn assess_crush_envelope(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    name: &'static str,
    store: EnergyStoreId,
) -> PoweredOreMassEnvelope {
    assess_powered_ore_mass_envelope(registries, state, PROCESS_CRUSH_ORE, ids.crusher, store)
        .unwrap_or_else(|error| panic!("gameplay harness {name} drive planning failed: {error}"))
}

fn assess_crush_envelopes(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
) -> CrushEnvelopes {
    CrushEnvelopes {
        small: assess_crush_envelope(registries, state, ids, "small", ids.small_drive),
        large: assess_crush_envelope(registries, state, ids, "large", ids.large_drive),
    }
}

fn resolve_crush_options(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
) -> CrushOptions {
    let small = resolve_crush_option(registries, state, ids, mass, "small", ids.small_drive).ok();
    let large = resolve_crush_option(registries, state, ids, mass, "large", ids.large_drive).ok();
    CrushOptions { small, large }
}

fn largest_resolvable_crush_batch(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    desired: Mass,
) -> Option<ResolvableCrushBatch> {
    if desired.is_zero() {
        return None;
    }
    let envelopes = assess_crush_envelopes(registries, state, ids);
    let mass = std::cmp::min(desired, envelopes.maximum_mass());
    if mass.is_zero() {
        return None;
    }
    let options = resolve_crush_options(registries, state, ids, mass);
    assert!(
        options.has_viable_option(),
        "powered crush envelope admitted a mass rejected by both canonical supply choices"
    );
    Some(ResolvableCrushBatch {
        mass,
        options,
        desired_constraints: CrushConstraintFlags::from_envelopes(envelopes, desired),
    })
}

fn maintenance_safe_crush_options(
    options: (Option<CrushOption>, Option<CrushOption>),
    thresholds: deep_hearth::maintenance::MaintenanceThresholds,
) -> (Option<CrushOption>, Option<CrushOption>) {
    let keep_safe = |option: CrushOption| {
        (thresholds.classify(option.resolved.condition_after()) != MaintenanceBand::Critical)
            .then_some(option)
    };
    (options.0.and_then(keep_safe), options.1.and_then(keep_safe))
}

pub(super) fn largest_safe_powered_crush_batch(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    desired: Mass,
    thresholds: deep_hearth::maintenance::MaintenanceThresholds,
) -> CrushBatchSearch {
    if desired.is_zero() {
        return CrushBatchSearch::EnergyUnavailable;
    }
    let envelopes = assess_crush_envelopes(registries, state, ids);
    let powered_mass = std::cmp::min(desired, envelopes.maximum_mass());
    if powered_mass.is_zero() {
        return if envelopes.minimum_is_maintenance_only() {
            CrushBatchSearch::MaintenanceBlocked
        } else {
            CrushBatchSearch::EnergyUnavailable
        };
    }
    let desired_constraints = CrushConstraintFlags::from_envelopes(envelopes, desired);
    let reduced_for_powered_constraints = powered_mass < desired;
    let energy_limited = reduced_for_powered_constraints && desired_constraints.stored_energy;
    let equipment_capacity_limited =
        reduced_for_powered_constraints && desired_constraints.equipment_capacity;
    let condition_lifetime_limited =
        reduced_for_powered_constraints && desired_constraints.condition_lifetime;
    let safe_mass = std::cmp::min(
        powered_mass,
        envelopes.maximum_mass_preserving_condition_above(thresholds.critical_below()),
    );
    if safe_mass.is_zero() {
        return CrushBatchSearch::MaintenanceBlocked;
    }
    let options = maintenance_safe_crush_options(
        resolve_crush_options(registries, state, ids, safe_mass).into_options(),
        thresholds,
    );
    assert!(
        options.0.is_some() || options.1.is_some(),
        "powered crush condition-floor projection admitted a mass rejected by canonical resolution"
    );
    CrushBatchSearch::Available(Box::new(CrushBatchPlan {
        mass: safe_mass,
        small: options.0,
        large: options.1,
        energy_limited,
        equipment_capacity_limited,
        condition_lifetime_limited,
        maintenance_limited: safe_mass < powered_mass,
    }))
}

pub(super) fn schedule_controlled_delivery_event(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    variation: &mut ScenarioVariation,
) {
    let reference_duration =
        largest_resolvable_crush_batch(registries, state, ids, variation.ore.nominal_batch_mass)
            .and_then(|resolved| resolved.options.small.or(resolved.options.large))
            .map(|option| option.resolved.process_resolution().duration().value())
            .unwrap_or_else(|| {
                panic!("gameplay harness has no powered reference operation for delivery timing")
            });
    assert!(
        reference_duration > 0,
        "nonzero gameplay batch must take at least one tick"
    );
    let nominal_batch_count = variation
        .ore
        .order_mass
        .milligrams()
        .div_ceil(variation.ore.nominal_batch_mass.milligrams());
    let work_horizon = reference_duration
        .checked_mul(nominal_batch_count)
        .unwrap_or_else(|| panic!("gameplay harness work horizon overflowed"));
    variation.delivery.delivery_at_tick =
        1 + mix64(variation.world_seed ^ 0x57A1_1EED_71A1_1EED) % work_horizon;
}

pub(super) fn print_crush_option(
    option: &CrushOption,
    thresholds: deep_hearth::maintenance::MaintenanceThresholds,
) {
    let stored_after = option
        .stored_before
        .checked_sub(option.resolved.required_energy())
        .unwrap_or_else(|| panic!("validated crush option overdraws its energy store"));
    println!(
        "  power option {}: duration={}t bottleneck={:?} energy={}nJ reserve={}nJ->{}nJ wear={}ppm->{}ppm ({:?})",
        option.name,
        option.resolved.process_resolution().duration().value(),
        option.resolved.bottleneck(),
        option.resolved.required_energy().nanojoules(),
        option.stored_before.nanojoules(),
        stored_after.nanojoules(),
        option.resolved.condition_before().parts_per_million(),
        option.resolved.condition_after().parts_per_million(),
        thresholds.classify(option.resolved.condition_after()),
    );
}

pub(super) fn choose_crush_option(
    small: Option<CrushOption>,
    large: Option<CrushOption>,
    context: CrushChoiceContext,
) -> (CrushOption, &'static str, PowerChoiceBasis) {
    let CrushChoiceContext {
        thresholds,
        preference,
    } = context;
    match (small, large) {
        (None, None) => unreachable!("safe powered batch must contain a viable energy option"),
        (Some(option), None) | (None, Some(option)) => (
            option,
            "only viable energy source that preserves non-critical condition",
            PowerChoiceBasis::SingleSource,
        ),
        (Some(small), Some(large)) => {
            debug_assert_ne!(
                thresholds.classify(small.resolved.condition_after()),
                MaintenanceBand::Critical
            );
            debug_assert_ne!(
                thresholds.classify(large.resolved.condition_after()),
                MaintenanceBand::Critical
            );
            match preference {
                PowerPreference::PreserveReserve => (
                    small,
                    "player priority preserves scarce high-power reserve",
                    PowerChoiceBasis::Policy,
                ),
                PowerPreference::FinishSooner => {
                    if large.resolved.process_resolution().duration()
                        < small.resolved.process_resolution().duration()
                    {
                        (
                            large,
                            "player priority minimizes projected batch completion time",
                            PowerChoiceBasis::Policy,
                        )
                    } else {
                        (
                            small,
                            "both power choices finish equally soon, so preserve reserve",
                            PowerChoiceBasis::Policy,
                        )
                    }
                }
            }
        }
    }
}
