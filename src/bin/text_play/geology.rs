//! Acquired geological knowledge, field survey, and evidence-bound extraction.

use std::fmt::Write as _;

use super::view;
use deep_hearth::core::quantity::Mass;
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceConsistency, build_geological_knowledge_map,
    player_prospecting_options, validate_start_field_prospecting,
};
use deep_hearth::labor::ProspectingMethodId;
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::material::MaterialId;
use deep_hearth::mining::{
    MiningMethodId, MiningTargetRequest, resolve_mining_target, validate_start_mining,
};
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::{AppState, Registries};

fn region(state: &AppState, width: usize) -> Result<VoxelBounds, String> {
    if !(1..=4).contains(&width) {
        return Err("survey width must be 1-4 voxels".to_owned());
    }
    let player = state
        .logistics()
        .player()
        .ok_or("no admitted player".to_owned())?;
    let here = player.position();
    let width = i64::try_from(width).map_err(|_| "survey width overflow")?;
    let end = VoxelCoord::new(
        here.x()
            .checked_add(width)
            .ok_or("survey coordinate overflow")?,
        here.y()
            .checked_add(1)
            .ok_or("survey coordinate overflow")?,
        here.z()
            .checked_add(width)
            .ok_or("survey coordinate overflow")?,
    );
    VoxelBounds::new(here, end).map_err(|error| error.to_string())
}

fn material_name(registries: &Registries, id: MaterialId) -> &str {
    registries
        .materials()
        .get_material(id)
        .map_or("unidentified material", |definition| definition.name())
}

pub fn parse_material(registries: &Registries, text: &str) -> Result<MaterialId, String> {
    if let Ok(value) = text.parse::<u32>() {
        if value != 0 {
            let id = MaterialId::new(value);
            if registries.materials().get_material(id).is_some() {
                return Ok(id);
            }
        }
    }
    registries
        .materials()
        .definitions()
        .find(|definition| definition.name().eq_ignore_ascii_case(text))
        .map(|definition| definition.id())
        .ok_or_else(|| {
            format!("unknown geological material '{text}'; use an authored material name or ID")
        })
}

fn carried_equipment(
    registries: &Registries,
    state: &AppState,
    index: usize,
) -> Result<deep_hearth::equipment::EquipmentId, String> {
    project_player_inventory(registries, state)
        .map_err(|error| error.to_string())?
        .equipment()
        .get(index.checked_sub(1).ok_or("equipment indices begin at 1")?)
        .map(|item| item.equipment())
        .ok_or("no carried equipment at that index".to_owned())
}

pub fn catalog(registries: &Registries, state: &AppState, width: usize) -> Result<String, String> {
    let region = region(state, width)?;
    let methods =
        player_prospecting_options(registries, state, region).map_err(|error| error.to_string())?;
    let mut output = format!(
        "Field surveys for current {} x {} voxel region (prospect MATERIAL METHOD [width] [with TOOL]):\n",
        width, width,
    );
    for option in methods {
        let _ = write!(
            output,
            "  {}. {:?} | {} | {} slow ticks | +/- {} ppm",
            option.method().value(),
            option.evidence(),
            match option.spatial_resolution() {
                deep_hearth::labor::ProspectingSpatialResolution::PerVoxel => "per voxel",
                deep_hearth::labor::ProspectingSpatialResolution::AggregateRegion => "aggregate",
            },
            option.duration().value(),
            option.abundance_uncertainty_ppm(),
        );
        if option.requires_equipment() {
            let accepted = option
                .accepted_instrument_definitions()
                .iter()
                .filter_map(|id| registries.equipment().get_equipment(*id))
                .map(|definition| definition.name())
                .collect::<Vec<_>>()
                .join(" / ");
            let _ = write!(output, " | requires {accepted}");
            if option.instruments().is_empty() {
                output.push_str(" (none usable here)");
            } else {
                let inventory = project_player_inventory(registries, state)
                    .map_err(|error| error.to_string())?;
                for instrument in option.instruments() {
                    if let Some(index) = inventory
                        .equipment()
                        .iter()
                        .position(|item| item.equipment() == instrument.equipment())
                    {
                        let _ = write!(
                            output,
                            " | with {} (condition {}% -> {}%)",
                            index + 1,
                            instrument.condition().parts_per_million() / 10_000,
                            instrument.condition_after().parts_per_million() / 10_000,
                        );
                    }
                }
            }
        } else {
            output.push_str(" | by hand");
        }
        output.push('\n');
    }
    Ok(output)
}

