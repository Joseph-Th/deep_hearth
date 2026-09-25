//! Checked long-horizon manual-power workload projection for provider comparisons.

use deep_hearth::core::quantity::Energy;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::labor::{ManualPowerMethodId, ManualPowerProjection, project_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::ore_processing::PoweredOreOrderBatch;
use deep_hearth::registry::Registries;

use super::ShapedBuild;

#[derive(Clone, Copy)]
pub(super) struct ManualPowerRoute {
    method: ManualPowerMethodId,
    equipment: EquipmentDefinitionId,
    store: EnergyStoreDefinitionId,
    requested: Energy,
    context: &'static str,
}

#[derive(Clone, Copy)]
pub(super) struct ManualPowerLifecycleCost {
    pub(super) attention_ticks: u64,
    pub(super) metabolic_nj: u128,
    pub(super) hydration_ul: u64,
    pub(super) condition_after: Condition,
}

#[derive(Clone, Copy)]
struct LifecycleAccumulator {
    attention_ticks: u64,
    metabolic_nj: u128,
    hydration_ul: u64,
    condition: Condition,
}

impl LifecycleAccumulator {
    const fn empty() -> Self {
        Self {
            attention_ticks: 0,
            metabolic_nj: 0,
            hydration_ul: 0,
            condition: Condition::PRISTINE,
        }
    }

    const fn with_build(build: ShapedBuild) -> Self {
        Self {
            attention_ticks: build.attention_ticks,
            metabolic_nj: build.metabolic_nj,
            hydration_ul: build.hydration_ul,
            condition: Condition::PRISTINE,
        }
    }

    fn record_charge(&mut self, charge: ManualPowerProjection, context: &'static str) {
        self.attention_ticks = self
            .attention_ticks
            .checked_add(charge.duration().value())
            .unwrap_or_else(|| panic!("power-provider {context} lifecycle attention overflowed"));
        self.metabolic_nj = self
            .metabolic_nj
            .checked_add(charge.resource_budget().metabolic_energy().nanojoules())
            .unwrap_or_else(|| panic!("power-provider {context} lifecycle metabolism overflowed"));
        self.hydration_ul = self
            .hydration_ul
            .checked_add(charge.resource_budget().hydration().microliters())
            .unwrap_or_else(|| panic!("power-provider {context} lifecycle hydration overflowed"));
        self.condition = charge.condition_after();
    }

    const fn policy_key(self, input_mass_mg: u64, tie_break: u8) -> (u64, u128, u64, u64, u8) {
        (
            self.attention_ticks,
            self.metabolic_nj,
            self.hydration_ul,
            input_mass_mg,
            tie_break,
        )
    }

    const fn finish(self) -> ManualPowerLifecycleCost {
        ManualPowerLifecycleCost {
            attention_ticks: self.attention_ticks,
            metabolic_nj: self.metabolic_nj,
            hydration_ul: self.hydration_ul,
            condition_after: self.condition,
        }
    }
}

impl ManualPowerRoute {
    pub(super) const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentDefinitionId,
        store: EnergyStoreDefinitionId,
        requested: Energy,
        context: &'static str,
    ) -> Self {
        Self {
            method,
            equipment,
            store,
            requested,
            context,
        }
    }

    fn project_requested(
        self,
        registries: &Registries,
        condition: Condition,
        requested: Energy,
    ) -> ManualPowerProjection {
        assert!(
            !requested.is_zero() && requested <= self.requested,
            "power-provider {} partial charge must stay within one full buffer request",
            self.context
        );
        project_manual_power(
            registries,
            self.method,
            self.equipment,
            condition,
            self.store,
            requested,
        )
        .unwrap_or_else(|error| {
            panic!(
                "power-provider {} charge projection failed: {error}",
                self.context
            )
        })
    }

    pub(super) fn project(
        self,
        registries: &Registries,
        condition: Condition,
    ) -> ManualPowerProjection {
        self.project_requested(registries, condition, self.requested)
    }

    pub(super) fn project_lifecycle(
        self,
        registries: &Registries,
        declared_work: Energy,
    ) -> ManualPowerLifecycleCost {
        assert!(
            !declared_work.is_zero(),
            "power-provider {} lifecycle requires positive declared work",
            self.context
        );
        let mut remaining_nj = declared_work.nanojoules();
        let full_request_nj = self.requested.nanojoules();
        let mut lifecycle = LifecycleAccumulator::empty();

        // Charge only useful work. The final event may be a partial buffer charge instead of
        // fictitious excess work introduced by ceiling division.
        while remaining_nj > 0 {
            let requested_nj = remaining_nj.min(full_request_nj);
            let charge = self.project_requested(
                registries,
                lifecycle.condition,
                Energy::from_nanojoules(requested_nj),
            );
            lifecycle.record_charge(charge, self.context);
            remaining_nj = remaining_nj
                .checked_sub(requested_nj)
                .unwrap_or_else(|| unreachable!("projected charge is bounded by remaining work"));
        }

        lifecycle.finish()
    }

    pub(super) fn project_lifecycle_batches(
        self,
        registries: &Registries,
        batches: &[PoweredOreOrderBatch],
    ) -> ManualPowerLifecycleCost {
        assert!(
            !batches.is_empty(),
            "power-provider {} lifecycle requires at least one consumer batch",
            self.context
        );
        let mut lifecycle = LifecycleAccumulator::empty();
        for batch in batches {
            let charge =
                self.project_requested(registries, lifecycle.condition, batch.required_energy());
            lifecycle.record_charge(charge, self.context);
        }
        lifecycle.finish()
    }
}

pub(super) fn first_candidate_preferred_charge(
    registries: &Registries,
    baseline_route: ManualPowerRoute,
    baseline_build: ShapedBuild,
    candidate_route: ManualPowerRoute,
    candidate_build: ShapedBuild,
    maximum_charges: u64,
    minimum_attention_return_ticks: u64,
) -> Option<u64> {
    let mut baseline = LifecycleAccumulator::with_build(baseline_build);
    let mut candidate = LifecycleAccumulator::with_build(candidate_build);

    for charges in 1..=maximum_charges {
        let baseline_charge = baseline_route.project(registries, baseline.condition);
        baseline.record_charge(baseline_charge, "baseline crossover");
        let candidate_charge = candidate_route.project(registries, candidate.condition);
        candidate.record_charge(candidate_charge, "candidate crossover");

        let attention_saving = baseline
            .attention_ticks
            .saturating_sub(candidate.attention_ticks);
        if minimum_attention_return_ticks > 0 {
            if attention_saving >= minimum_attention_return_ticks {
                return Some(charges);
            }
        } else if candidate.policy_key(candidate_build.input_mass_mg, 1)
            < baseline.policy_key(baseline_build.input_mass_mg, 0)
        {
            return Some(charges);
        }
    }
    None
}

pub(super) fn charge_events_for_declared_work(
    declared_work_nj: u128,
    capacity_nj: u128,
    context: &'static str,
) -> u64 {
    assert!(
        declared_work_nj > 0 && capacity_nj > 0,
        "power-provider {context} requires positive project work and buffer capacity"
    );
    let charges = declared_work_nj.div_ceil(capacity_nj);
    u64::try_from(charges)
        .unwrap_or_else(|_| panic!("power-provider {context} charge horizon exceeds u64"))
}
