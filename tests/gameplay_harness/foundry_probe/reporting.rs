//! Foundry probe reporting kept separate from physical scenario orchestration.

use deep_hearth::core::quantity::{Energy, Mass, Temperature};
use deep_hearth::core::time::TickSpan;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::FormId;

#[derive(Clone, Copy)]
pub(super) struct FoundryReport {
    pub(super) seed: u64,
    pub(super) sample: &'static str,
    pub(super) outcome: &'static str,
    pub(super) feed_form: FormId,
    pub(super) offered: Mass,
    pub(super) melted: Mass,
    pub(super) unmelted: Mass,
    pub(super) melt_limit: &'static str,
    pub(super) first_cast: Mass,
    pub(super) cast_limit: &'static str,
    pub(super) molten_after_first: Mass,
    pub(super) recovery_cast: Mass,
    pub(super) recovery_limit: &'static str,
    pub(super) molten_final: Mass,
    pub(super) heating_strategy: &'static str,
    pub(super) direct_heating_mass: Mass,
    pub(super) direct_heating_duration: TickSpan,
    pub(super) preheated_mass: Mass,
    pub(super) preheated_duration: TickSpan,
    pub(super) preheat_applied: bool,
    pub(super) preheat_target: Temperature,
    pub(super) preheat_energy: Energy,
    pub(super) preheat_duration: TickSpan,
    pub(super) furnace_condition: Condition,
    pub(super) mold_condition: Condition,
    pub(super) initial_electrical: Energy,
    pub(super) melt_energy: Energy,
    pub(super) final_electrical: Energy,
    pub(super) initial_thermal: Energy,
    pub(super) thermal_before_cast: Energy,
    pub(super) thermal_without_cast: Energy,
    pub(super) released_heat: Energy,
    pub(super) final_thermal: Energy,
    pub(super) cooled_thermal: Energy,
    pub(super) cooldown_ticks: u64,
    pub(super) recovery_heat: Energy,
    pub(super) melt_duration: TickSpan,
    pub(super) cast_duration: TickSpan,
    pub(super) recovery_duration: TickSpan,
}

impl FoundryReport {
    pub(super) fn print(self) {
        if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
            self.print_verbose();
        } else {
            self.print_review();
        }
    }

    fn print_verbose(self) {
        reviewln!(
            "CAPABILITY FOUNDRY seed=0x{:016X} sample={} outcome={} reachability=bootstrapped-industrial installation=required+structurally-supported role=capability-evidence player-loop=not-claimed system-depth=[sensible-heating-energy-partition-counterfactual,phase-change,copper-recovery,finite-electrical-input,finite-thermal-recovery,passive-heat-rejection,wear] feed-form={} offered={}mg melted={}mg unmelted={}mg melt-limit={} first-cast={}mg cast-limit={} molten-after-first={}mg recovery-cast={}mg recovery-limit={} molten-final={}mg heating=[runtime-route:{} same-source-preheat:counterfactual-only direct:{}mg/{}t preheated:{}mg/{}t] preheat=[applied:{} target:{}mK energy:{}nJ duration:{}t] initial-condition=[furnace:{} mold:{}ppm] electrical=[initial:{}nJ melt:{}nJ remaining:{}nJ] thermal=[initial:{}nJ pre-cast:{}nJ no-cast-baseline:{}nJ released:{}nJ captured:{}nJ cooled:{}nJ cooldown:{}t recovery-heat:{}nJ] durations=[melt:{}t cast:{}t recovery-cast:{}t] matter=conserved",
            self.seed,
            self.sample,
            self.outcome,
            self.feed_form.value(),
            self.offered.milligrams(),
            self.melted.milligrams(),
            self.unmelted.milligrams(),
            self.melt_limit,
            self.first_cast.milligrams(),
            self.cast_limit,
            self.molten_after_first.milligrams(),
            self.recovery_cast.milligrams(),
            self.recovery_limit,
            self.molten_final.milligrams(),
            self.heating_strategy,
            self.direct_heating_mass.milligrams(),
            self.direct_heating_duration.value(),
            self.preheated_mass.milligrams(),
            self.preheated_duration.value(),
            self.preheat_applied,
            self.preheat_target.millikelvin(),
            self.preheat_energy.nanojoules(),
            self.preheat_duration.value(),
            self.furnace_condition.parts_per_million(),
            self.mold_condition.parts_per_million(),
            self.initial_electrical.nanojoules(),
            self.melt_energy.nanojoules(),
            self.final_electrical.nanojoules(),
            self.initial_thermal.nanojoules(),
            self.thermal_before_cast.nanojoules(),
            self.thermal_without_cast.nanojoules(),
            self.released_heat.nanojoules(),
            self.final_thermal.nanojoules(),
            self.cooled_thermal.nanojoules(),
            self.cooldown_ticks,
            self.recovery_heat.nanojoules(),
            self.melt_duration.value(),
            self.cast_duration.value(),
            self.recovery_duration.value(),
        );
    }

    fn print_review(self) {
        reviewln!(
            "FOUNDRY REVIEW seed=0x{:016X} sample={} role=capability-only outcome={} pipeline=compare-heating-counterfactual->melt->cast->passive-cool->retry feed-form={} offered={}mg melted={}mg unmelted={}mg melt-limit={} first-cast={}mg cast-limit={} molten-after-first={}mg recovery-cast={}mg recovery-limit={} molten-final={}mg heating=[runtime-route:{} same-source-preheat:counterfactual-only direct:{}mg/{}t preheated:{}mg/{}t] preheat=[applied:{} target:{}mK energy:{}nJ duration:{}t] electrical=[melt:{}nJ remaining:{}nJ] thermal=[initial:{}nJ pre-cast:{}nJ no-cast-baseline:{}nJ captured:{}nJ cooldown:{}t cooled:{}nJ recovery-heat:{}nJ] durations=[melt:{}t cast:{}t recovery-cast:{}t] matter=conserved",
            self.seed,
            self.sample,
            self.outcome,
            self.feed_form.value(),
            self.offered.milligrams(),
            self.melted.milligrams(),
            self.unmelted.milligrams(),
            self.melt_limit,
            self.first_cast.milligrams(),
            self.cast_limit,
            self.molten_after_first.milligrams(),
            self.recovery_cast.milligrams(),
            self.recovery_limit,
            self.molten_final.milligrams(),
            self.heating_strategy,
            self.direct_heating_mass.milligrams(),
            self.direct_heating_duration.value(),
            self.preheated_mass.milligrams(),
            self.preheated_duration.value(),
            self.preheat_applied,
            self.preheat_target.millikelvin(),
            self.preheat_energy.nanojoules(),
            self.preheat_duration.value(),
            self.melt_energy.nanojoules(),
            self.final_electrical.nanojoules(),
            self.initial_thermal.nanojoules(),
            self.thermal_before_cast.nanojoules(),
            self.thermal_without_cast.nanojoules(),
            self.final_thermal.nanojoules(),
            self.cooldown_ticks,
            self.cooled_thermal.nanojoules(),
            self.recovery_heat.nanojoules(),
            self.melt_duration.value(),
            self.cast_duration.value(),
            self.recovery_duration.value(),
        );
    }
}