pub fn start(
    registries: &Registries,
    state: &mut AppState,
    args: &[&str],
) -> Result<String, String> {
    if args.len() < 2 {
        return Err("usage: prospect MATERIAL METHOD [width] [with TOOL]".to_owned());
    }
    let material = parse_material(registries, args[0])?;
    let method = args[1]
        .parse::<u32>()
        .map_err(|_| "prospecting method must be a positive ID")?;
    if method == 0 {
        return Err("prospecting method IDs begin at 1".to_owned());
    }
    let mut width = 1;
    let mut instrument = None;
    let mut remaining = &args[2..];
    if let Some(first) = remaining.first()
        && *first != "with"
    {
        width = first
            .parse()
            .map_err(|_| "survey width must be an integer")?;
        remaining = &remaining[1..];
    }
    if !remaining.is_empty() {
        let ["with", text] = remaining else {
            return Err("usage: prospect MATERIAL METHOD [width] [with TOOL]".to_owned());
        };
        let index = text
            .parse::<usize>()
            .map_err(|_| "tool must be a numbered carried equipment item")?;
        instrument = Some(carried_equipment(registries, state, index)?);
    }
    let region = region(state, width)?;
    let method = ProspectingMethodId::new(method);
    let request = instrument.map_or_else(
        || FieldProspectingRequest::new(method, region, material),
        |id| FieldProspectingRequest::new_with_equipment(method, region, material, id),
    );
    let validated = validate_start_field_prospecting(registries, state, request)
        .map_err(|error| error.to_string())?;
    let duration = validated.work().completes_at().value() - state.tick().value();
    let evidence = registries
        .labor()
        .get_prospecting(method)
        .ok_or("unknown prospecting method")?
        .evidence();
    validated.commit(state).map_err(|error| error.to_string())?;
    Ok(format!(
        "Started {evidence:?} of {} across {} x {} voxels; scheduled for {} slow ticks. Use 'wait work', then 'knowledge'.\n",
        material_name(registries, material),
        width,
        width,
        duration,
    ))
}

pub fn knowledge(
    registries: &Registries,
    state: &AppState,
    width: usize,
) -> Result<String, String> {
    let region = region(state, width)?;
    let map = build_geological_knowledge_map(state.geological_knowledge(), region);
    let mut output = format!(
        "Acquired geological evidence in current {} x {} region:\n",
        width, width
    );
    if map.assessments().is_empty() {
        output.push_str("  (none acquired; survey by prospecting)\n");
        return Ok(output);
    }
    for assessment in map.assessments() {
        let _ = write!(
            output,
            "  {} | {} observations | ",
            material_name(registries, assessment.material()),
            assessment.observations().len(),
        );
        match assessment.consistency() {
            GeologicalEvidenceConsistency::NoEvidence => output.push_str("no evidence"),
            GeologicalEvidenceConsistency::SpatiallyIncomparable => {
                output.push_str("different survey footprints; no single shared locality")
            }
            GeologicalEvidenceConsistency::Conflicting { .. } => {
                output.push_str("conflicting measurements; inspect samples below")
            }
            GeologicalEvidenceConsistency::Compatible {
                lower_ppm,
                upper_ppm,
            } => {
                let _ = write!(output, "abundance {lower_ppm}..{upper_ppm} ppm");
            }
        }
        if let Some(hardness) = assessment.excavation_hardness() {
            let _ = write!(
                output,
                " | hardness {}..{} MPa",
                hardness.lower().pascals() / 1_000_000,
                hardness.upper().pascals() / 1_000_000
            );
        }
        if let Some(scale) = assessment.resource_mass() {
            let _ = write!(
                output,
                " | surveyed mass {}..{} g",
                view::grams(scale.lower()),
                view::grams(scale.upper())
            );
        }
        output.push('\n');
        // Preserve each original measured footprint without resampling hidden deposits.
        if width > 1 || assessment.observations().len() > 1 {
            append_observation_details(
                &mut output,
                state,
                assessment.material(),
                assessment.observations(),
            );
        }
    }
    output.push_str("Estimates are historical acquired measurements, not exact live reserves.\n");
    Ok(output)
}

