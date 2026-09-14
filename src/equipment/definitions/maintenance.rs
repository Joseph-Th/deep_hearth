//! Authored material-backed equipment maintenance profiles.

use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EquipmentMaintenanceMaterialMode {
    AggregateWearStock,
    EmbodiedComponentReplacement,
}

/// Authored material-backed service for one equipment class.
///
/// Aggregate service consumes a proportional amount of external wear stock and reforms that exact
/// matter into a spent form. Component service instead exchanges one complete authored embodied
/// component for a fresh equivalent while the removed component becomes spent stock. Both routes
/// keep condition recovery behind explicit matter movement rather than a free durability reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceProfile {
    material_mode: EquipmentMaintenanceMaterialMode,
    replacement: CommodityKey,
    full_service_replacement_mass: Mass,
    spent: CommodityKey,
    restored_condition: Condition,
    full_service_duration: TickSpan,
    exertion: SurvivalExertion,
}

impl EquipmentMaintenanceProfile {
    #[must_use]
    pub fn new(
        replacement: CommodityKey,
        full_service_replacement_mass: Mass,
        spent: CommodityKey,
        restored_condition: Condition,
        full_service_duration: TickSpan,
        exertion: SurvivalExertion,
    ) -> Self {
        assert!(
            !full_service_replacement_mass.is_zero(),
            "equipment maintenance full-service replacement mass must be nonzero"
        );
        assert!(
            restored_condition > Condition::FAILED,
            "equipment maintenance restored condition must be above failed"
        );
        assert!(
            full_service_duration != TickSpan::ZERO,
            "equipment maintenance full-service duration must be nonzero"
        );
        exertion.assert_active_player_work();
        assert_eq!(
            replacement.material(),
            spent.material(),
            "equipment maintenance may change form but cannot change material identity"
        );
        assert_ne!(
            replacement, spent,
            "equipment maintenance spent output must differ from reusable replacement stock"
        );
        Self {
            material_mode: EquipmentMaintenanceMaterialMode::AggregateWearStock,
            replacement,
            full_service_replacement_mass,
            spent,
            restored_condition,
            full_service_duration,
            exertion,
        }
    }

    /// Authors replacement of one complete embodied assembly component.
    ///
    /// component_mass must match the corresponding assembly input exactly; registry validation
    /// enforces that relationship once the complete equipment definition is available.
    #[must_use]
    pub fn new_component_replacement(
        replacement: CommodityKey,
        component_mass: Mass,
        spent: CommodityKey,
        restored_condition: Condition,
        full_service_duration: TickSpan,
        exertion: SurvivalExertion,
    ) -> Self {
        assert_eq!(
            restored_condition,
            Condition::PRISTINE,
            "complete embodied-component replacement must restore pristine condition because residual component wear is not separately represented"
        );
        let mut profile = Self::new(
            replacement,
            component_mass,
            spent,
            restored_condition,
            full_service_duration,
            exertion,
        );
        profile.material_mode = EquipmentMaintenanceMaterialMode::EmbodiedComponentReplacement;
        profile
    }

    #[must_use]
    pub const fn replacement(self) -> CommodityKey {
        self.replacement
    }

    #[must_use]
    pub const fn full_service_replacement_mass(self) -> Mass {
        self.full_service_replacement_mass
    }

    /// Replacement stock required to restore condition_before to the authored service target.
    ///
    /// Aggregate wear stock scales with the fraction of target condition restored, rounded upward
    /// so positive repair can never become free. Embodied-component replacement always requires the
    /// complete authored component because a partial swap would fabricate an unmodeled component
    /// condition state.
    #[must_use]
    pub fn required_replacement_mass(self, condition_before: Condition) -> Mass {
        if condition_before >= self.restored_condition {
            return Mass::ZERO;
        }
        if self.is_component_replacement() {
            return self.full_service_replacement_mass;
        }
        let restored_parts = self
            .restored_condition
            .parts_per_million()
            .checked_sub(condition_before.parts_per_million())
            .unwrap_or_else(|| unreachable!("lower condition must leave a positive repair delta"));
        let numerator = u128::from(self.full_service_replacement_mass.milligrams())
            * u128::from(restored_parts);
        let denominator = u128::from(self.restored_condition.parts_per_million());
        let required = numerator.div_ceil(denominator);
        let required = u64::try_from(required).unwrap_or_else(|_| {
            unreachable!("partial maintenance mass cannot exceed authored full-service mass")
        });
        Mass::from_milligrams(required)
    }

    #[must_use]
    pub const fn spent(self) -> CommodityKey {
        self.spent
    }

    #[must_use]
    pub const fn restored_condition(self) -> Condition {
        self.restored_condition
    }

    #[must_use]
    pub const fn full_service_duration(self) -> TickSpan {
        self.full_service_duration
    }

    /// Active player-work duration required to restore condition_before to the service target.
    ///
    /// Aggregate service scales duration with the same restored-condition fraction as replacement
    /// stock. Component replacement remains indivisible because the whole authored component is
    /// exchanged as one service operation.
    #[must_use]
    pub fn required_service_duration(self, condition_before: Condition) -> TickSpan {
        if condition_before >= self.restored_condition {
            return TickSpan::ZERO;
        }
        if self.is_component_replacement() {
            return self.full_service_duration;
        }
        let restored_parts = self
            .restored_condition
            .parts_per_million()
            .checked_sub(condition_before.parts_per_million())
            .unwrap_or_else(|| unreachable!("lower condition must leave a positive repair delta"));
        let numerator = u128::from(self.full_service_duration.value()) * u128::from(restored_parts);
        let denominator = u128::from(self.restored_condition.parts_per_million());
        let required = numerator.div_ceil(denominator);
        let required = u64::try_from(required).unwrap_or_else(|_| {
            unreachable!(
                "partial maintenance duration cannot exceed authored full-service duration"
            )
        });
        TickSpan::new(required.max(1))
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub const fn is_component_replacement(self) -> bool {
        matches!(
            self.material_mode,
            EquipmentMaintenanceMaterialMode::EmbodiedComponentReplacement
        )
    }
}
