//! Settlement machine routes for material transformations already learned by hand.

use crate::core::quantity::MassSpecificEnergy;
use crate::crafting::PoweredCraftDefinition;
use crate::energy::EnergyCarrier;

use crate::content::capabilities::{
    CAPABILITY_POWERED_COPPER_HAMMERING_FLOW, CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
    CAPABILITY_POWERED_COPPER_WIRE_DRAWING_FLOW, CAPABILITY_POWERED_SAWING_FLOW,
    CAPABILITY_POWERED_STONE_GRINDING_FLOW, CAPABILITY_POWERED_WOOD_TURNING_FLOW,
};
use crate::content::processes::{
    PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_SAW_BLADE, PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_DRAW_COPPER_ELECTRICAL_WINDING, PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
    PROCESS_GRIND_STONE_SCRAP_TOOL, PROCESS_PIERCE_COPPER_SCREEN_PLATE,
    PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING, PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
    PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT, PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
    PROCESS_POWER_HAMMER_COPPER_ELECTRICAL_WINDING, PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
    PROCESS_POWER_HAMMER_COPPER_SAW_BLADE, PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_POWER_SAW_WOOD_BOARDS, PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
    PROCESS_POWER_TURN_WOOD_HANDLE, PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_TIMBER_FLYWHEEL,
    PROCESS_SHAPE_WOOD_HANDLE,
};

pub(super) fn definitions() -> [PoweredCraftDefinition; 11] {
    [
        PoweredCraftDefinition::new(
            PROCESS_POWER_SAW_WOOD_BOARDS,
            PROCESS_SAW_WOOD_BOARDS,
            CAPABILITY_POWERED_SAWING_FLOW,
            EnergyCarrier::Mechanical,
            // Primitive stored work can power sawing; larger accumulators reduce charging
            // interruptions without changing the manual transform's material yield. Powered
            // sawing still consumes blades over long campaigns, but its service cadence must
            // remain lower than hand sawing so mechanization actually returns player attention.
            MassSpecificEnergy::from_nanojoules_per_milligram(250_000),
            500,
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
            PROCESS_POWER_HAMMER_COPPER_ELECTRICAL_WINDING,
            PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING,
            CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(5_000_000),
            200,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
            PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
            CAPABILITY_POWERED_COPPER_WIRE_DRAWING_FLOW,
            EnergyCarrier::Mechanical,
            // One conductor batch fits primitive stored work, while repeated drawing remains a
            // finite-energy operation rather than free continuous production.
            MassSpecificEnergy::from_nanojoules_per_milligram(5_000_000),
            180,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
            EnergyCarrier::Mechanical,
            // Screen-plate drilling leaves machine feed rate as the useful schedule constraint
            // instead of making primitive stored-work capacity the dominant blocker.
            MassSpecificEnergy::from_nanojoules_per_milligram(2_500_000),
            250,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_TURN_WOOD_HANDLE,
            PROCESS_SHAPE_WOOD_HANDLE,
            CAPABILITY_POWERED_WOOD_TURNING_FLOW,
            EnergyCarrier::Mechanical,
            // Mechanized handle turning spends finite stored work while preserving the manual
            // transform's material yield.
            MassSpecificEnergy::from_nanojoules_per_milligram(200_000),
            400,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
            PROCESS_SHAPE_TIMBER_FLYWHEEL,
            CAPABILITY_POWERED_WOOD_TURNING_FLOW,
            EnergyCarrier::Mechanical,
            // A flywheel billet fits primitive stored work but consumes most of that buffer, making
            // delegated shaping a visible investment rather than free workshop capacity.
            MassSpecificEnergy::from_nanojoules_per_milligram(200_000),
            400,
        ),
        PoweredCraftDefinition::new(
            PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
            PROCESS_GRIND_STONE_SCRAP_TOOL,
            CAPABILITY_POWERED_STONE_GRINDING_FLOW,
            EnergyCarrier::Mechanical,
            // Service-stock grinding fits primitive stored work while keeping maintenance
            // preparation a finite delegated-work cost.
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
