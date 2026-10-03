//! Resolves preservation-infrastructure construction through ordinary manual-production routes.

use std::collections::BTreeMap;

use deep_hearth::content::{
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::registry::Registries;

use super::manual_construction_planning::{
    ManualConstructionRoute, manual_construction_route_from_roots,
};

#[derive(Clone, Debug)]
pub(super) struct PreservationConstructionPlan {
    pub(super) routes: Vec<ManualConstructionRoute>,
    pub(super) raw_mass: Mass,
    pub(super) attention_ticks: u64,
}

impl PreservationConstructionPlan {
    pub(super) fn raw_requirements(&self) -> BTreeMap<CommodityKey, Mass> {
        let mut requirements = BTreeMap::new();
        for route in &self.routes {
            let total = requirements
                .get(&route.raw_commodity)
                .copied()
                .unwrap_or(Mass::ZERO)
                .checked_add(route.raw_mass)
                .unwrap_or_else(|| panic!("preservation raw requirement overflowed"));
            requirements.insert(route.raw_commodity, total);
        }
        requirements
    }
}

#[cfg(test)]
pub(super) fn is_disclosed_preservation_raw_material(commodity: CommodityKey) -> bool {
    commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
        || commodity == CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
        || commodity == CommodityKey::new(MATERIAL_CLAY, FORM_LUMP)
        // Native copper is a legitimate post-processing settlement input. Survival scenarios may
        // disclose a finite owned quantity exactly as they disclose logs or stone; the route still
        // has to perform canonical cold-working before copper can enter an enclosure assembly.
        || commodity == CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)
}

pub(super) fn preservation_construction_plan(
    registries: &Registries,
    profile: &MaterialAssemblyProfile,
) -> PreservationConstructionPlan {
    let raw_roots = [
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ];
    let mut routes = Vec::with_capacity(profile.inputs().len());
    let mut raw_mass = Mass::ZERO;
    let mut attention_ticks = 0_u64;
    for input in profile.inputs() {
        let route = manual_construction_route_from_roots(
            registries,
            input.commodity(),
            input.mass(),
            &raw_roots,
            "preservation construction",
        )
        .unwrap_or_else(|| {
            panic!(
                "constructible preservation storage has no acyclic manual production route to assembly commodity {}",
                input.commodity().value()
            )
        });
        assert!(
            !route.steps.is_empty(),
            "preservation enclosure assembly commodity {} has no manual fabrication step; do not bootstrap a finished enclosure component as ordinary-play evidence",
            input.commodity().value()
        );
        raw_mass = raw_mass
            .checked_add(route.raw_mass)
            .unwrap_or_else(|| panic!("preservation construction raw-material mass overflowed"));
        attention_ticks = attention_ticks
            .checked_add(route.attention_ticks)
            .unwrap_or_else(|| panic!("preservation construction attention overflowed"));
        routes.push(route);
    }
    PreservationConstructionPlan {
        routes,
        raw_mass,
        attention_ticks,
    }
}
