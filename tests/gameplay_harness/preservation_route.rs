//! Resolves preservation-infrastructure construction through ordinary manual-production routes.

use std::collections::BTreeSet;

use deep_hearth::core::quantity::Mass;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

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
    pub(super) exertion_energy_nj: u128,
    pub(super) exertion_hydration_ul: u64,
}

#[derive(Clone, Debug)]
pub(super) struct PreservationConstructionPlan {
    pub(super) routes: Vec<ManualConstructionRoute>,
    pub(super) raw_mass: Mass,
    pub(super) attention_ticks: u64,
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
    let producers = registries
        .crafting()
        .definitions()
        .filter(|definition| {
            definition
                .outputs()
                .iter()
                .any(|candidate| candidate.commodity() == output)
        })
        .collect::<Vec<_>>();
    if producers.is_empty() {
        assert!(visiting.remove(&output));
        return Some(ManualConstructionRoute {
            raw_commodity: output,
            raw_mass: required_mass,
            steps: Vec::new(),
            attention_ticks: 0,
            exertion_energy_nj: 0,
            exertion_hydration_ul: 0,
        });
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
        let input_mass = Mass::from_milligrams(
            producer
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("preservation construction input mass overflowed")),
        );
        let duration_ticks = producer
            .duration()
            .value()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("preservation construction duration overflowed"));
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
        route.exertion_energy_nj = route
            .exertion_energy_nj
            .checked_add(
                producer
                    .exertion()
                    .energy_cost_per_tick()
                    .nanojoules()
                    .checked_mul(u128::from(duration_ticks))
                    .unwrap_or_else(|| panic!("preservation construction exertion overflowed")),
            )
            .unwrap_or_else(|| panic!("preservation construction exertion total overflowed"));
        route.exertion_hydration_ul = route
            .exertion_hydration_ul
            .checked_add(
                producer
                    .exertion()
                    .hydration_loss_per_tick()
                    .microliters()
                    .checked_mul(duration_ticks)
                    .unwrap_or_else(|| panic!("preservation construction hydration overflowed")),
            )
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
                route.exertion_energy_nj,
                route.exertion_hydration_ul,
            )
        })
        .min()?;
    let mut best = candidates.into_iter().filter(|route| {
        (
            route.attention_ticks,
            route.raw_mass.milligrams(),
            route.exertion_energy_nj,
            route.exertion_hydration_ul,
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
