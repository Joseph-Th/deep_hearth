//! Observed precision salvage routes shared by den eviction and patrol planning.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceRecord;

use super::super::DEN_LOOSE_CACHE_LIMIT;
use super::super::junkworks::den_or_relay_needs_striker;
use super::super::visibility::scout_visible_from;
use super::routes::{ScavengeRoute, two_edge_candidates, unguarded_unlit_cell};
use super::{is_contestable_drop, is_portable_drop, matches_salvage_goal, scout_wants_loose_drop};

type SalvageSite = (VoxelCoord, Option<VoxelCoord>);

/// One observed route and custody envelope for raid selection and den eviction.
struct SalvageSearch<'a> {
    registries: &'a Registries,
    state: &'a AppState,
    route: ScavengeRoute,
    observer: VoxelCoord,
    at: SimulationTick,
    needs_striker: bool,
}

impl<'a> SalvageSearch<'a> {
    fn new(
        registries: &'a Registries,
        state: &'a AppState,
        route: ScavengeRoute,
        observer: VoxelCoord,
        at: SimulationTick,
    ) -> Self {
        Self {
            registries,
            state,
            route,
            observer,
            at,
            needs_striker: den_or_relay_needs_striker(state, route.den),
        }
    }

    fn drops_at(
        &self,
        site: SalvageSite,
    ) -> impl Iterator<Item = (SalvageSite, &'a SurfaceResourceRecord)> + '_ {
        let (first, far) = site;
        let destination = far.unwrap_or(first);
        let mut drops = self
            .state
            .surface()
            .available_resources_at(Some(destination))
            .filter(|resource| {
                matches_salvage_goal(resource, self.needs_striker)
                    && is_contestable_drop(resource, DEN_LOOSE_CACHE_LIMIT)
                    && scout_wants_loose_drop(self.registries, self.state, resource, self.at)
            })
            .peekable();
        // Skip costly sight and illumination checks at empty or ineligible sites.
        let safe = |cell| {
            unguarded_unlit_cell(
                self.registries,
                self.state,
                self.observer,
                cell,
                self.route.player_position,
            )
        };
        let visible = drops.peek().is_some()
            && scout_visible_from(self.registries, self.state, self.observer, destination)
            && safe(first)
            && far.is_none_or(safe);
        visible
            .then_some(drops)
            .into_iter()
            .flatten()
            .map(move |resource| (site, resource))
    }

    fn local(&self) -> impl Iterator<Item = (SalvageSite, &'a SurfaceResourceRecord)> + '_ {
        self.drops_at((self.route.junction, None))
    }

    fn adjacent(&self) -> impl Iterator<Item = (SalvageSite, &'a SurfaceResourceRecord)> + '_ {
        self.state
            .logistics()
            .traversable_destinations(self.route.junction)
            .filter(|first| *first != self.route.den)
            .flat_map(|first| self.drops_at((first, None)))
    }

    fn far(&self) -> impl Iterator<Item = (SalvageSite, &'a SurfaceResourceRecord)> + '_ {
        two_edge_candidates(self.state, self.route)
            .flat_map(|(first, goal)| self.drops_at((first, Some(goal))))
    }
}

fn best_at_capacity<'a>(
    drops: impl Iterator<Item = (SalvageSite, &'a SurfaceResourceRecord)>,
    capacity: Mass,
) -> Option<(bool, SalvageSite)> {
    drops
        .map(|(site, resource)| {
            (
                !is_portable_drop(resource) || resource.remaining_mass() > capacity,
                resource.id(),
                site,
            )
        })
        .min()
        .map(|(needs_split, _, site)| (needs_split, site))
}

/// Whole components outrank splits, then the shortest real return route wins.
/// The junction itself is a zero-detour target, not an extra movement edge.
pub(in crate::kobolds) fn reachable_inherited_salvage(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    observer: VoxelCoord,
    capacity: Mass,
    at: SimulationTick,
) -> Option<SalvageSite> {
    if capacity.is_zero() {
        return None;
    }
    let search = SalvageSearch::new(registries, state, route, observer, at);
    let local = best_at_capacity(search.local(), capacity);
    if let Some((false, site)) = local {
        return Some(site);
    }
    let adjacent = best_at_capacity(search.adjacent(), capacity);
    if let Some((false, site)) = adjacent {
        return Some(site);
    }
    let far = best_at_capacity(search.far(), capacity);
    far.filter(|(needs_split, _)| !needs_split)
        .or(local)
        .or(adjacent)
        .or(far)
        .map(|(_, site)| site)
}

/// Capacity-independent evidence for one den-care decision. The existence of
/// the survey indicates eligible salvage; the smallest intact component answers
/// every capacity query without another search.
pub(in crate::kobolds) struct VisibleInheritedSalvage {
    minimum_whole_mass: Option<Mass>,
}

impl VisibleInheritedSalvage {
    pub(in crate::kobolds) fn whole_fits(&self, capacity: Mass) -> bool {
        self.minimum_whole_mass.is_some_and(|mass| mass <= capacity)
    }
}

pub(in crate::kobolds) fn survey_visible_inherited_salvage(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    observer: VoxelCoord,
    at: SimulationTick,
) -> Option<VisibleInheritedSalvage> {
    let search = SalvageSearch::new(registries, state, route, observer, at);
    let mut drops = search
        .local()
        .chain(search.adjacent())
        .chain(search.far())
        .peekable();
    drops.peek()?;
    let minimum_whole_mass = drops
        .filter_map(|(_, resource)| is_portable_drop(resource).then_some(resource.remaining_mass()))
        .min();
    Some(VisibleInheritedSalvage { minimum_whole_mass })
}
