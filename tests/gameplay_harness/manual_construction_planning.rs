//! Recursive topology planning for manual construction from disclosed raw roots.

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::crafting::project_manual_craft_hand_work;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::{CommoditySource, ProcessEquipmentRole, Registries};

#[derive(Clone, Copy, Debug)]
#[allow(
    dead_code,
    reason = "shared route evidence fields are consumed by different focused gameplay roots"
)]
pub(super) struct ManualConstructionStep {
    pub(super) process: deep_hearth::production::ProcessId,
    pub(super) batches: u64,
    pub(super) duration_ticks: u64,
    pub(super) input: CommodityKey,
    pub(super) input_mass: Mass,
    pub(super) output: CommodityKey,
}

#[derive(Clone, Debug)]
#[allow(
    dead_code,
    reason = "shared route evidence fields are consumed by different focused gameplay roots"
)]
pub(super) struct ManualConstructionRoute {
    pub(super) raw_commodity: CommodityKey,
    pub(super) raw_mass: Mass,
    pub(super) steps: Vec<ManualConstructionStep>,
    pub(super) attention_ticks: u64,
    pub(super) metabolic_energy_nj: u128,
    pub(super) hydration_ul: u64,
}

fn discover_manual_construction_route(
    registries: &Registries,
    output: CommodityKey,
    required_mass: Mass,
    raw_roots: &BTreeSet<CommodityKey>,
    visiting: &mut BTreeSet<CommodityKey>,
    context: &'static str,
) -> Option<ManualConstructionRoute> {
    if raw_roots.contains(&output) {
        return Some(ManualConstructionRoute {
            raw_commodity: output,
            raw_mass: required_mass,
            steps: Vec::new(),
            attention_ticks: 0,
            metabolic_energy_nj: 0,
            hydration_ul: 0,
        });
    }
    if !visiting.insert(output) {
        return None;
    }
    let producers = registries
        .commodity_handbook_entry(output)
        .into_iter()
        .flat_map(|entry| entry.sources().to_vec())
        .filter_map(|source| match source {
            CommoditySource::ManualCraft { process, .. } => Some(
                registries
                    .crafting()
                    .get_manual(process)
                    .unwrap_or_else(|| {
                        panic!("commodity handbook exposed an unknown manual process")
                    }),
            ),
            CommoditySource::SurfaceGathering { .. }
            | CommoditySource::EquipmentDisassembly { .. }
            | CommoditySource::EnergyStoreDisassembly { .. }
            | CommoditySource::StorageDismantling { .. }
            | CommoditySource::EquipmentMaintenanceSpent { .. }
            | CommoditySource::OreProcessing { .. }
            | CommoditySource::ThermalPhaseChange { .. } => None,
        })
        .filter(|producer| {
            registries
                .process_topology(producer.process())
                .unwrap_or_else(|| panic!("manual construction producer lost process topology"))
                .equipment_role()
                != ProcessEquipmentRole::Required
        })
        .collect::<Vec<_>>();
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
            "gameplay harness {context} manual producer {} has zero useful output",
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
                .unwrap_or_else(|| {
                    panic!("gameplay harness {context} route input mass overflowed")
                }),
        );
        let Some(mut route) = discover_manual_construction_route(
            registries,
            producer.input(),
            input_mass,
            raw_roots,
            visiting,
            context,
        ) else {
            continue;
        };
        let work = project_manual_craft_hand_work(
            registries,
            producer.process(),
            NonZeroU64::new(batches)
                .unwrap_or_else(|| unreachable!("nonzero output demand yields nonzero batches")),
        )
        .unwrap_or_else(|error| {
            panic!("gameplay harness {context} hand-work projection failed: {error}")
        });
        let duration_ticks = work.duration().value();
        route.attention_ticks = route
            .attention_ticks
            .checked_add(duration_ticks)
            .unwrap_or_else(|| panic!("gameplay harness {context} route attention overflowed"));
        route.metabolic_energy_nj = route
            .metabolic_energy_nj
            .checked_add(work.resource_budget().metabolic_energy().nanojoules())
            .unwrap_or_else(|| panic!("gameplay harness {context} route metabolism overflowed"));
        route.hydration_ul = route
            .hydration_ul
            .checked_add(work.resource_budget().hydration().microliters())
            .unwrap_or_else(|| panic!("gameplay harness {context} route hydration overflowed"));
        route.steps.push(ManualConstructionStep {
            process: producer.process(),
            batches,
            duration_ticks,
            input: producer.input(),
            input_mass,
            output,
        });
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
        .unwrap_or_else(|| unreachable!("best manual construction key came from a route"));
    assert!(
        best.next().is_none(),
        "gameplay harness {context} has equally efficient observable manual routes to commodity {}; add an explicit actor preference instead of using process identity",
        output.value()
    );
    Some(selected)
}

pub(super) fn manual_construction_route_from_roots(
    registries: &Registries,
    output: CommodityKey,
    required_mass: Mass,
    raw_roots: &[CommodityKey],
    context: &'static str,
) -> Option<ManualConstructionRoute> {
    assert!(
        !required_mass.is_zero(),
        "gameplay harness {context} requires nonzero produced mass"
    );
    assert!(
        !raw_roots.is_empty(),
        "gameplay harness {context} requires at least one disclosed raw root"
    );
    discover_manual_construction_route(
        registries,
        output,
        required_mass,
        &raw_roots.iter().copied().collect(),
        &mut BTreeSet::new(),
        context,
    )
}
