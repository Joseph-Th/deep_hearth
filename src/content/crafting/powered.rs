//! Settlement machine routes for material transformations already learned by hand.

use crate::core::quantity::MassSpecificEnergy;
use crate::crafting::PoweredCraftDefinition;
use crate::energy::EnergyCarrier;

use crate::content::capabilities::{
    CAPABILITY_POWERED_COPPER_HAMMERING_FLOW, CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
    CAPABILITY_POWERED_SAWING_FLOW, CAPABILITY_POWERED_STONE_GRINDING_FLOW,
    CAPABILITY_POWERED_WOOD_TURNING_FLOW,
};
use crate::content::processes::{
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SAW_BLADE,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
    PROCESS_GRIND_STONE_SCRAP_TOOL, PROCESS_PIERCE_COPPER_SCREEN_PLATE,
    PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE, PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT,
    PROCESS_POWER_GRIND_STONE_SCRAP_TOOL, PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
    PROCESS_POWER_HAMMER_COPPER_SAW_BLADE, PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_POWER_SAW_WOOD_BOARDS, PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
    PROCESS_POWER_TURN_WOOD_HANDLE, PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_TIMBER_FLYWHEEL,
    PROCESS_SHAPE_WOOD_HANDLE,
};

pub(super) fn definitions() -> [PoweredCraftDefinition; 9] {
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
        PoweredCraftDefinition::new(
            PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
            EnergyCarrier::Mechanical,
            // One 20 g plate consumes 50 J. At the settlement spindle's 1.5 g/s throughput,
            // material feed remains the limiting schedule on the first primitive flywheel.
            MassSpecificEnergy::from_nanojoules_per_milligram(2_500_000),
            250,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_TURN_WOOD_HANDLE,
            PROCESS_SHAPE_WOOD_HANDLE,
            CAPABILITY_POWERED_WOOD_TURNING_FLOW,
            EnergyCarrier::Mechanical,
            // 200 J/kg makes one handle billet a noticeable draw on the first flywheel without
            // hiding material yield or making repetitive turning free once it is mechanized.
            MassSpecificEnergy::from_nanojoules_per_milligram(200_000),
            400,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
            PROCESS_SHAPE_TIMBER_FLYWHEEL,
            CAPABILITY_POWERED_WOOD_TURNING_FLOW,
            EnergyCarrier::Mechanical,
            // A 2.4 kg flywheel billet consumes 480 J, intentionally fitting one first-generation
            // 500 J stone flywheel charge while leaving almost no work for another operation.
            MassSpecificEnergy::from_nanojoules_per_milligram(200_000),
            400,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
            PROCESS_GRIND_STONE_SCRAP_TOOL,
            CAPABILITY_POWERED_STONE_GRINDING_FLOW,
            EnergyCarrier::Mechanical,
            // The 900 g service-stock batch costs 270 J: a visible draw that still fits the first
            // stone flywheel, turning maintenance preparation into finite delegated work.
            MassSpecificEnergy::from_nanojoules_per_milligram(300_000),
            300,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT,
            PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
            CAPABILITY_POWERED_STONE_GRINDING_FLOW,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(300_000),
            300,
        ),
    ]
}
