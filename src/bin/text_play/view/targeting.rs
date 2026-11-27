//! Pointed inspection using the same tree, terrain and structure decisions as action admission.

use std::fmt::Write as _;

use deep_hearth::content::{FORM_BOARD, MATERIAL_WOOD};
use deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
use deep_hearth::logistics::{PlayerActiveSelection, project_player_standing_space};
use deep_hearth::material::CommodityKey;
use deep_hearth::simulation::{
    SecondaryActionOutcome, project_pointed_structural_break_target,
    project_pointed_timber_cover_target, project_pointed_tree_break_target,
    validate_pointed_terrain_excavation,
};
use deep_hearth::spatial::{ViewDirection, VoxelCoord};
use deep_hearth::world::raycast_player_block;
use deep_hearth::{AppState, Registries};

use super::{grams, pos, runway_label};

pub(super) fn pointed_report(
    registries: &Registries,
    state: &AppState,
    aim: ViewDirection,
    output: &mut String,
) {
    if let Some(root) = project_pointed_tree_break_target(registries, state, aim) {
        let _ = writeln!(
            output,
            "Break target: standing tree at {}. Felling requires a selected carried chopping tool and a same-level adjacent position.",
            pos(root),
        );
    } else {
        terrain_and_structure_preview(registries, state, aim, output);
    }
    if state.logistics().active_selection()
        == Some(PlayerActiveSelection::material(CommodityKey::new(
            MATERIAL_WOOD,
            FORM_BOARD,
        )))
    {
        output.push_str(&timber_cover_preview(registries, state, aim));
    }
}

fn terrain_and_structure_preview(
    registries: &Registries,
    state: &AppState,
    aim: ViewDirection,
    output: &mut String,
) {
    match raycast_player_block(registries, state, aim) {
        Ok(Some(target)) => {
            let block_name = registries
                .world()
                .block_for_state(target.state())
                .map_or("unknown block", |block| block.name());
            let _ = writeln!(
                output,
                "Pointed at {block_name} at {}, face {:?}",
                pos(target.voxel()),
                target.face()
            );
            if matches!(
                state.logistics().active_selection(),
                Some(PlayerActiveSelection::Equipment(_))
            ) && registries
                .world()
                .block_for_state(target.state())
                .is_some_and(|block| block.excavation().is_some())
            {
                output.push_str(&excavation_preview(registries, state, aim));
            }
        }
        Ok(None) => output.push_str("No targetable block within reach in that direction.\n"),
        Err(error) => {
            let _ = writeln!(output, "Pointing unavailable: {error}");
        }
    }
    output.push_str(&structural_break_preview(registries, state, aim));
}

fn excavation_preview(registries: &Registries, state: &AppState, aim: ViewDirection) -> String {
    match validate_pointed_terrain_excavation(registries, state, aim) {
        Ok(Some(plan)) => {
            let work = plan.work();
            let mass = work.excavated_mass();
            let remaining = work.source_mass_before();
            let cuts = remaining.milligrams().div_ceil(mass.milligrams());
            let duration = work
                .duration_control_ticks()
                .map(|ticks| ticks.div_ceil(CONTROL_TICKS_PER_SIMULATION_TICK));
            let destination = if work.output().carried_stockpile().is_some() {
                "pack"
            } else {
                "ground"
            };
            format!(
                "Excavation preview: {} g solid terrain remains; next cut {} g in {} to {destination}. At this cut size: {cuts} cuts; tool wear and reserves can change subsequent cuts. Use 'break DIRECTION repeat N' for bounded repeated cuts.\n",
                grams(remaining),
                grams(mass),
                runway_label(duration)
            )
        }
        Ok(None) => String::new(),
        Err(error) => format!("Excavation unavailable: {error}\n"),
    }
}

fn structural_break_preview(
    registries: &Registries,
    state: &AppState,
    aim: ViewDirection,
) -> String {
    match project_pointed_structural_break_target(registries, state, aim) {
        Ok(Some(element)) => state
            .structures()
            .get_element(element)
            .map_or_else(String::new, |member| {
                let label = if member.geometry().covers_horizontal_footprint() {
                    "panel"
                } else {
                    "framing"
                };
                format!(
                    "Break target: {label} at {} | {} g. 'break' selects this structure before terrain.\n",
                    pos(member.bounds().min()),
                    grams(member.embodied_mass()),
                )
            }),
        Err(count) => format!("Break target: {count} overlapping structures; aim more precisely.\n"),
        Ok(None) => String::new(),
    }
}

/// Show the production target before the player spends timber boards. A
/// standing-space hint is deliberately not a general pathfinding guarantee.
fn timber_cover_preview(registries: &Registries, state: &AppState, aim: ViewDirection) -> String {
    let target = match project_pointed_timber_cover_target(registries, state, aim) {
        Ok(Some(position)) => position,
        Ok(None) => {
            return "Cover preview: no load-bearing member at this aim. Raise framing above standing height, then aim toward open space beside it.\n".to_owned();
        }
        Err(SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { position }) => {
            return format!(
                "Cover preview: a panel at {} would intersect your body. Move outside that destination and aim at an exposed face of the raised post.\n",
                pos(position),
            );
        }
        Err(
            SecondaryActionOutcome::StructuralPlacementOccupied { position, .. }
            | SecondaryActionOutcome::StructuralPlacementLocationOccupied { position },
        ) => {
            return format!(
                "Cover preview: {} is already occupied. Aim at a free cell beside the raised support, above clear walking space.\n",
                pos(position),
            );
        }
        Err(SecondaryActionOutcome::StructuralPlacementWorldUnavailable { position }) => {
            return format!(
                "Cover preview: terrain at {} is not yet available. Approach the site to load it before building.\n",
                pos(position),
            );
        }
        Err(_) => {
            return "Cover preview: placement unavailable at this aim; try an exposed side of the raised support.\n".to_owned();
        }
    };
    let mut output = format!("Cover preview: panel would occupy {}. ", pos(target));
    let Some(standing_y) = target.y().checked_sub(2) else {
        output.push_str("No standing-height space can be projected below it.\n");
        return output;
    };
    let beneath = VoxelCoord::new(target.x(), standing_y, target.z());
    if !project_player_standing_space(registries, state, beneath) {
        let _ = writeln!(
            output,
            "No clear, grounded standing-height space at {} underneath it. A support post or terrain may occupy the standing column; aim at a free cell beside raised framing to make shelter.",
            pos(beneath),
        );
    } else {
        let _ = writeln!(
            output,
            "Potential standing space at {} below the panel; verify a walkable approach before building.",
            pos(beneath),
        );
    }
    output
}
