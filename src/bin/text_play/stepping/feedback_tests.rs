//! Construction messages must agree with the resolved structural lifecycle.

use super::{StructuralLifecycle, construction_status};

#[test]
fn completed_construction_never_claims_failed_or_unactivated_members_are_load_bearing() {
    let active = construction_status(Some(StructuralLifecycle::Active));
    assert!(active.contains("now load-bearing"));

    let failed = construction_status(Some(StructuralLifecycle::Failed));
    assert!(failed.contains("failed under structural load"));
    assert!(failed.contains("recover"));

    let planned = construction_status(Some(StructuralLifecycle::Planned));
    assert!(planned.contains("remains planned"));

    for absent_or_unusable in [
        Some(StructuralLifecycle::Failed),
        Some(StructuralLifecycle::Planned),
        None,
    ] {
        assert!(
            !construction_status(absent_or_unusable).contains("now load-bearing"),
            "only an active member can be reported as load-bearing"
        );
    }
}
