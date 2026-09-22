//! Survey-campaign investment policy contracts.

use super::*;

#[test]
fn maintained_fieldwork_witnesses_span_campaign_horizons() {
    assert_eq!(planned_future_sites(1), 1);
    assert_eq!(
        [
            planned_future_sites(2),
            planned_future_sites(3),
            planned_future_sites(6),
        ],
        [1, 3, 2],
        "maintained coverage must include a copper-capable three-site survey-investment witness"
    );
}

#[test]
fn survey_investment_requires_a_material_disclosed_attention_payoff() {
    assert_eq!(
        select_survey_strategy(102, Some(118)),
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(
        select_survey_strategy(204, Some(196)),
        FieldworkSurveyStrategy::PointSearch,
        "an eight-tick expected gain is too small to justify scarce-copper survey capital"
    );
    assert_eq!(
        select_survey_strategy(306, Some(274)),
        FieldworkSurveyStrategy::IndexedChannel,
        "a three-site campaign clears the minimum expected-return threshold"
    );
    assert_eq!(
        select_survey_strategy(196, Some(196)),
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(
        select_survey_strategy(204, None),
        FieldworkSurveyStrategy::PointSearch
    );
}
