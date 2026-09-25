//! Proves fixed-duration player work fits within full authored survival reserves.

use crate::maintenance::Condition;
use crate::survival::PhysiologyDefinition;

use super::super::super::RegistryDomains;
use super::assert_player_work_fits_reserves;

pub(super) fn validate_fixed_player_work_operability(
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
) {
    for definition in domains.labor.prospecting_definitions() {
        assert_player_work_fits_reserves(
            physiology,
            definition.exertion(),
            definition.duration(),
            "prospecting method",
            u64::from(definition.id().value()),
        );
    }

    for definition in domains.equipment.definitions() {
        let Some(maintenance) = definition.maintenance_profile() else {
            continue;
        };
        assert_player_work_fits_reserves(
            physiology,
            maintenance.exertion(),
            maintenance.required_service_duration(Condition::FAILED),
            "equipment full service",
            u64::from(definition.id().value()),
        );
    }

    for definition in domains.storage.definitions() {
        assert_player_work_fits_reserves(
            physiology,
            definition.dismantle_exertion(),
            definition.dismantle_duration(),
            "storage dismantling",
            u64::from(definition.id().value()),
        );
    }
}
