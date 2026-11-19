//! Bounded, deliberate cardinal travel using ordinary sampled body controls.
//! It follows only the next publicly observable foothold, never a remote route.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::{Session, view};
use deep_hearth::geology::visible_weathered_copper_sites;
use deep_hearth::kobolds::KoboldSighting;
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerControlInput, project_player_standing_space,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::simulation::PlayerDirectControlInput;
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord, WorldPoint};
use deep_hearth::surface::{AdjacentSurfaceResource, available_visible_surface_landmarks};
use deep_hearth::survival::Vitality;
use deep_hearth::world::{
    VisibleWaterSource, available_visible_generated_water, available_visible_water_landmarks,
};

const CENTER_TOLERANCE: i64 = 30_000_000;
const STEP_TICK_LIMIT: u64 = 240;

enum LegResult {
    Arrived,
    Interrupted,
    Blocked,
}

struct TravelDiscoveries {
    resources: BTreeSet<(VoxelCoord, CommodityKey)>,
    water: Vec<VisibleWaterSource>,
    copper: Vec<VoxelCoord>,
}

impl TravelDiscoveries {
    fn capture(session: &Session) -> Self {
        let (registries, state) = (&session.registries, &session.state);
        // Near and distant sightings describe the same physical opportunities.
        // Moving across their display boundary must not manufacture a discovery.
        let resources = AdjacentSurfaceResource::visible_in_generated_world(registries, state)
            .map(|site| (site.position(), site.commodity()))
            .chain(
                available_visible_surface_landmarks(registries, state)
                    .into_iter()
                    .map(|site| (site.position(), site.commodity())),
            )
            .collect();
        let mut water = available_visible_generated_water(registries, state);
        water.extend(available_visible_water_landmarks(registries, state));
        Self {
            resources,
            water,
            copper: visible_weathered_copper_sites(registries, state),
        }
    }

    fn attention(&self, next: &Self) -> Option<&'static str> {
        if !next.resources.is_subset(&self.resources) {
            Some("A new loose resource is visible.")
        } else if next.water.iter().any(|site| !self.water.contains(site)) {
            Some("A new water source is visible.")
        } else if next.copper.iter().any(|site| !self.copper.contains(site)) {
            Some("Newly visible copper-stained rock.")
        } else {
            None
        }
    }
}

struct TravelObservations {
    position: VoxelCoord,
    residents: Vec<KoboldSighting>,
    discoveries: Option<TravelDiscoveries>,
}

impl TravelObservations {
    fn capture(session: &Session, position: VoxelCoord, steady: bool) -> Self {
        Self {
            position,
            residents: view::local_resident_sightings(&session.registries, &session.state),
            discoveries: (!steady).then(|| TravelDiscoveries::capture(session)),
        }
    }

    fn attention(&self, session: &Session, target: VoxelCoord) -> Option<&'static str> {
        view::resident_watch_attention(
            &self.residents,
            &view::local_resident_sightings(&session.registries, &session.state),
            self.position,
            target,
        )
        .or_else(|| {
            self.discoveries
                .as_ref()?
                .attention(&TravelDiscoveries::capture(session))
        })
    }
}

impl Session {
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
            || body.vertical_velocity() != VerticalVelocity::ZERO
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
        let observations = TravelObservations::capture(self, self.player_position()?, steady);
        let mut steps = 0;
        for _ in 0..cells {
            let origin = self.player_position()?;
            let Some(target) = self.trek_foothold(origin, movement, descend) else {
                output.push_str(if descend {
                    "Descent stopped: no clear two- or three-metre landing.\n"
                } else {
                    "Trek stopped: no clear level or one-metre foothold; choose another route.\n"
                });
                break;
            };
            match self.trek_leg(origin, target, urgency, &mut output)? {
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

    fn trek_foothold(
        &self,
        origin: VoxelCoord,
        movement: PlayerControlInput,
        descend: bool,
    ) -> Option<VoxelCoord> {
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
            .find(|&point| project_player_standing_space(&self.registries, &self.state, point))
    }

    fn trek_leg(
        &mut self,
        origin: VoxelCoord,
        target: VoxelCoord,
        urgency: (bool, bool, bool, bool),
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
                && body.vertical_velocity() == VerticalVelocity::ZERO;
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
            let component = |delta: i64| {
                if delta.abs() <= CENTER_TOLERANCE {
                    0
                } else {
                    (delta / 1_000).clamp(-cap, cap) as i32
                }
            };
            let horizontal = HorizontalControlInput::new(component(dx), component(dz))
                .map_err(|e| e.to_string())?;
            let input = PlayerControlInput::new(horizontal).with_jump(uphill && tick == 0);
            let report = self.step(PlayerDirectControlInput::new(input), output)?;
            if report.travel_interrupt || view::urgency(&self.registries, &self.state) != urgency {
                return Ok(LegResult::Interrupted);
            }
        }
        Ok(LegResult::Blocked)
    }
}
