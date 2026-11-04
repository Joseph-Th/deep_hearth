//! Operation errors must explain the physical blocker and preserve nested causes.

use super::*;
use crate::content::PROCESS_MELT_PURE_COPPER;

#[test]
fn actual_precipitation_denials_have_actionable_messages_and_error_causes() {
    use std::error::Error;

    let fixture = charge_fixture();
    let before = fixture.state.clone();
    let galena = MaterialLotSelection::new(fixture.galena, Mass::from_milligrams(GALENA_MG));
    let iron = MaterialLotSelection::new(fixture.iron, Mass::from_milligrams(IRON_MG));
    let single = [galena];
    let missing = resolve_precipitation_process(
        &fixture.registries,
        &fixture.state,
        PrecipitationRequest::new(
            PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
            fixture.source,
            &single,
            fixture.furnace,
            fixture.electricity,
        ),
    )
    .unwrap_err();
    let underlying = "precipitation charge requires both crushed galena and iron scrap or chips";
    assert_eq!(
        missing.to_string(),
        format!("precipitation batch rejected: {underlying}")
    );
    assert_eq!(
        missing.source().map(ToString::to_string),
        Some(underlying.to_owned())
    );

    let wrong_charge = [
        galena,
        MaterialLotSelection::new(fixture.iron, Mass::from_milligrams(IRON_MG - 1)),
    ];
    let imbalance = resolve_precipitation_process(
        &fixture.registries,
        &fixture.state,
        PrecipitationRequest::new(
            PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
            fixture.source,
            &wrong_charge,
            fixture.furnace,
            fixture.electricity,
        ),
    )
    .unwrap_err();
    assert_eq!(
        imbalance.to_string(),
        format!(
            "precipitation batch rejected: precipitation charge has {GALENA_MG} mg galena and {} mg iron; whole charge units require {GALENA_MG}:{IRON_MG} mg",
            IRON_MG - 1
        )
    );

    let selected = [galena, iron];
    let unknown = resolve_precipitation_process(
        &fixture.registries,
        &fixture.state,
        PrecipitationRequest::new(
            PROCESS_MELT_PURE_COPPER,
            fixture.source,
            &selected,
            fixture.furnace,
            fixture.electricity,
        ),
    )
    .unwrap_err();
    assert_eq!(
        unknown.to_string(),
        format!(
            "precipitation process {} is not registered",
            PROCESS_MELT_PURE_COPPER.value()
        )
    );
    assert!(unknown.source().is_none());

    assert_eq!(fixture.state, before);
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );

    let job_error =
        PrecipitationJobValidationError::Powered(PoweredOreJobValidationError::MissingEnergy);
    assert_eq!(
        job_error.to_string(),
        "persisted precipitation job violates powered ore physics: missing consumed work-energy trace"
    );
    assert_eq!(
        job_error.source().map(ToString::to_string),
        Some("missing consumed work-energy trace".to_owned())
    );
}
