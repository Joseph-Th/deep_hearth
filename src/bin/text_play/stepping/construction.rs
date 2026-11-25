//! Exact supported timber placement and shelter preview over production admission.

use std::fmt::Write as _;

use super::{Session, feedback, view};
use deep_hearth::content::FORM_BOARD;
use deep_hearth::logistics::project_player_overhead_cover_footprint;
use deep_hearth::simulation::SecondaryActionOutcome;
use deep_hearth::spatial::VoxelCoord;

impl Session {
    pub(in crate::text_play) fn build_at(
        &mut self,
        index: usize,
        position: VoxelCoord,
        preview: bool,
    ) -> Result<String, String> {
        let inventory =
            deep_hearth::logistics::project_player_inventory(&self.registries, &self.state)
                .map_err(|error| error.to_string())?;
        let stack = inventory
            .carried()
            .stacks()
            .get(index - 1)
            .ok_or("no carried stack at that index")?;
        let validated = deep_hearth::simulation::validate_start_player_supported_timber_at(
            &self.registries,
            &self.state,
            stack,
            position,
        )
        .map_err(|outcome| match outcome {
            SecondaryActionOutcome::NoStructuralPlacementTarget => {
                "destination is not a clear cell within pointing reach".to_owned()
            }
            SecondaryActionOutcome::MaterialNotContextual => {
                "build-at requires a timber log or timber board stack".to_owned()
            }
            _ => feedback::secondary(&self.registries, &outcome),
        })?;
        if preview {
            let mut report = format!(
                "Ready: supported timber at ({},{},{}); current material, support and work admission passed.\n",
                position.x(),
                position.y(),
                position.z()
            );
            if stack.commodity().form() == FORM_BOARD
                && let Some(footprint) =
                    project_player_overhead_cover_footprint(&self.registries, &self.state)
            {
                if !footprint.intersects_column(position) {
                    report.push_str("Shelter preview: this panel is not over your current standing position; it will not provide overhead cover here.\n");
                } else if position.y() < footprint.minimum_y() {
                    let _ = writeln!(
                        report,
                        "Shelter preview: panel is below head clearance; overhead cover here requires a panel at height {} or higher. This placement may obstruct movement.",
                        footprint.minimum_y(),
                    );
                } else if !footprint.fits_single_column() {
                    report.push_str("Shelter preview: the player straddles multiple cells; one panel alone will not cover the complete standing footprint. Move toward the cell center or cover the other cells.\n");
                } else {
                    report.push_str("Shelter preview: this panel is above the entire current standing footprint and is positioned to provide overhead cover when completed. It does not block wind from the sides.\n");
                }
            }
            return Ok(report);
        }
        validated
            .commit(&mut self.state)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "Structural construction started at ({},{},{}).\n{}",
            position.x(),
            position.y(),
            position.z(),
            view::status(&self.registries, &self.state)
        ))
    }
}
