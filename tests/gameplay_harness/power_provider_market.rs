//! Buildable human-power provider market derived from disclosed raw roots.

use std::collections::BTreeSet;

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_DOUBLE_WOUND_TREADLE_DYNAMO,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE, EQUIPMENT_TIMBER_TREADLE_DYNAMO,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_WALKING_WHEEL, MATERIAL_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::energy::EnergyCarrier;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::labor::ManualPowerMethodId;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

use super::manual_construction_planning::manual_construction_route_from_roots;

fn bootstrap_commodity_reachable(
    registries: &Registries,
    commodity: CommodityKey,
    roots: &BTreeSet<CommodityKey>,
) -> bool {
    if roots.contains(&commodity) {
        return true;
    }
    let raw_roots = roots.iter().copied().collect::<Vec<_>>();
    manual_construction_route_from_roots(
        registries,
        commodity,
        Mass::from_milligrams(1),
        &raw_roots,
        "power-provider market reachability",
    )
    .is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PowerProviderChoice {
    pub(super) method: ManualPowerMethodId,
    pub(super) equipment: EquipmentDefinitionId,
}

impl PowerProviderChoice {
    pub(super) const fn new(method: ManualPowerMethodId, equipment: EquipmentDefinitionId) -> Self {
        Self { method, equipment }
    }

    #[allow(
        dead_code,
        reason = "lived settlement planning classifies reachable providers by embodied copper"
    )]
    pub(super) fn uses_copper(self, registries: &Registries) -> bool {
        registries
            .equipment()
            .get_equipment(self.equipment)
            .and_then(|definition| definition.assembly_profile())
            .is_some_and(|assembly| {
                assembly
                    .inputs()
                    .iter()
                    .any(|input| input.commodity().material() == MATERIAL_COPPER)
            })
    }

    #[cfg(not(test))]
    fn fallback_label(self) -> String {
        format!(
            "method-{}-equipment-{}",
            self.method.value(),
            self.equipment.value()
        )
    }

    #[cfg(not(test))]
    pub(super) fn primitive_label(self) -> String {
        for reference in PrimitivePowerChoice::ALL {
            if reference.provider() == self {
                return reference.label().to_owned();
            }
        }
        self.fallback_label()
    }

    #[cfg(not(test))]
    pub(super) fn settlement_label(self) -> String {
        for reference in SettlementPowerChoice::ALL {
            if reference.provider() == self {
                return reference.label().to_owned();
            }
        }
        self.fallback_label()
    }
}

pub(super) fn reachable_mechanical_power_providers(
    registries: &Registries,
    roots: impl IntoIterator<Item = CommodityKey>,
) -> BTreeSet<PowerProviderChoice> {
    let roots = roots.into_iter().collect::<BTreeSet<_>>();
    let mut providers = BTreeSet::new();
    for method in registries
        .labor()
        .manual_power_definitions()
        .filter(|method| method.carrier() == EnergyCarrier::Mechanical)
    {
        for equipment in registries.equipment().definitions() {
            if equipment.requires_structural_support()
                || !equipment.assembly_profile().is_some_and(|assembly| {
                    assembly.inputs().iter().all(|input| {
                        bootstrap_commodity_reachable(registries, input.commodity(), &roots)
                    })
                })
                || !matches!(
                    equipment
                        .capabilities()
                        .get_capability(method.power_capability()),
                    Some(CapabilityValue::Power(power)) if !power.is_zero()
                )
            {
                continue;
            }
            providers.insert(PowerProviderChoice::new(method.id(), equipment.id()));
        }
    }
    providers
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrimitivePowerChoice {
    Crank,
    Treadle,
    WalkingWheel,
}

impl PrimitivePowerChoice {
    #[cfg(not(test))]
    pub(super) const ALL: [Self; 3] = [Self::Crank, Self::Treadle, Self::WalkingWheel];

    pub(super) const fn equipment(self) -> EquipmentDefinitionId {
        match self {
            Self::Crank => EQUIPMENT_STONE_HAND_CRANK,
            Self::Treadle => EQUIPMENT_TIMBER_TREADLE_DRIVE,
            Self::WalkingWheel => EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        }
    }

    pub(super) const fn method(self) -> ManualPowerMethodId {
        match self {
            Self::Crank => MANUAL_POWER_HAND_CRANK,
            Self::Treadle => MANUAL_POWER_FOOT_TREADLE,
            Self::WalkingWheel => MANUAL_POWER_WALKING_WHEEL,
        }
    }

    pub(super) const fn provider(self) -> PowerProviderChoice {
        PowerProviderChoice::new(self.method(), self.equipment())
    }

    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Crank => "crank",
            Self::Treadle => "treadle",
            Self::WalkingWheel => "walking-wheel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettlementPowerChoice {
    StoneCrank,
    CopperCrank,
    Treadle,
    TreadleDynamo,
    DoubleWoundTreadleDynamo,
    WalkingWheel,
}

impl SettlementPowerChoice {
    #[cfg(not(test))]
    pub(super) const ALL: [Self; 6] = [
        Self::StoneCrank,
        Self::CopperCrank,
        Self::Treadle,
        Self::TreadleDynamo,
        Self::DoubleWoundTreadleDynamo,
        Self::WalkingWheel,
    ];

    pub(super) const fn equipment(self) -> EquipmentDefinitionId {
        match self {
            Self::StoneCrank => EQUIPMENT_STONE_HAND_CRANK,
            Self::CopperCrank => EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            Self::Treadle => EQUIPMENT_TIMBER_TREADLE_DRIVE,
            Self::TreadleDynamo => EQUIPMENT_TIMBER_TREADLE_DYNAMO,
            Self::DoubleWoundTreadleDynamo => EQUIPMENT_DOUBLE_WOUND_TREADLE_DYNAMO,
            Self::WalkingWheel => EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        }
    }

    pub(super) const fn method(self) -> ManualPowerMethodId {
        match self {
            Self::StoneCrank | Self::CopperCrank => MANUAL_POWER_HAND_CRANK,
            Self::Treadle | Self::TreadleDynamo | Self::DoubleWoundTreadleDynamo => {
                MANUAL_POWER_FOOT_TREADLE
            }
            Self::WalkingWheel => MANUAL_POWER_WALKING_WHEEL,
        }
    }

    pub(super) const fn provider(self) -> PowerProviderChoice {
        PowerProviderChoice::new(self.method(), self.equipment())
    }

    #[cfg(not(test))]
    pub(super) fn uses_copper(self, registries: &Registries) -> bool {
        self.provider().uses_copper(registries)
    }

    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::StoneCrank => "stone-crank",
            Self::CopperCrank => "copper-crank",
            Self::Treadle => "treadle",
            Self::TreadleDynamo => "treadle-dynamo",
            Self::DoubleWoundTreadleDynamo => "double-wound-treadle-dynamo",
            Self::WalkingWheel => "walking-wheel",
        }
    }
}
