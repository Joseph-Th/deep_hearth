//! Boundary contracts for casting mass-envelope constraint reporting.

use super::*;

fn envelope(
    offered: u64,
    equipment: u64,
    transfer: u64,
    condition: u64,
    maximum: u64,
) -> CastingLotMassEnvelope {
    CastingLotMassEnvelope {
        offered_mass: Mass::from_milligrams(offered),
        equipment_capacity: Mass::from_milligrams(equipment),
        transfer_energy_capacity: Mass::from_milligrams(transfer),
        condition_lifetime_capacity: Mass::from_milligrams(condition),
        maximum_mass: Mass::from_milligrams(maximum),
    }
}

#[test]
fn exact_casting_capacity_boundaries_are_inclusive() {
    assert_eq!(envelope(10, 10, 20, 20, 10).limiting_constraint(), None);
    assert_eq!(envelope(10, 20, 10, 20, 10).limiting_constraint(), None);
    assert_eq!(envelope(10, 20, 20, 10, 10).limiting_constraint(), None);
    assert_eq!(envelope(10, 20, 20, 20, 10).limiting_constraint(), None);
}

#[test]
fn casting_constraint_reporting_matches_canonical_admission_order() {
    assert_eq!(
        envelope(11, 10, 20, 20, 10).limiting_constraint(),
        Some(CastingLotMassConstraint::EquipmentCapacity)
    );
    assert_eq!(
        envelope(11, 20, 10, 20, 10).limiting_constraint(),
        Some(CastingLotMassConstraint::TransferEnergyRange)
    );
    assert_eq!(
        envelope(11, 20, 20, 10, 10).limiting_constraint(),
        Some(CastingLotMassConstraint::ConditionLifetime)
    );
    assert_eq!(
        envelope(11, 20, 20, 20, 10).limiting_constraint(),
        Some(CastingLotMassConstraint::ThermalSinkCapacity)
    );
    assert_eq!(
        envelope(11, 10, 10, 10, 10).limiting_constraint(),
        Some(CastingLotMassConstraint::EquipmentCapacity)
    );
}
