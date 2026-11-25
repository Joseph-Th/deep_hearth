//! Bounded, deliberate cardinal travel using ordinary sampled body controls.
//! It follows only the next publicly observable foothold, never a remote route.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::{Session, view};
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::geology::visible_weathered_copper_sites;
use deep_hearth::logistics::{
    HorizontalControlInput, MAX_LEDGE_CLIMB_METERS, PlayerControlInput, player_body_is_grounded,
    project_player_climb_target, project_player_descent_approach, project_player_jump_rise_bound,
    project_player_standing_space,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::simulation::PlayerDirectControlInput;
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord, WorldPoint};
use deep_hearth::surface::{AdjacentSurfaceResource, available_visible_surface_landmarks};
use deep_hearth::survival::Vitality;
use deep_hearth::world::{
    ForageScentKind, assess_player_forage_scents, available_visible_generated_water,
    available_visible_water_landmarks,
};

const CENTER_TOLERANCE: i64 = 30_000_000;
const STEP_TICK_LIMIT: u64 = 240;
// Long-range landmarks inform route choice in 'look'. Interrupt a moving
// player only for discoveries near enough to act on without a major detour.
const DISCOVERY_ATTENTION_RADIUS: u64 = 12;

enum LegResult {
    Arrived,
    Interrupted,
    Blocked,
}

struct TravelDiscoveries {
    observer: Option<VoxelCoord>,
    resources: BTreeSet<(VoxelCoord, CommodityKey)>,
    water: BTreeSet<(VoxelCoord, FluidStoreId)>,
    copper: BTreeSet<VoxelCoord>,
    scents: BTreeSet<ForageScentKind>,
}

impl TravelDiscoveries {
    fn capture(session: &Session) -> Self {
        let (registries, state) = (&session.registries, &session.state);
        // Remember every already visible landmark, even outside the attention radius.
        // Otherwise approaching a known distant site would masquerade as discovering it.
        let resources = AdjacentSurfaceResource::visible_in_generated_world(registries, state)
            .map(|site| (site.position(), site.commodity()))
            .chain(
                available_visible_surface_landmarks(registries, state)
                    .into_iter()
                    .map(|site| (site.position(), site.commodity())),
            )
            .collect();
        let water = available_visible_generated_water(registries, state)
            .into_iter()
            .chain(available_visible_water_landmarks(registries, state))
            .map(|source| (source.position(), source.store()))
            .collect();
        Self {
            observer: state.logistics().player().map(|player| player.position()),
            resources,
            water,
            copper: visible_weathered_copper_sites(registries, state)
                .into_iter()
                .collect(),
            scents: assess_player_forage_scents(registries, state)
                .into_iter()
                .map(|scent| scent.kind())
                .collect(),
        }
    }

    fn close_to(&self, site: VoxelCoord) -> bool {
        self.observer.is_some_and(|at| {
            // The player travels cardinally. A diagonal sighting inside the
            // visibility square can still require a long off-route detour.
            let east_west = at.x().abs_diff(site.x());
            east_west <= DISCOVERY_ATTENTION_RADIUS
                && at.z().abs_diff(site.z()) <= DISCOVERY_ATTENTION_RADIUS - east_west
        })
    }

    fn attention(&self, next: &Self) -> Option<&'static str> {
        if next
            .resources
            .iter()
            .any(|site| next.close_to(site.0) && !self.resources.contains(site))
        {
            Some("A new loose resource is visible.")
        } else if next
            .water
            .iter()
            .any(|site| next.close_to(site.0) && !self.water.contains(site))
        {
            Some("A new water source is visible.")
        } else if next
            .copper
            .iter()
            .any(|site| next.close_to(*site) && !self.copper.contains(site))
        {
            Some("Newly visible copper-stained rock.")
        } else if !next.scents.is_subset(&self.scents) {
            Some("A new forage scent is noticeable.")
        } else {
            None
        }
    }

    /// A landmark that leaves sight remains known for this trek. Only scents
    /// describe a changing present-tense observation and may become new again
    /// after the wind stops carrying them.
    fn observe(&mut self, next: Self) -> Option<&'static str> {
        let attention = self.attention(&next);
        self.observer = next.observer;
        self.resources.extend(next.resources);
        self.water.extend(next.water);
        self.copper.extend(next.copper);
        self.scents = next.scents;
        attention
    }
}

