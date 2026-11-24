//! Viewer argument admission before GPU or world setup.

use super::*;

#[test]
fn nonfinite_camera_and_time_inputs_are_rejected() {
    for flag in ["--hour", "--yaw", "--pitch", "--rise"] {
        for value in ["NaN", "inf", "-inf"] {
            assert!(parse([flag.to_owned(), value.to_owned()].into_iter()).is_err());
        }
    }
}

#[test]
fn bounded_camera_angles_remain_finite() {
    let Parsed::Run(options) = parse(
        ["--yaw", "-450", "--pitch", "180"]
            .map(str::to_owned)
            .into_iter(),
    )
    .unwrap() else {
        panic!("camera options did not produce a run");
    };
    assert_eq!(options.yaw_degrees, 270.0);
    assert_eq!(options.pitch_degrees, 89.0);
}
