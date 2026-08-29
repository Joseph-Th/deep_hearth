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
    pub(super) condition_capacity_limited: bool,
    pub(super) condition_lifetime_limited: bool,
    pub(super) maintenance_limited: bool,
}

pub(super) enum CrushBatchSearch {
    Available(Box<CrushBatchPlan>),
    EnergyUnavailable,
    MaintenanceBlocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrushConstraint {
    StoredEnergy,
    ConditionCapacity,
    ConditionLifetime,
}

struct CrushOptions {
    small: Option<CrushOption>,
    large: Option<CrushOption>,
    small_constraint: Option<CrushConstraint>,
    large_constraint: Option<CrushConstraint>,
}

impl CrushOptions {
    fn has_viable_option(&self) -> bool {
        self.small.is_some() || self.large.is_some()
    }

    fn constrained_by(&self, constraint: CrushConstraint) -> bool {
        self.small_constraint == Some(constraint) || self.large_constraint == Some(constraint)
    }

    fn maintenance_only_failure(&self) -> bool {
        !self.has_viable_option()
            && [self.small_constraint, self.large_constraint]
                .into_iter()
                .all(|constraint| {
                    matches!(
                        constraint,
                        Some(
                            CrushConstraint::ConditionCapacity | CrushConstraint::ConditionLifetime
                        )
                    )
                })
    }

    fn into_options(self) -> (Option<CrushOption>, Option<CrushOption>) {
        (self.small, self.large)
    }
}

#[derive(Clone, Copy, Default)]
struct CrushConstraintFlags {
    stored_energy: bool,
    condition_capacity: bool,
    condition_lifetime: bool,
}

impl CrushConstraintFlags {
    fn from_options(options: &CrushOptions) -> Self {
        Self {
            stored_energy: options.constrained_by(CrushConstraint::StoredEnergy),
            condition_capacity: options.constrained_by(CrushConstraint::ConditionCapacity),
            condition_lifetime: options.constrained_by(CrushConstraint::ConditionLifetime),
        }
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
) -> Result<CrushOption, CrushConstraint> {
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
        })) => Err(CrushConstraint::StoredEnergy),
        Err(ComminutionResolutionError::BatchMassExceeded { .. }) => {
            Err(CrushConstraint::ConditionCapacity)
        }
        Err(ComminutionResolutionError::ConditionDuration(_)) => {
            Err(CrushConstraint::ConditionLifetime)
        }
        Err(error) => panic!("gameplay harness {name} drive resolution failed: {error}"),
    }
}

fn resolve_crush_options(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
) -> CrushOptions {
    let (small, small_constraint) =
        match resolve_crush_option(registries, state, ids, mass, "small", ids.small_drive) {
            Ok(option) => (Some(option), None),
            Err(constraint) => (None, Some(constraint)),
        };
    let (large, large_constraint) =
        match resolve_crush_option(registries, state, ids, mass, "large", ids.large_drive) {
            Ok(option) => (Some(option), None),
            Err(constraint) => (None, Some(constraint)),
        };
    CrushOptions {
        small,
        large,
        small_constraint,
        large_constraint,
    }
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
    let desired_options = resolve_crush_options(registries, state, ids, desired);
    let desired_constraints = CrushConstraintFlags::from_options(&desired_options);
    if desired_options.has_viable_option() {
        return Some(ResolvableCrushBatch {
            mass: desired,
            options: desired_options,
            desired_constraints,
        });
    }

    let mut low = 1_u64;
    let mut high = desired.milligrams().saturating_sub(1);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let mass = Mass::from_milligrams(midpoint);
        let options = resolve_crush_options(registries, state, ids, mass);
        if options.has_viable_option() {
            best = Some((mass, options));
            low = midpoint + 1;
        } else {
            high = midpoint.saturating_sub(1);
        }
    }
    best.map(|(mass, options)| ResolvableCrushBatch {
        mass,
        options,
        desired_constraints,
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
    let resolved = match largest_resolvable_crush_batch(registries, state, ids, desired) {
        Some(resolved) => resolved,
        None => {
            let minimum = resolve_crush_options(registries, state, ids, Mass::from_milligrams(1));
            return if minimum.maintenance_only_failure() {
                CrushBatchSearch::MaintenanceBlocked
            } else {
                CrushBatchSearch::EnergyUnavailable
            };
        }
    };
    let powered_mass = resolved.mass;
    let reduced_for_powered_constraints = powered_mass < desired;
    let energy_limited =
        reduced_for_powered_constraints && resolved.desired_constraints.stored_energy;
    let condition_capacity_limited =
        reduced_for_powered_constraints && resolved.desired_constraints.condition_capacity;
    let condition_lifetime_limited =
        reduced_for_powered_constraints && resolved.desired_constraints.condition_lifetime;
    let safe_at_powered =
        maintenance_safe_crush_options(resolved.options.into_options(), thresholds);
    if safe_at_powered.0.is_some() || safe_at_powered.1.is_some() {
        return CrushBatchSearch::Available(Box::new(CrushBatchPlan {
            mass: powered_mass,
            small: safe_at_powered.0,
            large: safe_at_powered.1,
            energy_limited,
            condition_capacity_limited,
            condition_lifetime_limited,
            maintenance_limited: false,
        }));
    }

    let mut low = 1_u64;
    let mut high = powered_mass.milligrams().saturating_sub(1);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let mass = Mass::from_milligrams(midpoint);
        let options = maintenance_safe_crush_options(
            resolve_crush_options(registries, state, ids, mass).into_options(),
            thresholds,
        );
        if options.0.is_some() || options.1.is_some() {
            best = Some((mass, options.0, options.1));
            low = midpoint + 1;
        } else {
            high = midpoint.saturating_sub(1);
        }
    }
    match best {
        Some((mass, small, large)) => CrushBatchSearch::Available(Box::new(CrushBatchPlan {
            mass,
            small,
            large,
            energy_limited,
            condition_capacity_limited,
            condition_lifetime_limited,
            maintenance_limited: true,
        })),
        None => CrushBatchSearch::MaintenanceBlocked,
    }
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
