//! Crafting, ore-processing, and thermal handbook relationships.

use crate::material::CommodityKey;
use crate::registry::Registries;

use super::{CommoditySource, CommodityUse};

pub(super) fn collect_crafting_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.crafting().definitions() {
        if definition.input() == commodity {
            uses.push(CommodityUse::ManualCraft {
                process: definition.process(),
                required_mass: definition.input_mass(),
            });
        }
        for output in definition.outputs() {
            if output.commodity() == commodity {
                sources.push(CommoditySource::ManualCraft {
                    process: definition.process(),
                    output_mass: output.mass(),
                });
            }
        }
    }
}

pub(super) fn collect_ore_processing_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.ore_processing().manual_comminution_definitions() {
        if commodity.form() == definition.input_form() {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.form() == definition.output_form() {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
    for definition in registries.ore_processing().comminution_definitions() {
        if commodity.form() == definition.input_form() {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.form() == definition.output_form() {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
    for definition in registries.ore_processing().screening_definitions() {
        if commodity.form() == definition.input_form() {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.form() == definition.output_form() {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
}

pub(super) fn collect_separation_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries
        .ore_processing()
        .manual_constituent_separation_definitions()
    {
        if commodity.material() == definition.target_material()
            && commodity.form() == definition.input_form()
        {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.material() == definition.target_material()
            && commodity.form() == definition.target_output_form()
        {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
    for definition in registries
        .ore_processing()
        .constituent_separation_definitions()
    {
        if commodity.form() == definition.input_form()
            && (!definition.requires_target_host()
                || commodity.material() == definition.target_material())
        {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.material() == definition.target_material()
            && commodity.form() == definition.target_output_form()
        {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
}

pub(super) fn collect_thermal_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.thermal().melting_definitions() {
        if commodity.material() == definition.material() {
            if definition.solid_forms().contains(&commodity.form()) {
                uses.push(CommodityUse::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
            if commodity.form() == definition.liquid_form() {
                sources.push(CommoditySource::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
        }
    }
    for definition in registries.thermal().casting_definitions() {
        if commodity.material() == definition.material() {
            if commodity.form() == definition.liquid_form() {
                uses.push(CommodityUse::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
            if commodity.form() == definition.solid_form() {
                sources.push(CommoditySource::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
        }
    }
}
