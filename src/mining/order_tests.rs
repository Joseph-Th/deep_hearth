//! Order projections must agree with sequential canonical batch physics, not nominal flow estimates.

use super::*;
use crate::capability::CapabilityValue;
use crate::content::{EQUIPMENT_STONE_QUARRY_PICK, MINING_METHOD_HAND_PICK, build_registries};

fn constant_equipment(
    method: &MiningMethodDefinition,
    flow: CapabilityValue,
    capacity: Mass,
) -> EquipmentDefinition {
    use crate::capability::CapabilityProfile;
    let registries = build_registries();
    let authored = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_QUARRY_PICK)
        .unwrap_or_else(|| panic!("quarry definition missing"));
    EquipmentDefinition::new(
        authored.id(),
        "constant physics fixture",
        authored.mass(),
        CapabilityProfile::new([
            (method.mass_flow_capability(), flow),
            (
                method.max_batch_mass_capability(),
                CapabilityValue::Mass(capacity),
            ),
            (
                method.max_hardness_capability(),
                CapabilityValue::Pressure(Pressure::from_pascals(10)),
            ),
        ])
        .unwrap_or_else(|error| panic!("fixture profile invalid: {error}")),
        authored.maintenance_thresholds(),
    )
}

#[test]
fn mining_order_projection_rejects_zero_and_excessive_work_before_physics() {
    let registries = build_registries();
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("mining method missing"));
    let equipment = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_QUARRY_PICK)
        .unwrap_or_else(|| panic!("quarry definition missing"));
    let one = Mass::from_milligrams(1);
    let maximum = Mass::from_milligrams(u64::MAX);
    for (mass, batch, bound, expected) in [
        (Mass::ZERO, one, 1, MiningOrderError::ZeroRequestedMass),
        (one, Mass::ZERO, 1, MiningOrderError::ZeroBatchMass),
        (
            one,
            one,
            0,
            MiningOrderError::BatchLimitExceeded {
                required: 1,
                maximum: 0,
            },
        ),
        (
            maximum,
            one,
            10,
            MiningOrderError::BatchLimitExceeded {
                required: u64::MAX,
                maximum: 10,
            },
        ),
        (
            maximum,
            Mass::from_milligrams(u64::MAX - 1),
            1,
            MiningOrderError::BatchLimitExceeded {
                required: 2,
                maximum: 1,
            },
        ),
    ] {
        assert_eq!(
            resolve_mining_order(
                registries.core().physical_tick_duration(),
                method,
                equipment,
                MiningOrderRequest::new(Condition::FAILED, Pressure::ZERO, mass, batch, bound)
            ),
            Err(expected)
        );
    }
}

#[test]
fn mining_order_projection_preserves_capacity_hardness_and_lifetime_failures() {
    use crate::core::quantity::MassFlow;
    let registries = build_registries();
    let authored = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("mining method missing"));
    let method = MiningMethodDefinition::new(
        authored.id(),
        "four-tick lifetime fixture",
        authored.mass_flow_capability(),
        authored.max_batch_mass_capability(),
        authored.max_hardness_capability(),
        250_000,
        authored.exertion(),
    );
    let batch = Mass::from_milligrams(2);
    let equipment = constant_equipment(
        &method,
        CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
        batch,
    );
    let tick = PhysicalTickDuration::from_microseconds(1_000_000);
    let hardness = Pressure::from_pascals(10);
    let resolve = |before, hardness, mass, batch| {
        resolve_mining_order(
            tick,
            &method,
            &equipment,
            MiningOrderRequest::new(before, hardness, mass, batch, 3),
        )
    };
    let final_useful_tick = resolve(
        Condition::PRISTINE,
        hardness,
        Mass::from_milligrams(4),
        batch,
    )
    .unwrap_or_else(|error| panic!("exact lifetime should succeed: {error}"));
    assert_eq!(final_useful_tick.duration(), TickSpan::new(4));
    assert_eq!(final_useful_tick.condition_after(), Condition::FAILED);
    assert_eq!(final_useful_tick.batches(), 2);
    // A stale pristine assumption is not carried into a new projection from worn condition.
    let worn = Condition::new(250_000).unwrap_or_else(|error| panic!("condition invalid: {error}"));
    for (before, resistance, mass, selected) in [
        (
            Condition::PRISTINE,
            hardness,
            Mass::from_milligrams(3),
            Mass::from_milligrams(3),
        ),
        (
            Condition::PRISTINE,
            Pressure::from_pascals(11),
            batch,
            batch,
        ),
        (worn, hardness, batch, batch),
        (Condition::FAILED, hardness, batch, batch),
    ] {
        let error = resolve_mining_physics(tick, &method, &equipment, before, resistance, mass)
            .err()
            .unwrap_or_else(|| panic!("canonical batch should reject fixture"));
        assert_eq!(
            resolve(before, resistance, mass, selected),
            Err(MiningOrderError::Physics { batch: 1, error })
        );
    }
    assert_eq!(
        resolve(
            Condition::PRISTINE,
            hardness,
            Mass::from_milligrams(5),
            batch
        ),
        Err(MiningOrderError::Physics {
            batch: 3,
            error: MiningPhysicsError::MissingCapability {
                capability: method.mass_flow_capability(),
            }
        })
    );
    // A selected batch larger than capacity is irrelevant when the entire short order fits.
    let short = resolve(
        Condition::PRISTINE,
        hardness,
        Mass::from_milligrams(1),
        Mass::from_milligrams(3),
    )
    .unwrap_or_else(|error| panic!("short order should fit: {error}"));
    assert_eq!(short.duration(), TickSpan::new(1));
    assert_eq!(short.batches(), 1);
}