struct TravelObservations {
    residents: view::ResidentWatch,
    discoveries: Option<TravelDiscoveries>,
}

impl TravelObservations {
    fn capture(session: &Session, position: VoxelCoord, steady: bool) -> Self {
        Self {
            residents: view::ResidentWatch::capture(&session.registries, &session.state, position),
            discoveries: (!steady).then(|| TravelDiscoveries::capture(session)),
        }
    }

    fn attention(&mut self, session: &Session, target: VoxelCoord) -> Option<&'static str> {
        if let Some(description) = self.residents.observe(view::ResidentWatch::capture(
            &session.registries,
            &session.state,
            target,
        )) {
            return Some(description);
        }
        let previous = self.discoveries.as_mut()?;
        let next = TravelDiscoveries::capture(session);
        previous.observe(next)
    }
}

/// Convert a physical centering error to a bounded ordinary movement input.
fn centering_input(dx: i64, dz: i64, cap: i64) -> Result<HorizontalControlInput, String> {
    let component = |delta: i64| {
        if delta.abs() <= CENTER_TOLERANCE {
            0
        } else {
            (delta / 1_000).clamp(-cap, cap) as i32
        }
    };
    HorizontalControlInput::new(component(dx), component(dz)).map_err(|error| error.to_string())
}

impl Session {
    /// Move toward the centre of the current grounded cell with sampled
    /// low-amplitude input. Never snap, teleport, or bypass the motion owner.
    pub(in crate::text_play) fn center_in_cell(&mut self) -> Result<String, String> {
        if self
            .state
            .survival()
            .player()
            .is_none_or(|player| player.vitality() == Vitality::ZERO)
        {
            return Err("a living admitted player is required to center".to_owned());
        }
        let origin = self.player_position()?;
        let body = self
            .state
            .logistics()
            .player()
            .ok_or("no player admitted")?
            .body();
        if self.state.player_work().active().is_some()
            || body.horizontal_velocity() != HorizontalVelocity::ZERO
            || body.vertical_velocity() != VerticalVelocity::ZERO
            || !player_body_is_grounded(&self.registries, &self.state)
        {
            return Err("finish work and settle movement before centering".to_owned());
        }
        let center = WorldPoint::centered_on_voxel_floor(origin)
            .ok_or("cell centre is outside the continuous world")?;
        if center.y_nanometers() != body.foot_position().y_nanometers()
            || !project_player_standing_space(&self.registries, &self.state, origin)
        {
            return Err("no grounded standing space at the centre of this cell".to_owned());
        }
        let initial = view::urgency(&self.registries, &self.state);
        let mut output = String::new();
        for _ in 0..STEP_TICK_LIMIT {
            let body = self
                .state
                .logistics()
                .player()
                .ok_or("no player admitted")?
                .body();
            let foot = body.foot_position();
            let dx = center.x_nanometers() - foot.x_nanometers();
            let dz = center.z_nanometers() - foot.z_nanometers();
            let settled = body.horizontal_velocity() == HorizontalVelocity::ZERO
                && body.vertical_velocity() == VerticalVelocity::ZERO
                && player_body_is_grounded(&self.registries, &self.state);
            if dx.abs() <= CENTER_TOLERANCE && dz.abs() <= CENTER_TOLERANCE && settled {
                output.push_str("Centred in the current cell (within 3 cm).\n");
                output.push_str(&view::look(&self.registries, &self.state, None));
                return Ok(output);
            }
            let horizontal = centering_input(dx, dz, 150_000)?;
            let input = PlayerDirectControlInput::new(PlayerControlInput::new(horizontal));
            let report = self.step(input, &mut output)?;
            if report.noteworthy
                || view::urgency(&self.registries, &self.state) != initial
                || self.player_position()? != origin
            {
                output.push_str("Centering interrupted by a physical event or condition change.\n");
                output.push_str(&view::look(&self.registries, &self.state, None));
                return Ok(output);
            }
            let next = self
                .state
                .logistics()
                .player()
                .ok_or("no player admitted")?
                .body();
            if !horizontal.is_neutral()
                && next.foot_position() == foot
                && next.horizontal_velocity() == HorizontalVelocity::ZERO
            {
                break;
            }
        }
        output.push_str(
            "Centering stopped: could not reach the centre with the current clearance.\n",
        );
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }

