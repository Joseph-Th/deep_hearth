//! Regression tests for provisions readouts.

#[test]
fn natural_spring_readout_reports_finite_resting_supply_without_claiming_regeneration() {
    use deep_hearth::core::quantity::Volume;

    let rate = Volume::from_microliters(125);
    let remaining = Volume::from_microliters(1_250_000);
    let report = super::spring_supply_label(remaining, rate);
    assert!(report.contains("Natural spring supply:"), "{report}");
    assert!(
        report.contains("1250 mL") && report.contains("10h 0m"),
        "{report}"
    );
    assert!(
        report.contains("equivalent to") && report.contains("finite water"),
        "{report}"
    );
    let dry = super::spring_supply_label(Volume::ZERO, rate);
    assert!(dry.contains("Dry natural spring basin here"), "{dry}");
    assert!(dry.contains("Seek another source"), "{dry}");
    let no_drain = super::spring_supply_label(remaining, Volume::ZERO);
    assert!(
        no_drain.contains("1250 mL of finite surface water"),
        "{no_drain}"
    );
}
