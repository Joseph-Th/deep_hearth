//! Resolves preservation-infrastructure construction through ordinary manual-production routes.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use deep_hearth::content::{FORM_LOG, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD};
use deep_hearth::core::quantity::Mass;
use deep_hearth::crafting::project_manual_craft_hand_work;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::production::ProcessId;
use deep_hearth::registry::{ProcessEquipmentRole, Registries};

#[derive(Clone, Copy, Debug)]
pub(super) struct ManualConstructionStep {
    pub(super) process: ProcessId,
    pub(super) batches: u64,
    pub(super) duration_ticks: u64,
    pub(super) input: CommodityKey,
    pub(super) input_mass: Mass,
    pub(super) output: CommodityKey,
}

#[derive(Clone, Debug)]
pub(super) struct ManualConstructionRoute {
    pub(super) raw_commodity: CommodityKey,
    pub(super) raw_mass: Mass,
    pub(super) steps: Vec<ManualConstructionStep>,
    pub(super) attention_ticks: u64,
    pub(super) metabolic_energy_nj: u128,
    pub(super) hydration_ul: u64,
}

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

pub(super) fn is_disclosed_preservation_raw_material(commodity: CommodityKey) -> bool {
    commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
        || commodity == CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
}

fn discover_manual_construction_route(
    registries: &Registries,
    output: CommodityKey,
    required_mass: Mass,
    visiting: &mut BTreeSet<CommodityKey>,
) -> Option<ManualConstructionRoute> {
    if !visiting.insert(output) {
        return None;
    }
    let authored_producers = registries
        .crafting()
        .manual_producers(output)
        .collect::<Vec<_>>();
    if authored_producers.is_empty() {
        assert!(visiting.remove(&output));
        if !is_disclosed_preservation_raw_material(output) {
            return None;
        }
        return Some(ManualConstructionRoute {
            raw_commodity: output,
            raw_mass: required_mass,
            steps: Vec::new(),
            attention_ticks: 0,
            metabolic_energy_nj: 0,
            hydration_ul: 0,
        });
    }
    // This planner proves the ordinary raw-material bootstrap for preservation infrastructure. It
    // may use recipes whose equipment profile is optional, but it must not assume that another
    // durable station already exists. Required-equipment recipes remain valid authored alternatives
    // for actors that own their provider; they are not bootstrap edges.
    let producers = authored_producers
        .into_iter()
        .filter(|producer| {
            registries
                .process_topology(producer.process())
                .unwrap_or_else(|| panic!("manual preservation producer lost process topology"))
                .equipment_role()
                != ProcessEquipmentRole::Required
        })
        .collect::<Vec<_>>();
    if producers.is_empty() {
        assert!(visiting.remove(&output));
        return None;
    }

    let mut candidates = Vec::new();
    for producer in producers {
        let output_per_batch = producer
            .outputs()
            .iter()
            .find(|candidate| candidate.commodity() == output)
            .map(|candidate| candidate.mass())
            .unwrap_or_else(|| unreachable!("selected producer must contain requested output"));
        assert!(
            !output_per_batch.is_zero(),
            "manual construction producer {} has zero useful output",
            producer.process().value()
        );
        let batches = required_mass
            .milligrams()
            .div_ceil(output_per_batch.milligrams());
        let batches_nonzero = NonZeroU64::new(batches)
            .unwrap_or_else(|| unreachable!("nonzero construction demand yields nonzero batches"));
        let input_mass = Mass::from_milligrams(
            producer
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("preservation construction input mass overflowed")),
        );
        let work = project_manual_craft_hand_work(registries, producer.process(), batches_nonzero)
            .unwrap_or_else(|error| {
                panic!("preservation construction hand-work projection failed: {error}")
            });
        let duration_ticks = work.duration().value();
        let step = ManualConstructionStep {
            process: producer.process(),
            batches,
            duration_ticks,
            input: producer.input(),
            input_mass,
            output,
        };
        let Some(mut route) =
            discover_manual_construction_route(registries, producer.input(), input_mass, visiting)
        else {
            continue;
        };
        route.attention_ticks = route
            .attention_ticks
            .checked_add(duration_ticks)
            .unwrap_or_else(|| panic!("preservation construction attention overflowed"));
        route.metabolic_energy_nj = route
            .metabolic_energy_nj
            .checked_add(work.resource_budget().metabolic_energy().nanojoules())
            .unwrap_or_else(|| panic!("preservation construction metabolic total overflowed"));
        route.hydration_ul = route
            .hydration_ul
            .checked_add(work.resource_budget().hydration().microliters())
            .unwrap_or_else(|| panic!("preservation construction hydration total overflowed"));
        route.steps.push(step);
        candidates.push(route);
    }
    assert!(visiting.remove(&output));
    let best_key = candidates
        .iter()
        .map(|route| {
            (
                route.attention_ticks,
                route.raw_mass.milligrams(),
                route.metabolic_energy_nj,
                route.hydration_ul,
            )
        })
        .min()?;
    let mut best = candidates.into_iter().filter(|route| {
        (
            route.attention_ticks,
            route.raw_mass.milligrams(),
            route.metabolic_energy_nj,
            route.hydration_ul,
        ) == best_key
    });
    let selected = best
        .next()
        .unwrap_or_else(|| unreachable!("manual construction best key came from a route"));
    assert!(
        best.next().is_none(),
        "preservation construction has multiple equally efficient observable manual routes to commodity {}; add an explicit player policy instead of using process identity",
        output.value()
    );
    Some(selected)
}

pub(super) fn preservation_construction_plan(
    registries: &Registries,
    profile: &MaterialAssemblyProfile,
) -> PreservationConstructionPlan {
    let mut routes = Vec::with_capacity(profile.inputs().len());
    let mut raw_mass = Mass::ZERO;
    let mut attention_ticks = 0_u64;
    for input in profile.inputs() {
        let route = discover_manual_construction_route(
            registries,
            input.commodity(),
            input.mass(),
            &mut BTreeSet::new(),
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
