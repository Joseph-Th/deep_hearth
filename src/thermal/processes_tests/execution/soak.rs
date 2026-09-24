//! Long-horizon sensible-heating conservation and deterministic-replay coverage.

use super::*;

#[cfg(feature = "test-soak")]
struct SensibleHeatingSoakFixture {
    registries: Registries,
    state: AppState,
    source: StockpileId,
    destination: StockpileId,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
    initial_matter: crate::core::quantity::AggregateMass,
    initial_explicit_energy: crate::core::quantity::PreciseEnergy,
    wood: CommodityKey,
    target: Temperature,
}

#[cfg(feature = "test-soak")]
impl SensibleHeatingSoakFixture {
    fn new() -> Self {
        let registries = make_registries(EnergyCarrier::Electrical);
        let mut state = AppState::new();
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200))
            .unwrap_or_else(|error| panic!("heating soak source allocation failed: {error}"));
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200))
            .unwrap_or_else(|error| panic!("heating soak destination allocation failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(150),
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| panic!("heating soak input deposit failed: {error}"));
        let equipment = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE)
            .unwrap_or_else(|error| panic!("heating soak equipment allocation failed: {error}"));
        let energy_store = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            BATTERY,
            Energy::from_nanojoules(800_000_000),
        )
        .unwrap_or_else(|error| panic!("heating soak energy allocation failed: {error}"));
        let initial_matter = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| {
                panic!("heating soak initial matter accounting failed: {error}")
            })
            .total();
        let initial_explicit_energy = calculate_explicit_energy_accounting(&registries, &state)
            .and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            })
            .unwrap_or_else(|error| {
                panic!("heating soak initial energy accounting failed: {error}")
            });
        Self {
            registries,
            state,
            source,
            destination,
            equipment,
            energy_store,
            initial_matter,
            initial_explicit_energy,
            wood: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            target: Temperature::from_millikelvin(303_000),
        }
    }

    fn maybe_start_batch(&mut self, step: u64) {
        let available = self
            .state
            .inventory()
            .get_stockpile(self.source)
            .unwrap_or_else(|| panic!("heating soak source disappeared"))
            .get_mass(self.wood);
        if !step.is_multiple_of(13) || available < Mass::from_milligrams(10) {
            return;
        }
        let resolved = resolve_test_sensible_heating_process(
            &self.registries,
            &self.state,
            PROCESS,
            self.source,
            self.equipment,
            self.energy_store,
            self.target,
        )
        .unwrap_or_else(|error| panic!("heating soak resolution failed at step {step}: {error}"));
        validate_start_process(
            &self.registries,
            &self.state,
            resolved.process_resolution(),
            self.source,
            self.destination,
        )
        .unwrap_or_else(|error| {
            panic!("heating soak start validation failed at step {step}: {error}")
        })
        .commit(&mut self.state)
        .unwrap_or_else(|error| panic!("heating soak start commit failed at step {step}: {error}"));
    }

    fn advance(&mut self, step: u64) {
        let _ = advance_tick(&self.registries, &mut self.state)
            .unwrap_or_else(|error| panic!("heating soak tick {step} failed: {error}"));
    }

    fn audit(&self, step: u64) {
        validate_loaded_state(&self.registries, &self.state).unwrap_or_else(|error| {
            panic!("heating soak exhaustive audit failed at step {step}: {error}")
        });
        let matter = calculate_matter_accounting(&self.state)
            .unwrap_or_else(|error| {
                panic!("heating soak matter accounting failed at step {step}: {error}")
            })
            .total();
        assert_eq!(matter, self.initial_matter);
        let explicit_energy = calculate_explicit_energy_accounting(&self.registries, &self.state)
            .and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            })
            .unwrap_or_else(|error| {
                panic!("heating soak explicit energy accounting failed at step {step}: {error}")
            });
        assert_eq!(explicit_energy, self.initial_explicit_energy);
    }

    fn finish(self) -> AppState {
        assert_eq!(self.state.production().jobs().count(), 0);
        assert_eq!(
            self.state
                .inventory()
                .get_stockpile(self.source)
                .map(|stockpile| stockpile.get_mass(self.wood)),
            Some(Mass::ZERO)
        );
        assert_eq!(
            self.state
                .inventory()
                .get_stockpile(self.destination)
                .map(|stockpile| stockpile.get_mass(self.wood)),
            Some(Mass::from_milligrams(150))
        );
        assert_eq!(
            self.state
                .energy()
                .get_store(self.energy_store)
                .map(|store| store.stored()),
            Some(Energy::from_nanojoules(35_000_000))
        );
        assert!(
            self.state
                .inventory()
                .lots()
                .filter(|lot| lot.stockpile() == self.destination)
                .all(|lot| lot.temperature() == self.target
                    && lot.composition() == &MaterialComposition::pure(MATERIAL_WOOD))
        );
        let final_matter = calculate_matter_accounting(&self.state)
            .unwrap_or_else(|error| panic!("heating soak final matter accounting failed: {error}"))
            .total();
        assert_eq!(final_matter, self.initial_matter);
        let final_explicit_energy =
            calculate_explicit_energy_accounting(&self.registries, &self.state)
                .and_then(|accounting| {
                    accounting
                        .total()
                        .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
                })
                .unwrap_or_else(|error| {
                    panic!("heating soak final energy accounting failed: {error}")
                });
        assert_eq!(final_explicit_energy, self.initial_explicit_energy);
        self.state
    }
}

#[cfg(feature = "test-soak")]
fn run_sensible_heating_soak() -> AppState {
    let mut fixture = SensibleHeatingSoakFixture::new();
    for step in 0_u64..5_000 {
        fixture.maybe_start_batch(step);
        fixture.advance(step);
        if step.is_multiple_of(97) {
            fixture.audit(step);
        }
    }
    fixture.finish()
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn sensible_heating_soak_preserves_determinism_matter_and_finite_energy() {
    let first = run_sensible_heating_soak();
    let second = run_sensible_heating_soak();

    assert_eq!(first, second);
    assert_eq!(first.tick().value(), 5_000);
    assert_eq!(
        first
            .equipment()
            .get_equipment(EquipmentId::new(1))
            .map(|record| record.condition()),
        Some(condition(985_000))
    );
}
