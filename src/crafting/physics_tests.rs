//! Shared manual-craft schedule physics regressions.

use super::*;
use crate::capability::{CapabilityProfile, CapabilityValue, CapabilityValueKind};
use crate::core::quantity::MassFlow;
use crate::equipment::{EquipmentDefinitionId, resolve_equipment_mass_flow_schedule};
use crate::maintenance::MaintenanceThresholds;

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(998_001);
const TEST_EQUIPMENT: EquipmentDefinitionId = EquipmentDefinitionId::new(998_001);

fn condition(parts_per_million: u32) -> Condition {
    Condition::new(parts_per_million)
        .unwrap_or_else(|error| panic!("condition fixture failed: {error}"))
}

fn equipment_with_capability(value: Option<CapabilityValue>) -> EquipmentDefinition {
    let capabilities =
        CapabilityProfile::new(value.into_iter().map(|value| (TEST_CAPABILITY, value)))
            .unwrap_or_else(|error| panic!("capability fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(condition(600_000), condition(250_000))
        .unwrap_or_else(|error| panic!("maintenance fixture failed: {error}"));
    EquipmentDefinition::new(
        TEST_EQUIPMENT,
        "manual craft physics fixture",
        Mass::from_milligrams(1),
        capabilities,
        thresholds,
    )
}

#[test]
fn equipment_schedule_binds_throughput_duration_and_wear_once() {
    let provider = equipment_with_capability(Some(CapabilityValue::MassFlow(
        MassFlow::from_milligrams_per_second(10),
    )));
    let schedule = resolve_equipment_mass_flow_schedule(
        &provider,
        Condition::PRISTINE,
        TEST_CAPABILITY,
        Mass::from_milligrams(100),
        PhysicalTickDuration::from_microseconds(1_000_000),
        100_000,
    )
    .unwrap_or_else(|error| panic!("bounded craft schedule failed: {error:?}"));

    assert_eq!(schedule.rate(), MassFlow::from_milligrams_per_second(10));
    assert_eq!(schedule.duration(), TickSpan::new(10));
    assert_eq!(schedule.condition_after(), Condition::FAILED);
}

#[test]
fn equipment_physics_owns_capability_resolution_before_scheduling() {
    let missing = equipment_with_capability(None);
    assert_eq!(
        resolve_manual_craft_equipment_physics(
            &missing,
            Condition::PRISTINE,
            TEST_CAPABILITY,
            Mass::from_milligrams(100),
            PhysicalTickDuration::from_microseconds(1_000_000),
            100_000,
        ),
        Err(ManualCraftEquipmentResolutionError::MissingCapability {
            capability: TEST_CAPABILITY,
        })
    );

    let wrong_kind =
        equipment_with_capability(Some(CapabilityValue::Mass(Mass::from_milligrams(10))));
    assert_eq!(
        resolve_manual_craft_equipment_physics(
            &wrong_kind,
            Condition::PRISTINE,
            TEST_CAPABILITY,
            Mass::from_milligrams(100),
            PhysicalTickDuration::from_microseconds(1_000_000),
            100_000,
        ),
        Err(
            ManualCraftEquipmentResolutionError::CapabilityKindMismatch {
                capability: TEST_CAPABILITY,
                found: CapabilityValueKind::Mass,
            }
        )
    );

    let provider = equipment_with_capability(Some(CapabilityValue::MassFlow(
        MassFlow::from_milligrams_per_second(10),
    )));
    let schedule = resolve_manual_craft_equipment_physics(
        &provider,
        Condition::PRISTINE,
        TEST_CAPABILITY,
        Mass::from_milligrams(100),
        PhysicalTickDuration::from_microseconds(1_000_000),
        100_000,
    )
    .unwrap_or_else(|error| panic!("shared equipment physics failed: {error:?}"));
    assert_eq!(schedule.duration(), TickSpan::new(10));
    assert_eq!(schedule.condition_after(), Condition::FAILED);
}

#[test]
fn hand_duration_scales_exact_integral_batches() {
    let batches = NonZeroU64::new(3).unwrap_or_else(|| unreachable!("three is nonzero"));
    assert_eq!(
        resolve_manual_craft_hand_duration(TickSpan::new(7), batches),
        Some(TickSpan::new(21))
    );
}
