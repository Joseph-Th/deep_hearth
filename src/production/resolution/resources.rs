//! Private energy and equipment binding state for resolved production operations.

use crate::energy::{ValidatedEnergySink, ValidatedEnergySupply};
use crate::equipment::ValidatedEquipmentUse;
use crate::maintenance::Condition;

use super::ProcessResolutionError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProcessEquipmentResolution {
    equipment_use: ValidatedEquipmentUse,
    condition_after: Condition,
}

impl ProcessEquipmentResolution {
    const fn new(equipment_use: ValidatedEquipmentUse, condition_after: Condition) -> Self {
        Self {
            equipment_use,
            condition_after,
        }
    }

    fn validate(self) -> Result<Self, ProcessResolutionError> {
        let before = self.equipment_use.trace().condition();
        if self.condition_after > before {
            return Err(ProcessResolutionError::EquipmentConditionImproved {
                before,
                after: self.condition_after,
            });
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessResourceResolution {
    None,
    Equipment {
        equipment: ProcessEquipmentResolution,
    },
    SupplyAndEquipment {
        energy_supply: ValidatedEnergySupply,
        equipment: ProcessEquipmentResolution,
    },
    SinkAndEquipment {
        energy_sink: ValidatedEnergySink,
        equipment: ProcessEquipmentResolution,
    },
}

pub(super) struct ResolvedProcessResources {
    pub(super) energy_supply: Option<ValidatedEnergySupply>,
    pub(super) energy_sink: Option<ValidatedEnergySink>,
    pub(super) equipment_use: Option<ValidatedEquipmentUse>,
    pub(super) equipment_condition_after: Option<Condition>,
}

impl ProcessResourceResolution {
    pub(super) const fn none() -> Self {
        Self::None
    }

    pub(super) const fn with_equipment(
        equipment_use: ValidatedEquipmentUse,
        condition_after: Condition,
    ) -> Self {
        Self::Equipment {
            equipment: ProcessEquipmentResolution::new(equipment_use, condition_after),
        }
    }

    pub(super) const fn with_supply_and_equipment(
        energy_supply: ValidatedEnergySupply,
        equipment_use: ValidatedEquipmentUse,
        condition_after: Condition,
    ) -> Self {
        Self::SupplyAndEquipment {
            energy_supply,
            equipment: ProcessEquipmentResolution::new(equipment_use, condition_after),
        }
    }

    pub(super) const fn with_sink_and_equipment(
        energy_sink: ValidatedEnergySink,
        equipment_use: ValidatedEquipmentUse,
        condition_after: Condition,
    ) -> Self {
        Self::SinkAndEquipment {
            energy_sink,
            equipment: ProcessEquipmentResolution::new(equipment_use, condition_after),
        }
    }

    pub(super) fn resolve(self) -> Result<ResolvedProcessResources, ProcessResolutionError> {
        match self {
            Self::None => Ok(ResolvedProcessResources {
                energy_supply: None,
                energy_sink: None,
                equipment_use: None,
                equipment_condition_after: None,
            }),
            Self::Equipment { equipment } => {
                let equipment = equipment.validate()?;
                Ok(ResolvedProcessResources {
                    energy_supply: None,
                    energy_sink: None,
                    equipment_use: Some(equipment.equipment_use),
                    equipment_condition_after: Some(equipment.condition_after),
                })
            }
            Self::SupplyAndEquipment {
                energy_supply,
                equipment,
            } => {
                let equipment = equipment.validate()?;
                Ok(ResolvedProcessResources {
                    energy_supply: Some(energy_supply),
                    energy_sink: None,
                    equipment_use: Some(equipment.equipment_use),
                    equipment_condition_after: Some(equipment.condition_after),
                })
            }
            Self::SinkAndEquipment {
                energy_sink,
                equipment,
            } => {
                let equipment = equipment.validate()?;
                Ok(ResolvedProcessResources {
                    energy_supply: None,
                    energy_sink: Some(energy_sink),
                    equipment_use: Some(equipment.equipment_use),
                    equipment_condition_after: Some(equipment.condition_after),
                })
            }
        }
    }
}
