//! Tick-local selection of competing physical shaft runs and endpoint stores.
//!
//! Materialized rollers and stored energy are authoritative; this planner only
//! arbitrates which non-overlapping transfers will execute on the next tick.

use super::*;

mod arbitration;
use arbitration::select_shaft_pairs;

#[derive(Clone, Copy)]
struct ShaftPair {
    run: usize,
    transfer: MechanicalTransmissionReceipt,
}

struct ShaftRun {
    rollers: Vec<StructuralElementId>,
}

/// Search every physically complete straight run before committing any shared
/// roller or endpoint to a transfer. Candidate evaluation is read-only, so a
/// smaller early coordinate must not starve a more productive crossing run.
fn available_shaft_pairs(
    registries: &Registries,
    state: &AppState,
    effective_structures: &StructureState,
    passive: &PassiveEnergyDissipationPlan,
    dismantled_this_tick: Option<StructuralElementId>,
) -> (Vec<ShaftRun>, Vec<ShaftPair>) {
    // The shaft must exist and be usable both before the tick and after
    // earlier structural settlement. A newly activated roller cannot spend
    // work until the following tick, while a cracked or failed existing roller
    // disconnects immediately rather than transmitting through debris.
    let rollers = unique_roller_cells(
        effective_structures
            .elements()
            .filter(|member| {
                is_usable_roller(member, dismantled_this_tick)
                    && state
                        .structures()
                        .get_element(member.id())
                        .is_some_and(|before| is_usable_roller(before, dismantled_this_tick))
            })
            .map(|member| (member.bounds().min(), member.id())),
    );
    let mut runs = Vec::new();
    let mut pairs = Vec::new();
    for position in rollers.keys().copied() {
        for (dx, dz) in [(1_i64, 0_i64), (0, 1)] {
            let Some((path, low, high)) = discover_shaft_run(&rollers, position, dx, dz) else {
                continue;
            };
            let Ok(segments) = u32::try_from(path.len()) else {
                continue;
            };
            let run = runs.len();
            let previous_pairs = pairs.len();
            for left in state.logistics().energy_stores_at(low) {
                for right in state.logistics().energy_stores_at(high) {
                    if let Some(transfer) =
                        plan_pair(registries, state, passive, path[0], segments, left, right)
                    {
                        pairs.push(ShaftPair { run, transfer });
                    }
                }
            }
            if pairs.len() > previous_pairs {
                // Retain one physical path per run, regardless of the number
                // of stores competing for its two endpoint positions.
                runs.push(ShaftRun { rollers: path });
            }
        }
    }
    (runs, pairs)
}

/// Select useful whole-nanojoule delivery before minimizing source draw.
/// Equal physics use the original deterministic shaft coordinate order, then
/// stable endpoint identity, so equal candidates do not depend on hash order.
fn compare_pairs(a: &ShaftPair, b: &ShaftPair) -> Ordering {
    b.transfer
        .delivered()
        .cmp(&a.transfer.delivered())
        .then_with(|| a.transfer.drawn().cmp(&b.transfer.drawn()))
        .then_with(|| a.run.cmp(&b.run))
        .then_with(|| a.transfer.source().cmp(&b.transfer.source()))
        .then_with(|| a.transfer.destination().cmp(&b.transfer.destination()))
}

/// Every shaft roller and store is used at most once per physical tick.
/// Candidate ranking is globally aware of intersecting runs and co-located
/// endpoints; after selecting a transfer, rejected pairs may still compete
/// through other non-overlapping paths.
pub(crate) fn decide_mechanical_transmission(
    registries: &Registries,
    state: &AppState,
    effective_structures: &StructureState,
    passive: &PassiveEnergyDissipationPlan,
    dismantled_this_tick: Option<StructuralElementId>,
) -> MechanicalTransmissionPlan {
    let (runs, mut pairs) = available_shaft_pairs(
        registries,
        state,
        effective_structures,
        passive,
        dismantled_this_tick,
    );
    pairs.sort_unstable_by(compare_pairs);
    let accepted = select_shaft_pairs(&pairs, &runs);
    MechanicalTransmissionPlan {
        transfers: accepted.into_iter().map(|pair| pair.transfer).collect(),
    }
}
