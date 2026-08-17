//! Dedicated gameplay exercise target kept separate from the crate's monolithic unit-test binary.

#[test]
fn gameplay_harness_configuration_contracts() {
    let gaps = deep_hearth::content::gameplay_harness_configuration_contract_gaps();
    assert!(
        gaps.is_empty(),
        "gameplay harness configuration failures:\n- {}",
        gaps.join("\n- ")
    );
}

#[test]
fn gameplay_harness_agent_experience_matrix() {
    deep_hearth::content::run_gameplay_harness();
}
