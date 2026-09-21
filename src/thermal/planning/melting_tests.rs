//! Boundary contracts for melting mass-envelope constraint reporting.

use super::*;

fn envelope(
    offered: u64,
    equipment: u64,
    transfer: u64,
    finite_energy: u64,
    condition: u64,
) -> MeltingLotMassEnvelope {
    MeltingLotMassEnvelope {
        offered_mass: Mass::from_milligrams(offered),
        equipment_capacity: Mass::from_milligrams(equipment),
        transfer_energy_capacity: Mass::from_milligrams(transfer),
        finite_energy_capacity: Mass::from_milligrams(finite_energy),
        condition_lifetime_capacity: Mass::from_milligrams(condition),
    }
}

#[test]
fn exact_melting_capacity_boundaries_are_inclusive() {
    assert_eq!(envelope(10, 10, 20, 20, 20).limiting_constraint(), None);
    assert_eq!(envelope(10, 20, 10, 20, 20).limiting_constraint(), None);
    assert_eq!(envelope(10, 20, 20, 10, 20).limiting_constraint(), None);
    assert_eq!(envelope(10, 20, 20, 20, 10).limiting_constraint(), None);
    assert_eq!(
        envelope(10, 10, 10, 10, 10).maximum_mass(),
        Mass::from_milligrams(10)
    );
}

#[test]
fn melting_constraint_reporting_matches_canonical_admission_order() {
    assert_eq!(
        envelope(11, 10, 20, 20, 20).limiting_constraint(),
        Some(MeltingLotMassConstraint::EquipmentCapacity)
    );
    assert_eq!(
        envelope(11, 20, 10, 20, 20).limiting_constraint(),
        Some(MeltingLotMassConstraint::TransferEnergyRange)
    );
    assert_eq!(
        envelope(11, 20, 20, 10, 20).limiting_constraint(),
        Some(MeltingLotMassConstraint::FiniteEnergy)
    );
    assert_eq!(
        envelope(11, 20, 20, 20, 10).limiting_constraint(),
        Some(MeltingLotMassConstraint::ConditionLifetime)
    );
    assert_eq!(
        envelope(11, 10, 10, 10, 10).limiting_constraint(),
        Some(MeltingLotMassConstraint::EquipmentCapacity)
    );
}
