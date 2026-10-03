//! Checked long-horizon manual-power workload projection for provider comparisons.

use deep_hearth::core::quantity::Energy;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::labor::{ManualPowerMethodId, ManualPowerProjection, project_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

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

    /// Projects cumulative full-buffer lifecycle costs once per charge count.
    ///
    /// Market-frontier discovery asks the same route about many adjacent horizons. Replaying the
    /// whole lifecycle from pristine condition for every horizon would turn an O(n) gameplay
    /// question into O(n²) harness work. Carry the canonical projected condition forward once and
    /// retain each cumulative result instead.
    pub(super) fn project_full_charge_series(
        self,
        registries: &Registries,
        maximum_charges: u64,
    ) -> Vec<ManualPowerLifecycleCost> {
        assert!(
            maximum_charges > 0,
            "power-provider {} frontier requires at least one charge",
            self.context
        );
        let mut lifecycle = LifecycleAccumulator::empty();
        let mut series = Vec::with_capacity(
            usize::try_from(maximum_charges)
                .unwrap_or_else(|_| panic!("power-provider frontier exceeds usize")),
        );
        for _ in 0..maximum_charges {
            let charge = self.project(registries, lifecycle.condition);
            lifecycle.record_charge(charge, self.context);
            series.push(lifecycle.finish());
        }
        series
    }
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
