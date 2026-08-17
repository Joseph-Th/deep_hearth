//! Cross-owner production validation; this child reconciles job traces, reservations, and runtime
//! resources.

use super::*;

pub(super) fn validate_production_references(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    let mut expected_reservations = BTreeMap::<StockpileId, Mass>::new();
    let mut occupied_energy = BTreeMap::<crate::energy::EnergyStoreId, ProductionJobId>::new();
    let mut occupied_equipment = BTreeMap::<EquipmentId, ProductionJobId>::new();
    for job in state.systems.production.jobs() {
        if registries.production().get_process(job.process()).is_none() {
            return Err(StateValidationError::UnknownJobProcess {
                job: job.id(),
                process: job.process(),
            });
        }
        if state
            .systems
            .inventory
            .get_stockpile(job.source())
            .is_none()
        {
            return Err(StateValidationError::UnknownJobSource {
                job: job.id(),
                stockpile: job.source(),
            });
        }
        if let Some(trace) = job.consumed_energy() {
            let Some(store) = state.systems.energy.get_store(trace.source()) else {
                return Err(StateValidationError::UnknownJobEnergySource {
                    job: job.id(),
                    store: trace.source(),
                });
            };
            if store.definition() != trace.definition() {
                return Err(StateValidationError::JobEnergyDefinitionMismatch {
                    job: job.id(),
                    traced: trace.definition(),
                    stored: store.definition(),
                });
            }
            let Some(definition) = registries.energy().get_store(trace.definition()) else {
                return Err(StateValidationError::Energy(
                    EnergyValidationError::UnknownDefinition {
                        store: trace.source(),
                        definition: trace.definition(),
                    },
                ));
            };
            if definition.carrier() != trace.carrier() {
                return Err(StateValidationError::JobEnergyCarrierMismatch {
                    job: job.id(),
                    traced: trace.carrier(),
                    authored: definition.carrier(),
                });
            }
            if let Some(first) = occupied_energy.insert(trace.source(), job.id()) {
                return Err(StateValidationError::EnergyStoreDoubleBooked {
                    store: trace.source(),
                    first,
                    second: job.id(),
                });
            }
        }
        if let Some(trace) = job.released_energy() {
            let Some(store) = state.systems.energy.get_store(trace.destination()) else {
                return Err(StateValidationError::UnknownJobEnergySink {
                    job: job.id(),
                    store: trace.destination(),
                });
            };
            if store.definition() != trace.definition() {
                return Err(StateValidationError::JobReleasedEnergyDefinitionMismatch {
                    job: job.id(),
                    traced: trace.definition(),
                    stored: store.definition(),
                });
            }
            let Some(definition) = registries.energy().get_store(trace.definition()) else {
                return Err(StateValidationError::Energy(
                    EnergyValidationError::UnknownDefinition {
                        store: trace.destination(),
                        definition: trace.definition(),
                    },
                ));
            };
            if definition.carrier() != trace.carrier() {
                return Err(StateValidationError::JobReleasedEnergyCarrierMismatch {
                    job: job.id(),
                    traced: trace.carrier(),
                    authored: definition.carrier(),
                });
            }
            if definition.max_input_power().is_zero() {
                return Err(StateValidationError::JobReleasedEnergySinkHasNoInputPower {
                    job: job.id(),
                    store: trace.destination(),
                });
            }
            let after = store.stored().checked_add(trace.energy()).ok_or(
                StateValidationError::JobReleasedEnergyCapacityOverflow {
                    job: job.id(),
                    store: trace.destination(),
                },
            )?;
            if after > definition.capacity() {
                return Err(StateValidationError::JobReleasedEnergyCapacityExceeded {
                    job: job.id(),
                    store: trace.destination(),
                    stored: store.stored(),
                    released: trace.energy(),
                    capacity: definition.capacity(),
                });
            }
            if let Some(first) = occupied_energy.insert(trace.destination(), job.id()) {
                return Err(StateValidationError::EnergyStoreDoubleBooked {
                    store: trace.destination(),
                    first,
                    second: job.id(),
                });
            }
        }
        if let Some(provider) = job.equipment_provider() {
            let Some(record) = state.systems.equipment.get_equipment(provider.equipment()) else {
                return Err(StateValidationError::UnknownJobEquipment {
                    job: job.id(),
                    equipment: provider.equipment(),
                });
            };
            if record.definition() != provider.definition() {
                return Err(StateValidationError::JobEquipmentDefinitionMismatch {
                    job: job.id(),
                    traced: provider.definition(),
                    stored: record.definition(),
                });
            }
            if record.condition() != provider.condition() {
                return Err(StateValidationError::JobEquipmentConditionMismatch {
                    job: job.id(),
                    traced: provider.condition(),
                    stored: record.condition(),
                });
            }
            if let Some(first) = occupied_equipment.insert(provider.equipment(), job.id()) {
                return Err(StateValidationError::EquipmentDoubleBooked {
                    equipment: provider.equipment(),
                    first,
                    second: job.id(),
                });
            }
        }
        validate_loaded_comminution_job(registries, job)
            .map_err(StateValidationError::ComminutionJob)?;
        validate_loaded_screening_job(registries, job)
            .map_err(StateValidationError::ScreeningJob)?;
        validate_loaded_thermal_job(registries, job).map_err(StateValidationError::ThermalJob)?;
        if let Some(suspension) = job.suspension() {
            if suspension.suspended_at() > state.tick() {
                return Err(StateValidationError::JobSuspendedInFuture {
                    job: job.id(),
                    current: state.tick(),
                    suspended_at: suspension.suspended_at(),
                });
            }
        } else if job.completes_at() <= state.tick() {
            return Err(StateValidationError::JobAlreadyDue {
                job: job.id(),
                current: state.tick(),
                due: job.completes_at(),
            });
        }

        for trace in job.consumed_inputs() {
            let commodity = trace.profile().commodity();
            if !registries.materials().has_commodity(commodity) {
                return Err(StateValidationError::UnknownJobConsumedCommodity {
                    job: job.id(),
                    commodity,
                });
            }
            for component in trace.profile().composition().components() {
                if registries
                    .materials()
                    .get_material(component.material())
                    .is_none()
                {
                    return Err(
                        StateValidationError::UnknownJobConsumedCompositionMaterial {
                            job: job.id(),
                            material: component.material(),
                        },
                    );
                }
            }
            validate_material_particle_size_state(
                registries.materials(),
                commodity,
                trace.profile().particle_size_distribution(),
            )
            .map_err(|error| {
                StateValidationError::InvalidJobConsumedParticleSizeState {
                    job: job.id(),
                    error,
                }
            })?;
        }

        for stream in job.output_streams() {
            let destination = stream.destination();
            let Some(destination_record) = state.systems.inventory.get_stockpile(destination)
            else {
                return Err(StateValidationError::UnknownJobDestination {
                    job: job.id(),
                    stockpile: destination,
                });
            };
            for output in stream.outputs() {
                if !registries.materials().has_commodity(output.commodity()) {
                    return Err(StateValidationError::UnknownJobOutputCommodity {
                        job: job.id(),
                        commodity: output.commodity(),
                    });
                }
                for component in output.composition().components() {
                    if registries
                        .materials()
                        .get_material(component.material())
                        .is_none()
                    {
                        return Err(StateValidationError::UnknownJobOutputCompositionMaterial {
                            job: job.id(),
                            material: component.material(),
                        });
                    }
                }
                validate_stockpile_storage(
                    registries,
                    destination_record,
                    destination,
                    output.commodity(),
                    output.composition(),
                    output.temperature(),
                    output.particle_size_distribution(),
                )
                .map_err(|error| StateValidationError::JobOutputStorage {
                    job: job.id(),
                    error,
                })?;
            }
            let output_mass = sum_lot_spec_mass(stream.outputs())
                .ok_or(StateValidationError::JobOutputMassOverflow { job: job.id() })?;
            let current = expected_reservations
                .get(&destination)
                .copied()
                .unwrap_or(Mass::ZERO);
            let expected = current.checked_add(output_mass).ok_or(
                StateValidationError::ReservedMassOverflow {
                    stockpile: destination,
                },
            )?;
            expected_reservations.insert(destination, expected);
        }
    }

    for stockpile in state.systems.inventory.stockpiles() {
        let expected = expected_reservations
            .get(&stockpile.id())
            .copied()
            .unwrap_or(Mass::ZERO);
        if stockpile.reserved_inbound() != expected {
            return Err(StateValidationError::ReservedInboundMismatch {
                stockpile: stockpile.id(),
                reserved: stockpile.reserved_inbound(),
                expected,
            });
        }
    }

    Ok(())
}