fn append_observation_details(
    output: &mut String,
    state: &AppState,
    material: MaterialId,
    ids: &[deep_hearth::geology::GeologicalObservationId],
) {
    for observation in state.geological_knowledge().observations() {
        if !ids.contains(&observation.id()) {
            continue;
        }
        let Some(finding) = observation.finding(material) else {
            continue;
        };
        let bounds = observation.region();
        let start = bounds.min();
        let end = bounds.max_exclusive();
        let footprint =
            if end.x() == start.x() + 1 && end.y() == start.y() + 1 && end.z() == start.z() + 1 {
                format!("voxel ({},{},{})", start.x(), start.y(), start.z())
            } else {
                format!(
                    "region ({},{},{}) to ({},{},{}) exclusive",
                    start.x(),
                    start.y(),
                    start.z(),
                    end.x(),
                    end.y(),
                    end.z(),
                )
            };
        let _ = write!(
            output,
            "    Sample #{}: {footprint} | {:?} | {}..{} ppm",
            observation.id().value(),
            observation.evidence(),
            finding.lower_ppm(),
            finding.upper_ppm(),
        );
        if let Some(hardness) = observation.excavation_hardness() {
            let _ = write!(
                output,
                " | hardness {}..{} MPa",
                hardness.lower().pascals() / 1_000_000,
                hardness.upper().pascals() / 1_000_000,
            );
        }
        if let Some(resource) = observation.resource_mass() {
            let _ = write!(
                output,
                " | sampled mass {}..{} g",
                super::view::grams(resource.lower()),
                super::view::grams(resource.upper()),
            );
        }
        let _ = writeln!(
            output,
            " | observed at tick {}",
            observation.observed_at().value()
        );
    }
}

pub fn mine(
    registries: &Registries,
    state: &mut AppState,
    args: &[&str],
) -> Result<String, String> {
    let [material, grams, "with", equipment, ..] = args else {
        return Err("usage: mine MATERIAL GRAMS with TOOL [using METHOD]".to_owned());
    };
    if !matches!(args.len(), 4 | 6) || (args.len() == 6 && args[4] != "using") {
        return Err("usage: mine MATERIAL GRAMS with TOOL [using METHOD]".to_owned());
    }
    let material = parse_material(registries, material)?;
    let grams = grams
        .parse::<u64>()
        .map_err(|_| "mining mass must be whole grams")?;
    let milligrams = grams.checked_mul(1_000).ok_or("mining mass overflow")?;
    if milligrams == 0 {
        return Err("mining mass must be positive".to_owned());
    }
    let equipment_index = equipment
        .parse::<usize>()
        .map_err(|_| "mining tool must be a numbered carried equipment item")?;
    let tool = carried_equipment(registries, state, equipment_index)?;
    let method = if args.len() == 6 {
        let id = args[5]
            .parse::<u32>()
            .map_err(|_| "mining method must be a positive ID")?;
        if id == 0 {
            return Err("mining method IDs begin at 1".to_owned());
        }
        MiningMethodId::new(id)
    } else {
        deep_hearth::content::MINING_METHOD_HAND_PICK
    };
    let target =
        resolve_mining_target(state, MiningTargetRequest::new(region(state, 1)?, material))
            .map_err(|error| error.to_string())?;
    let destination = state
        .logistics()
        .player()
        .ok_or("no admitted player")?
        .carried_stockpile();
    let work = validate_start_mining(
        registries,
        state,
        method,
        target,
        destination,
        tool,
        Mass::from_milligrams(milligrams),
    )
    .map_err(|error| error.to_string())?;
    let job = work.commit(state).map_err(|error| error.to_string())?;
    Ok(format!(
        "Started {} g {} extraction (job {:?}) with carried tool {}. Use 'wait work'.\n",
        grams,
        material_name(registries, material),
        job,
        equipment_index,
    ))
}
