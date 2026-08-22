//! Tests for the sibling power physics module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn zero_required_output_has_zero_metabolic_duration() {
    assert_eq!(
        calculate_metabolic_duration(Energy::ZERO, Energy::from_nanojoules(1)),
        Ok(TickSpan::ZERO)
    );
}

#[test]
fn full_width_manual_power_scaling_does_not_reject_representable_effort() {
    let maximum = SurvivalExertion::new(
        Energy::from_nanojoules(u128::MAX),
        Volume::from_microliters(u64::MAX),
    );

    assert_eq!(
        metabolic_output_per_tick(maximum.energy_cost_per_tick(), PARTS_PER_MILLION),
        Energy::from_nanojoules(u128::MAX)
    );
    assert_eq!(
        resolve_manual_power_exertion(
            Energy::from_nanojoules(u128::MAX),
            TickSpan::new(1),
            maximum,
            PARTS_PER_MILLION,
        ),
        Ok(maximum)
    );
}

#[test]
fn bottlenecked_manual_power_scales_effort_to_actual_output() {
    let maximum = SurvivalExertion::new(
        Energy::from_nanojoules(1_500_000_000_000),
        Volume::from_microliters(350),
    );
    let output = Energy::from_nanojoules(25_000_000_000);

    let slow = resolve_manual_power_exertion(output, TickSpan::new(10), maximum, 200_000)
        .unwrap_or_else(|error| panic!("slow manual-power effort failed: {error:?}"));
    let fast = resolve_manual_power_exertion(output, TickSpan::new(5), maximum, 200_000)
        .unwrap_or_else(|error| panic!("fast manual-power effort failed: {error:?}"));

    assert_eq!(
        slow.energy_cost_per_tick(),
        Energy::from_nanojoules(12_500_000_000)
    );
    assert_eq!(
        fast.energy_cost_per_tick(),
        Energy::from_nanojoules(25_000_000_000)
    );
    assert_eq!(slow.hydration_loss_per_tick(), Volume::from_microliters(3));
    assert_eq!(fast.hydration_loss_per_tick(), Volume::from_microliters(6));
    assert_eq!(
        slow.energy_cost_per_tick().nanojoules() * 10,
        fast.energy_cost_per_tick().nanojoules() * 5
    );
}
