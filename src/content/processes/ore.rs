//! Ore dressing, comminution, sizing, and concentration process definitions.

use crate::production::ProcessDefinition;

use super::super::capabilities::{
    CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW, CAPABILITY_GRINDER_BATCH,
    CAPABILITY_GRINDER_FLOW, CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW,
    CAPABILITY_SEPARATOR_BATCH, CAPABILITY_SEPARATOR_FLOW,
};

use super::{
    PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_HAND_BREAK_ORE,
    PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_REGRIND_COPPER_TAILINGS,
    PROCESS_SCAVENGE_COPPER_TAILINGS, PROCESS_SCREEN_CRUSHED_ORE, PROCESS_SEPARATE_NATIVE_COPPER,
    mass_flow_resolver_requirements,
};

pub(super) fn definitions() -> [ProcessDefinition; 11] {
    [
        ProcessDefinition::new(
            PROCESS_CRUSH_ORE,
            "crush ore",
            mass_flow_resolver_requirements(CAPABILITY_CRUSHER_FLOW, CAPABILITY_CRUSHER_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            "screen crushed ore",
            mass_flow_resolver_requirements(CAPABILITY_SCREEN_FLOW, CAPABILITY_SCREEN_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_GRIND_CRUSHED_ORE,
            "grind crushed ore",
            mass_flow_resolver_requirements(CAPABILITY_GRINDER_FLOW, CAPABILITY_GRINDER_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            "fine grind screen oversize",
            mass_flow_resolver_requirements(CAPABILITY_GRINDER_FLOW, CAPABILITY_GRINDER_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_HAND_SORT_NATIVE_COPPER,
            "hand sort native copper from crushed ore",
            Vec::new(),
        ),
        ProcessDefinition::new(PROCESS_HAND_BREAK_ORE, "hand break ore", Vec::new()),
        ProcessDefinition::new(
            PROCESS_SEPARATE_NATIVE_COPPER,
            "separate native copper from crushed ore",
            mass_flow_resolver_requirements(CAPABILITY_SEPARATOR_FLOW, CAPABILITY_SEPARATOR_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_CONCENTRATE_COPPER,
            "concentrate copper from liberated ore",
            mass_flow_resolver_requirements(CAPABILITY_SEPARATOR_FLOW, CAPABILITY_SEPARATOR_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE,
            "clean native copper from rich concentrate",
            mass_flow_resolver_requirements(CAPABILITY_SEPARATOR_FLOW, CAPABILITY_SEPARATOR_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_REGRIND_COPPER_TAILINGS,
            "regrind copper-bearing tailings",
            mass_flow_resolver_requirements(CAPABILITY_GRINDER_FLOW, CAPABILITY_GRINDER_BATCH),
        ),
        ProcessDefinition::new(
            PROCESS_SCAVENGE_COPPER_TAILINGS,
            "scavenge copper from reground tailings",
            mass_flow_resolver_requirements(CAPABILITY_SEPARATOR_FLOW, CAPABILITY_SEPARATOR_BATCH),
        ),
    ]
}
