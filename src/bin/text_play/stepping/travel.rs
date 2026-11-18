//! Bounded, deliberate cardinal travel using ordinary sampled body controls.
//! It follows only the next publicly observable foothold, never a remote route.

use std::fmt::Write as _;

use super::{Session, view};
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerControlInput, project_player_standing_space,
};
use deep_hearth::simulation::PlayerDirectControlInput;
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord, WorldPoint};
use deep_hearth::surface::available_visible_surface_landmarks;
use deep_hearth::survival::Vitality;

const CENTER_TOLERANCE: i64 = 30_000_000;
const STEP_TICK_LIMIT: u64 = 240;

enum LegResult {
    Arrived,
    Interrupted,
    Blocked,
}

impl Session {
    /// Travel one to 32 cells in a chosen cardinal direction. Stop before
    /// a cliff, an obstructed step, an unexpected fall, or a new discovery.
    pub(in crate::text_play) fn trek(
        &mut self,
        movement: PlayerControlInput,
        cells: u64,
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
        let before = view::local_cues(&self.registries, &self.state);
        let landmarks = available_visible_surface_landmarks(&self.registries, &self.state);
        let mut steps = 0;
        for _ in 0..cells {
            let origin = self.player_position()?;
            let Some(target) = self.trek_foothold(origin, movement) else {
                output.push_str(
                    "Trek stopped: no clear level or one-metre foothold; choose another route.\n",
                );
                break;
            };
            match self.trek_leg(origin, target, urgency, &mut output)? {
                LegResult::Arrived => steps += 1,
                LegResult::Interrupted => {
                    output.push_str("Trek interrupted by an event or changed condition.\n");
                    break;
                }
                LegResult::Blocked => {
                    output.push_str("Trek stopped: physical approach could not be completed.\n");
                    break;
                }
            }
            let after = view::local_cues(&self.registries, &self.state);
            if let Some(description) = before.work_attention(&after, target) {
                let _ = writeln!(output, "Trek stopped: {description}");
                break;
            }
            if available_visible_surface_landmarks(&self.registries, &self.state)
                .iter()
                .any(|source| !landmarks.contains(source))
            {
                output.push_str("Trek stopped: a new distant resource is visible.\n");
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
    ) -> Option<VoxelCoord> {
        let x = origin
            .x()
            .checked_add(i64::from(movement.horizontal().x_ppm().signum()))?;
        let z = origin
            .z()
            .checked_add(i64::from(movement.horizontal().z_ppm().signum()))?;
        [0_i64, -1, 1]
            .into_iter()
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