    /// Travel one to 32 cells in a chosen cardinal direction. Stop before
    /// a cliff, an obstructed step, an unexpected fall, or a new discovery.
    pub(in crate::text_play) fn trek(
        &mut self,
        movement: PlayerControlInput,
        cells: u64,
        steady: bool,
        descend: bool,
    ) -> Result<String, String> {
        let body = self
            .state
            .logistics()
            .player()
            .ok_or("no player admitted")?
            .body();
        if self.state.player_work().active().is_some()
            || body.horizontal_velocity() != HorizontalVelocity::ZERO
            || (!movement.climb()
                && (body.vertical_velocity() != VerticalVelocity::ZERO
                    || !player_body_is_grounded(&self.registries, &self.state)))
        {
            return Err("finish work and settle movement before trekking".to_owned());
        }
        if self
            .state
            .survival()
            .player()
            .is_none_or(|p| p.vitality() == Vitality::ZERO)
        {
            return Err("a living player is required to trek".to_owned());
        }
        let mut output = String::new();
        let start = self.state.control_tick().value();
        let urgency = view::urgency(&self.registries, &self.state);
        let mut observations = TravelObservations::capture(self, self.player_position()?, steady);
        let mut steps = 0;
        for _ in 0..cells {
            let origin = self.player_position()?;
            let Some((target, climb)) = self.plan_trek_leg(origin, movement, descend, &mut output)
            else {
                break;
            };
            match self.trek_leg(origin, target, climb, urgency, &mut output)? {
                LegResult::Arrived => steps += 1,
                LegResult::Interrupted => {
                    if self.player_position()? == target {
                        steps += 1;
                        output.push_str("Entered target cell; movement may still need settling.\n");
                    }
                    output.push_str("Trek interrupted by an event or changed condition.\n");
                    break;
                }
                LegResult::Blocked => {
                    output.push_str("Trek stopped: physical approach could not be completed.\n");
                    if target.y() > origin.y() && climb.is_none() {
                        if project_player_climb_target(
                            &self.registries,
                            &self.state,
                            movement.horizontal(),
                        ) == Some(target)
                        {
                            output.push_str("A fixed-ledge climb is currently projected. Try 'climb' in the same direction after settling.\n");
                        } else {
                            output.push_str("No clear climb is projected from this position. Check headroom (especially under roofs), center if possible, or take a different exit.\n");
                        }
                    }
                    break;
                }
            }
            if let Some(description) = observations.attention(self, target) {
                let _ = writeln!(output, "Trek stopped: {description}");
                break;
            }
        }
        let actual = self.state.control_tick().value() - start;
        let _ = writeln!(
            output,
            "Trekked {steps} of {cells} cells in {actual} control ticks."
        );
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }

    /// Choose a reachable foothold and the control maneuver appropriate to the
    /// current carried load, without altering the canonical motion owner.
    fn plan_trek_leg(
        &self,
        origin: VoxelCoord,
        movement: PlayerControlInput,
        descend: bool,
        output: &mut String,
    ) -> Option<(VoxelCoord, Option<HorizontalControlInput>)> {
        let Some(target) = self.trek_foothold(origin, movement, descend) else {
            output.push_str(if movement.climb() {
                "Climb stopped: no reachable fixed ledge with clear lift and landing space (up to two metres).\n"
            } else if descend {
                "Descent stopped: no clear two- or three-metre landing.\n"
            } else {
                "Trek stopped: no clear level or one-metre foothold; choose another route.\n"
            });
            return None;
        };
        if origin.y().saturating_sub(target.y()) > MAX_LEDGE_CLIMB_METERS {
            output.push_str("Warning: this drop exceeds the short-ledge climbing limit; returning this way may be impossible.\n");
        }
        if movement.climb() {
            return Some((target, Some(movement.horizontal())));
        }
        let jump_too_low = target.y() > origin.y()
            && project_player_jump_rise_bound(&self.registries, &self.state)
                .is_some_and(|height| height.micrometers() < 1_000_000);
        if !jump_too_low {
            return Some((target, None));
        }
        if project_player_climb_target(&self.registries, &self.state, movement.horizontal())
            != Some(target)
        {
            output.push_str("Trek stopped: the carried load prevents clearing this jump and there is no clear climb.\n");
            return None;
        }
        output.push_str("The current load limits the jump height; climbing the ledge instead.\n");
        Some((target, Some(movement.horizontal())))
    }