#[test]
fn mining_order_projection_preserves_physics_overflow_and_capability_errors() {
    use crate::core::quantity::MassFlow;
    use crate::core::throughput::MassFlowDurationError;
    let registries = build_registries();
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("mining method missing"));
    let mass = Mass::from_milligrams(u64::MAX);
    let tick = PhysicalTickDuration::from_microseconds(1);
    for (flow, expected) in [
        (
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            MiningPhysicsError::Duration(MassFlowDurationError::TickRangeExceeded),
        ),
        (
            CapabilityValue::MassFlow(MassFlow::ZERO),
            MiningPhysicsError::ZeroThroughput,
        ),
        (
            CapabilityValue::Mass(Mass::from_milligrams(1)),
            MiningPhysicsError::CapabilityKindMismatch {
                capability: method.mass_flow_capability(),
                expected: crate::capability::CapabilityValueKind::MassFlow,
                found: crate::capability::CapabilityValueKind::Mass,
            },
        ),
    ] {
        let equipment = constant_equipment(method, flow, mass);
        let result = resolve_mining_order(
            tick,
            method,
            &equipment,
            MiningOrderRequest::new(
                Condition::PRISTINE,
                Pressure::from_pascals(10),
                mass,
                mass,
                1,
            ),
        );
        assert_eq!(
            result,
            Err(MiningOrderError::Physics {
                batch: 1,
                error: expected
            })
        );
    }
}

#[test]
fn mining_order_projection_matches_sequential_physics_with_wear_and_remainders() {
    let registries = build_registries();
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("mining method missing"));
    let equipment = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_QUARRY_PICK)
        .unwrap_or_else(|| panic!("quarry definition missing"));
    let CapabilityValue::Mass(batch) = equipment
        .capabilities()
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("batch capability missing"))
    else {
        panic!("batch kind changed")
    };
    let CapabilityValue::Pressure(hardness) = equipment
        .capabilities()
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| panic!("hardness capability missing"))
    else {
        panic!("hardness kind changed")
    };
    let tick = registries.core().physical_tick_duration();
    let half = Mass::from_milligrams(batch.milligrams() / 2);
    for before in [
        Condition::PRISTINE,
        Condition::new(750_000).unwrap_or_else(|error| panic!("condition failed: {error}")),
    ] {
        for masses in [vec![half], vec![batch], vec![batch; 40], {
            let mut masses = vec![batch; 40];
            masses.push(half);
            masses
        }] {
            let mut condition = before;
            let mut ticks = 0;
            let mut pristine_ticks = 0;
            let mut total = Mass::ZERO;
            for &mass in &masses {
                let physics =
                    resolve_mining_physics(tick, method, equipment, condition, hardness, mass)
                        .unwrap_or_else(|error| panic!("canonical physics failed: {error}"));
                condition = physics.condition_after();
                ticks += physics.duration().value();
                pristine_ticks +=
                    resolve_mining_physics(tick, method, equipment, before, hardness, mass)
                        .unwrap_or_else(|error| panic!("initial-condition physics failed: {error}"))
                        .duration()
                        .value();
                total = total
                    .checked_add(mass)
                    .unwrap_or_else(|| panic!("test order overflow"));
            }
            let projection = resolve_mining_order(
                tick,
                method,
                equipment,
                MiningOrderRequest::new(before, hardness, total, batch, masses.len() as u64),
            )
            .unwrap_or_else(|error| panic!("projection failed: {error}"));
            assert_eq!(projection.duration(), TickSpan::new(ticks));
            assert_eq!(projection.condition_after(), condition);
            assert_eq!(projection.batches(), masses.len() as u64);
            assert!(ticks >= pristine_ticks);
            if masses.len() > 2 && before == Condition::PRISTINE {
                assert!(
                    ticks > pristine_ticks,
                    "long orders must account for throughput lost to wear"
                );
            }
        }
    }
}
