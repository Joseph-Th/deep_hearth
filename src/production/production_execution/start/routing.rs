//! Output-route, storage, capacity-input, and structural-support admission for process starts.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    StockpileId, validate_stockpile_storage, validate_stockpile_support_for_new_inbound,
};
use crate::material::MaterialLotSpec;
use crate::production::resolution::{ProcessResolution, sum_lot_spec_mass};
use crate::production::state::ProductionOutputStream;
use crate::registry::Registries;

use super::{ProcessOutputRoute, StartProcessError};

#[must_use]
pub(super) struct ValidatedOutputRouting {
    pub(super) output_streams: Vec<ProductionOutputStream>,
    pub(super) inbound_by_destination: BTreeMap<StockpileId, Mass>,
    pub(super) destination_structure_revision: Option<u64>,
}

fn bind_output_routes(
    resolution: &ProcessResolution,
    routes: &[ProcessOutputRoute],
) -> Result<BTreeMap<crate::production::ProcessOutputStreamId, StockpileId>, StartProcessError> {
    if routes.len() != resolution.output_streams().len() {
        return Err(StartProcessError::OutputRouteCountMismatch {
            streams: resolution.output_streams().len(),
            routes: routes.len(),
        });
    }
    let stream_ids = resolution
        .output_streams()
        .iter()
        .map(|stream| stream.id())
        .collect::<BTreeSet<_>>();
    let mut destinations_by_stream = BTreeMap::new();
    for route in routes {
        if !stream_ids.contains(&route.stream()) {
            return Err(StartProcessError::UnknownOutputRoute {
                stream: route.stream(),
            });
        }
        if destinations_by_stream
            .insert(route.stream(), route.destination())
            .is_some()
        {
            return Err(StartProcessError::DuplicateOutputRoute {
                stream: route.stream(),
            });
        }
    }
    Ok(destinations_by_stream)
}

fn validate_output_material(
    registries: &Registries,
    output: &MaterialLotSpec,
) -> Result<(), StartProcessError> {
    if registries
        .materials()
        .get_material(output.commodity().material())
        .is_none()
    {
        return Err(StartProcessError::UnknownOutputMaterial {
            material: output.commodity().material(),
        });
    }
    if registries
        .materials()
        .get_form(output.commodity().form())
        .is_none()
    {
        return Err(StartProcessError::UnknownOutputForm {
            form: output.commodity().form(),
        });
    }
    for component in output.composition().components() {
        if registries
            .materials()
            .get_material(component.material())
            .is_none()
        {
            return Err(StartProcessError::UnknownOutputCompositionMaterial {
                material: component.material(),
            });
        }
    }
    Ok(())
}

fn validate_output_materials(
    registries: &Registries,
    resolution: &ProcessResolution,
) -> Result<(), StartProcessError> {
    for stream in resolution.output_streams() {
        for output in stream.outputs() {
            validate_output_material(registries, output)?;
        }
    }
    Ok(())
}

fn build_routed_output_streams(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    destinations_by_stream: &BTreeMap<crate::production::ProcessOutputStreamId, StockpileId>,
) -> Result<(Vec<ProductionOutputStream>, BTreeMap<StockpileId, Mass>), StartProcessError> {
    let mut inbound_by_destination = BTreeMap::<StockpileId, Mass>::new();
    let mut output_streams = Vec::with_capacity(resolution.output_streams().len());
    for stream in resolution.output_streams() {
        let destination = destinations_by_stream.get(&stream.id()).copied().ok_or(
            StartProcessError::MissingOutputRoute {
                stream: stream.id(),
            },
        )?;
        let Some(destination_record) = state.inventory().get_stockpile(destination) else {
            return Err(StartProcessError::UnknownStockpile {
                stockpile: destination,
            });
        };
        for output in stream.outputs() {
            validate_stockpile_storage(
                registries,
                destination_record,
                destination,
                output.commodity(),
                output.composition(),
                output.temperature(),
                output.particle_size_distribution(),
            )
            .map_err(StartProcessError::DestinationStorage)?;
        }
        let stream_mass = sum_lot_spec_mass(stream.outputs())
            .unwrap_or_else(|| panic!("resolved process stream mass overflowed after validation"));
        let current = inbound_by_destination
            .get(&destination)
            .copied()
            .unwrap_or(Mass::ZERO);
        let inbound = current
            .checked_add(stream_mass)
            .ok_or(StartProcessError::MassOverflow {
                stockpile: destination,
            })?;
        inbound_by_destination.insert(destination, inbound);
        output_streams.push(ProductionOutputStream {
            id: stream.id(),
            destination,
            outputs: stream.outputs().to_vec(),
        });
    }
    Ok((output_streams, inbound_by_destination))
}

fn validate_output_destination_supports(
    state: &AppState,
    inbound_by_destination: &BTreeMap<StockpileId, Mass>,
) -> Result<Option<u64>, StartProcessError> {
    let mut destination_structure_revision = None;
    for destination in inbound_by_destination.keys().copied() {
        if let Some(revision) = validate_stockpile_support_for_new_inbound(state, destination)
            .map_err(StartProcessError::StructuralLoad)?
        {
            if let Some(existing) = destination_structure_revision {
                assert_eq!(
                    existing, revision,
                    "one process start must bind every output support check to one structure revision"
                );
            } else {
                destination_structure_revision = Some(revision);
            }
        }
    }
    Ok(destination_structure_revision)
}

pub(super) fn validate_output_routing(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    routes: &[ProcessOutputRoute],
) -> Result<ValidatedOutputRouting, StartProcessError> {
    let destinations_by_stream = bind_output_routes(resolution, routes)?;

    validate_output_materials(registries, resolution)?;
    let (output_streams, inbound_by_destination) =
        build_routed_output_streams(registries, state, resolution, &destinations_by_stream)?;
    let destination_structure_revision =
        validate_output_destination_supports(state, &inbound_by_destination)?;

    Ok(ValidatedOutputRouting {
        output_streams,
        inbound_by_destination,
        destination_structure_revision,
    })
}