    fn trek_foothold(
        &self,
        origin: VoxelCoord,
        movement: PlayerControlInput,
        descend: bool,
    ) -> Option<VoxelCoord> {
        if movement.climb() {
            return project_player_climb_target(
                &self.registries,
                &self.state,
                movement.horizontal(),
            );
        }
        let x = origin
            .x()
            .checked_add(i64::from(movement.horizontal().x_ppm().signum()))?;
        let z = origin
            .z()
            .checked_add(i64::from(movement.horizontal().z_ppm().signum()))?;
        let offsets: &[i64] = if descend { &[-2, -3] } else { &[0, -1, 1] };
        // A conscious drop must not bypass a shallower standing surface.
        if descend && self.trek_foothold(origin, movement, false).is_some() {
            return None;
        }
        offsets
            .iter()
            .copied()
            .filter_map(|dy| Some(VoxelCoord::new(x, origin.y().checked_add(dy)?, z)))
            .find(|&point| {
                if point.y() < origin.y() {
                    project_player_descent_approach(&self.registries, &self.state, point)
                } else {
                    project_player_standing_space(&self.registries, &self.state, point)
                }
            })
    }

    fn trek_leg(
        &mut self,
        origin: VoxelCoord,
        target: VoxelCoord,
        climb: Option<HorizontalControlInput>,
        urgency: Option<view::Urgency>,
        output: &mut String,
    ) -> Result<LegResult, String> {
        let center = WorldPoint::centered_on_voxel_floor(target)
            .ok_or("foothold outside continuous world")?;
        let uphill = target.y() > origin.y();
        let downhill = target.y() < origin.y();
        for tick in 0..STEP_TICK_LIMIT {
            let body = self
                .state
                .logistics()
                .player()
                .ok_or("no player admitted")?
                .body();
            let foot = body.foot_position();
            let dx = center.x_nanometers() - foot.x_nanometers();
            let dz = center.z_nanometers() - foot.z_nanometers();
            let settled = body.horizontal_velocity() == HorizontalVelocity::ZERO
                && body.vertical_velocity() == VerticalVelocity::ZERO
                && player_body_is_grounded(&self.registries, &self.state);
            if dx.abs() <= CENTER_TOLERANCE && dz.abs() <= CENTER_TOLERANCE && settled {
                return Ok(if foot.y_nanometers() == center.y_nanometers() {
                    LegResult::Arrived
                } else {
                    LegResult::Blocked
                });
            }
            let position = self.player_position()?;
            if (position.x(), position.z()) != (origin.x(), origin.z())
                && (position.x(), position.z()) != (target.x(), target.z())
            {
                return Ok(LegResult::Blocked);
            }
            // Modest horizontal speed on descents limits overshoot while
            // gravity acts. Jump starts once; subsequent input simply steers.
            let cap = if downhill {
                150_000
            } else if uphill && tick < 12 {
                650_000
            } else {
                300_000
            };
            let horizontal = centering_input(dx, dz, cap)?;
            let input = if let Some(direction) = climb
                && (position.x(), position.z()) == (origin.x(), origin.z())
            {
                PlayerControlInput::new(direction).with_climb(true)
            } else {
                PlayerControlInput::new(horizontal)
                    .with_jump(climb.is_none() && uphill && tick == 0)
            };
            let report = self.step(PlayerDirectControlInput::new(input), output)?;
            if report.travel_interrupt || view::urgency(&self.registries, &self.state) != urgency {
                return Ok(LegResult::Interrupted);
            }
        }
        Ok(LegResult::Blocked)
    }
}

#[cfg(test)]
#[path = "travel_tests.rs"]
mod tests;
