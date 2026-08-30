//! Contract tests for condition and maintenance calculations.

use super::*;

fn condition(value: u32) -> Condition {
    match Condition::new(value) {
        Ok(condition) => condition,
        Err(error) => panic!("condition fixture failed: {error}"),
    }
}

#[test]
fn wear_clamps_at_failed_bound_without_destroying_records() {
    assert_eq!(
        calculate_condition_after_active_ticks(20, condition(10), TickSpan::new(1)),
        Condition::FAILED
    );
}

#[test]
fn active_tick_wear_clamps_without_duration_overflow() {
    assert_eq!(
        calculate_condition_after_active_ticks(
            CONDITION_PARTS_PER_MILLION,
            Condition::PRISTINE,
            TickSpan::new(u64::MAX),
        ),
        Condition::FAILED
    );
}

#[test]
fn usable_condition_allows_the_final_tick_that_reaches_failed() {
    assert_eq!(
        calculate_usable_condition_after_active_ticks(60, condition(100), TickSpan::new(2)),
        Ok(Condition::FAILED)
    );
}

#[test]
fn usable_condition_rejects_ticks_after_equipment_has_failed() {
    let error =
        match calculate_usable_condition_after_active_ticks(60, condition(100), TickSpan::new(3)) {
            Err(error) => error,
            Ok(after) => panic!(
                "third active tick unexpectedly completed with condition {} ppm",
                after.parts_per_million()
            ),
        };
    assert_eq!(error.before, condition(100));
    assert_eq!(error.maximum, TickSpan::new(2));

    let failed_error =
        match calculate_usable_condition_after_active_ticks(1, Condition::FAILED, TickSpan::new(1))
        {
            Err(error) => error,
            Ok(after) => panic!(
                "failed equipment unexpectedly completed active work at {} ppm",
                after.parts_per_million()
            ),
        };
    assert_eq!(failed_error.before, Condition::FAILED);
    assert_eq!(failed_error.maximum, TickSpan::ZERO);
    assert_eq!(failed_error.requested, TickSpan::new(1));
}

#[test]
fn warning_and_critical_bands_are_authored_independently_of_wear_curve() {
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("threshold fixture failed: {error}"),
    };
    assert_eq!(
        thresholds.classify(condition(800_000)),
        MaintenanceBand::Normal
    );
    assert_eq!(
        thresholds.classify(condition(500_000)),
        MaintenanceBand::Warning
    );
    assert_eq!(
        thresholds.classify(condition(200_000)),
        MaintenanceBand::Critical
    );
}

#[test]
fn maintenance_thresholds_require_nonempty_normal_and_warning_bands() {
    let warning = condition(600_000);
    assert_eq!(
        MaintenanceThresholds::new(warning, warning),
        Err(MaintenanceThresholdError::CriticalNotBelowWarning {
            warning_below: warning,
            critical_below: warning,
        })
    );
    assert_eq!(
        MaintenanceThresholds::new(Condition::PRISTINE, condition(250_000)),
        Err(MaintenanceThresholdError::WarningAtPristine {
            warning_below: Condition::PRISTINE,
        })
    );
}

#[test]
fn condition_deserialization_rejects_out_of_range_values() {
    let result: Result<Condition, _> = serde_json::from_str("1000001");
    assert!(result.is_err());
}

#[test]
fn condition_wear_rate_requires_normalized_nonzero_value() {
    assert_valid_condition_wear_ppm_per_tick(1);
    assert_valid_condition_wear_ppm_per_tick(CONDITION_PARTS_PER_MILLION);
    assert!(std::panic::catch_unwind(|| assert_valid_condition_wear_ppm_per_tick(0)).is_err());
    assert!(
        std::panic::catch_unwind(|| {
            assert_valid_condition_wear_ppm_per_tick(CONDITION_PARTS_PER_MILLION + 1)
        })
        .is_err()
    );
}
