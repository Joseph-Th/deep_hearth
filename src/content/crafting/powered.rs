//! Settlement machine routes for material transformations already learned by hand.

use crate::core::quantity::MassSpecificEnergy;
use crate::crafting::PoweredCraftDefinition;
use crate::energy::EnergyCarrier;

use crate::content::capabilities::{
    CAPABILITY_POWERED_COPPER_HAMMERING_FLOW, CAPABILITY_POWERED_SAWING_FLOW,
};
use crate::content::processes::{
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SAW_BLADE,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
    PROCESS_POWER_HAMMER_COPPER_SAW_BLADE, PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_POWER_SAW_WOOD_BOARDS, PROCESS_SAW_WOOD_BOARDS,
};

pub(super) fn definitions() -> [PoweredCraftDefinition; 4] {
    [
        PoweredCraftDefinition::new(
            PROCESS_POWER_SAW_WOOD_BOARDS,
            PROCESS_SAW_WOOD_BOARDS,
            CAPABILITY_POWERED_SAWING_FLOW,
            EnergyCarrier::Mechanical,
            // 250 J/kg: low enough that even the first 500 J flywheel can run one log, while
            // larger accumulators buy fewer charging interruptions rather than a hidden yield buff.
            MassSpecificEnergy::from_nanojoules_per_milligram(250_000),
            800,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(5_000_000),
            200,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT,
            PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
            CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(5_000_000),
            200,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_HAMMER_COPPER_SAW_BLADE,
            PROCESS_COLD_WORK_COPPER_SAW_BLADE,
            CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(5_000_000),
            200,
        ),
    ]
}
