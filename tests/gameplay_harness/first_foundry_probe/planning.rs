//! Material opportunity, capital demand, and manual component preparation for the first foundry.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use deep_hearth::content::{
    ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER, ENERGY_STONE_THERMAL_SINK,
    EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
    EQUIPMENT_STONE_INGOT_MOLD, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_TIMBER_TREADLE_DYNAMO,
    EQUIPMENT_TIMBER_TREADLE_HAMMER, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL,
    FORM_REINFORCEMENT, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{project_manual_craft_equipment, project_manual_craft_hand_work};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::inventory::StockpileId;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::{CommoditySource, Registries};
use deep_hearth::survival::project_survival_resource_budget;

use super::super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::super::focused_witnesses::FOUNDRY_BOOTSTRAP_RECOVERY_COVERAGE_SEED;
use super::super::inherited_condition::healthy_used_equipment_condition;
use super::super::manual_craft_execution::execute_manual_craft;
use super::super::material_selection::select_stockpile_commodity_mass;
use super::super::seed::mix64;
use super::super::workshop_craft_planning::manual_craft_plan_with_available_equipment;
use super::recovery::{
    minimum_powered_ore_feed_for_target_recovery, projected_inherited_processing_batch_limit,
};

const FOUNDRY_RAW_INPUTS: [CommodityKey; 3] = [
    CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
    CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
    CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectedWorkshopTool {
    definition: EquipmentDefinitionId,
    condition: Condition,
}

pub(super) fn inherited_equipment_condition(
    registries: &Registries,
    definition: EquipmentDefinitionId,
    case: FocusedProbeCase,
) -> Condition {
    if matches!(
        case.role(),
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage
    ) {
        return Condition::PRISTINE;
    }
    healthy_used_equipment_condition(
        registries,
        definition,
        mix64(case.seed() ^ u64::from(definition.value()) ^ 0x464F_554E_4457_4541),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProjectedFoundryCraftRoute {
    pub(super) process: ProcessId,
    pub(super) input: CommodityKey,
    pub(super) input_mass: Mass,
    pub(super) batches: u64,
    pub(super) equipment: Option<EquipmentDefinitionId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FoundryBootstrapRoutePlan {
    pub(super) immediate: ProjectedFoundryCraftRoute,
    pub(super) fabrication_raw: BTreeMap<CommodityKey, Mass>,
    pub(super) capital_raw: BTreeMap<CommodityKey, Mass>,
}

impl FoundryBootstrapRoutePlan {
    pub(super) fn capital_native_copper(&self) -> Mass {
        self.capital_raw
            .get(&CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
            .copied()
            .unwrap_or(Mass::ZERO)
    }

    pub(super) fn capital_raw_mass(&self) -> Mass {
        self.capital_raw
            .values()
            .copied()
            .try_fold(Mass::ZERO, Mass::checked_add)
            .unwrap_or_else(|| panic!("first foundry raw component mass overflowed"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FoundryOwnedOreOpportunity {
    pub(super) mass: Mass,
    pub(super) copper_ppm: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FoundryResourceOpportunity {
    pub(super) native: Mass,
    pub(super) owned_ore: FoundryOwnedOreOpportunity,
    pub(super) immediate_native_input: Mass,
    pub(super) required_after_current: Mass,
    pub(super) recovery_batch_limit: Mass,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FoundryFabrication {
    pub(super) total_ticks: u64,
    pub(super) stone_ticks: u64,
    pub(super) wood_ticks: u64,
    pub(super) copper_ticks: u64,
    pub(super) hand_ticks: u64,
    pub(super) frame_saw_ticks: u64,
    pub(super) treadle_hammer_ticks: u64,
}

fn scaled_mass(mass: Mass, ppm: u32, context: &'static str) -> Mass {
    let milligrams = u128::from(mass.milligrams())
        .checked_mul(u128::from(ppm))
        .and_then(|value| value.checked_div(1_000_000))
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_else(|| panic!("first foundry {context} scaling overflowed"));
    Mass::from_milligrams(milligrams)
}

fn project_foundry_craft_route(
    registries: &Registries,
    commodity: CommodityKey,
    required: Mass,
    tools: &mut [ProjectedWorkshopTool],
    context: &'static str,
) -> ProjectedFoundryCraftRoute {
    let mut candidates = Vec::new();
    let handbook = registries
        .commodity_handbook_entry(commodity)
        .unwrap_or_else(|| panic!("first foundry {context} requested an unknown commodity"));
    for source in handbook.sources() {
        let CommoditySource::ManualCraft { process, .. } = *source else {
            continue;
        };
        let definition = registries
            .crafting()
            .get_manual(process)
            .unwrap_or_else(|| panic!("commodity handbook exposed an unknown manual process"));
        if !FOUNDRY_RAW_INPUTS.contains(&definition.input()) {
            continue;
        }
        let output = definition
            .outputs()
            .iter()
            .find(|output| output.commodity() == commodity)
            .map(|output| output.mass())
            .unwrap_or_else(|| {
                panic!(
                    "first foundry {context} producer {} lost requested output {}",
                    definition.process().value(),
                    commodity.value()
                )
            });
        let batches = required.milligrams().div_ceil(output.milligrams());
        let batches_nonzero = NonZeroU64::new(batches)
            .unwrap_or_else(|| unreachable!("nonzero foundry demand yields nonzero batches"));
        let input_mass = Mass::from_milligrams(
            definition
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("first foundry {context} input mass overflowed")),
        );

        if let Ok(work) =
            project_manual_craft_hand_work(registries, definition.process(), batches_nonzero)
        {
            candidates.push((
                ProjectedFoundryCraftRoute {
                    process: definition.process(),
                    input: definition.input(),
                    input_mass,
                    batches,
                    equipment: None,
                },
                (
                    work.duration().value(),
                    input_mass.milligrams(),
                    work.resource_budget().metabolic_energy().nanojoules(),
                    work.resource_budget().hydration().microliters(),
                ),
                None,
            ));
        }
        for (tool_index, tool) in tools.iter().copied().enumerate() {
            let Ok(projection) = project_manual_craft_equipment(
                registries,
                definition.process(),
                batches_nonzero,
                tool.definition,
                tool.condition,
            ) else {
                continue;
            };
            let body = project_survival_resource_budget(
                registries.survival().physiology(),
                definition.exertion(),
                projection.duration(),
            )
            .unwrap_or_else(|error| {
                panic!("first foundry {context} survival projection failed: {error:?}")
            });
            candidates.push((
                ProjectedFoundryCraftRoute {
                    process: definition.process(),
                    input: definition.input(),
                    input_mass,
                    batches,
                    equipment: Some(tool.definition),
                },
                (
                    projection.duration().value(),
                    input_mass.milligrams(),
                    body.metabolic_energy().nanojoules(),
                    body.hydration().microliters(),
                ),
                Some((tool_index, projection.condition_after())),
            ));
        }
    }
    let best_key = candidates
        .iter()
        .map(|(_, key, _)| *key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "first foundry {context} has no ordinary route from disclosed raw material to {}",
                commodity.value()
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, key, _)| *key == best_key);
    let (route, _, condition_update) = best
        .next()
        .unwrap_or_else(|| unreachable!("foundry route best key came from a candidate"));
    assert!(
        best.next().is_none(),
        "first foundry {context} has equally efficient projected routes to {}; add an explicit actor preference instead of relying on catalog identity",
        commodity.value()
    );
    if let Some((tool_index, condition_after)) = condition_update {
        tools[tool_index].condition = condition_after;
    }
    route
}

pub(super) fn foundry_bootstrap_route_plan(
    registries: &Registries,
    case: FocusedProbeCase,
    immediate_reinforcement: Mass,
) -> FoundryBootstrapRoutePlan {
    let mut tools = [
        ProjectedWorkshopTool {
            definition: EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            condition: inherited_equipment_condition(
                registries,
                EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
                case,
            ),
        },
        ProjectedWorkshopTool {
            definition: EQUIPMENT_TIMBER_TREADLE_HAMMER,
            condition: inherited_equipment_condition(
                registries,
                EQUIPMENT_TIMBER_TREADLE_HAMMER,
                case,
            ),
        },
    ];
    let immediate = project_foundry_craft_route(
        registries,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        immediate_reinforcement,
        &mut tools,
        "immediate reinforcement",
    );
    assert_eq!(
        immediate.input,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        "first foundry immediate route must begin from disclosed native copper"
    );
    let mut fabrication_raw = BTreeMap::new();
    for (commodity, required) in foundry_component_requirements(registries) {
        let route = project_foundry_craft_route(
            registries,
            commodity,
            required,
            &mut tools,
            "component preparation",
        );
        let total = fabrication_raw
            .get(&route.input)
            .copied()
            .unwrap_or(Mass::ZERO)
            .checked_add(route.input_mass)
            .unwrap_or_else(|| panic!("first foundry component raw demand overflowed"));
        fabrication_raw.insert(route.input, total);
    }
    let mut capital_raw = fabrication_raw.clone();
    let thermal_sink = registries
        .energy()
        .get_store(ENERGY_STONE_THERMAL_SINK)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("first foundry thermal sink lost its ordinary assembly profile"));
    for input in thermal_sink.inputs() {
        assert!(
            FOUNDRY_RAW_INPUTS.contains(&input.commodity()),
            "first foundry direct thermal-sink input {} is outside the disclosed raw-material family",
            input.commodity().value()
        );
        let total = capital_raw
            .get(&input.commodity())
            .copied()
            .unwrap_or(Mass::ZERO)
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("first foundry direct raw demand overflowed"));
        capital_raw.insert(input.commodity(), total);
    }
    FoundryBootstrapRoutePlan {
        immediate,
        fabrication_raw,
        capital_raw,
    }
}

fn native_with_shortfall(
    immediate_native_input: Mass,
    required_after_current: Mass,
    shortfall: Mass,
) -> Mass {
    immediate_native_input
        .checked_add(
            required_after_current
                .checked_sub(shortfall)
                .unwrap_or_else(|| unreachable!("foundry shortfall is bounded by required copper")),
        )
        .unwrap_or_else(|| panic!("first foundry native opportunity overflowed"))
}

fn insufficient_feed(required_feed: Mass, seed: u64) -> Mass {
    if required_feed.milligrams() <= 1 {
        return Mass::ZERO;
    }
    let fraction_ppm = 350_000
        + u32::try_from(mix64(seed ^ 0x464F_554E_4452_494E) % 500_001)
            .unwrap_or_else(|_| unreachable!("bounded foundry feed fraction fits u32"));
    let proposed = scaled_mass(required_feed, fraction_ppm, "insufficient ore");
    Mass::from_milligrams(
        proposed
            .milligrams()
            .max(1)
            .min(required_feed.milligrams() - 1),
    )
}

/// Builds disclosed copper opportunities around the current authored foundry workload threshold.
///
/// The actor never receives the generated regime. It sees only the resulting inventory and applies
/// ordinary native working plus its inherited powered ore-dressing capability. Keeping the
/// generator relative to current authored demand prevents content retuning from collapsing organic
/// samples into one stale outcome.
/// Recoverable worlds size owned ore against the already-earned powered dressing line rather than
/// pretending the player forgot that infrastructure at the foundry frontier.
pub(super) fn foundry_resource_opportunity(
    registries: &Registries,
    case: FocusedProbeCase,
    route_plan: &FoundryBootstrapRoutePlan,
    settlement_ingots: Mass,
    settlement_cast_mass: Mass,
) -> FoundryResourceOpportunity {
    let immediate_native_input = route_plan.immediate.input_mass;
    let required_after_current = route_plan
        .capital_native_copper()
        .checked_add(settlement_ingots)
        .and_then(|mass| mass.checked_add(settlement_cast_mass))
        .unwrap_or_else(|| panic!("first foundry workload-backed copper requirement overflowed"));
    assert!(
        !required_after_current.is_zero(),
        "first foundry workload threshold must remain positive"
    );
    let recovery_batch_limit = projected_inherited_processing_batch_limit(
        registries,
        inherited_equipment_condition(
            registries,
            deep_hearth::content::EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            case,
        ),
        inherited_equipment_condition(
            registries,
            deep_hearth::content::EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            case,
        ),
    )
    .unwrap_or_else(|| panic!("first foundry inherited processing line cannot accept a batch"));

    let grade = || {
        300_000
            + u32::try_from(mix64(case.seed() ^ 0x4F52_455F_4752_4144) % 400_001)
                .unwrap_or_else(|_| unreachable!("bounded foundry ore grade fits u32"))
    };
    let make_shortfall = |salt: u64| {
        let fraction_ppm = 200_000
            + u32::try_from(mix64(case.seed() ^ salt) % 350_001)
                .unwrap_or_else(|_| unreachable!("bounded foundry shortfall fraction fits u32"));
        let milligrams = scaled_mass(required_after_current, fraction_ppm, "organic shortfall")
            .milligrams()
            .max(1)
            .min(required_after_current.milligrams());
        Mass::from_milligrams(milligrams)
    };
    let opportunity = match case.role() {
        FocusedProbeRole::MaintainedAnchor => {
            let surplus = scaled_mass(required_after_current, 125_000, "anchor surplus");
            FoundryResourceOpportunity {
                native: immediate_native_input
                    .checked_add(required_after_current)
                    .and_then(|mass| mass.checked_add(surplus))
                    .unwrap_or_else(|| panic!("first foundry anchor opportunity overflowed")),
                owned_ore: FoundryOwnedOreOpportunity {
                    mass: scaled_mass(required_after_current, 500_000, "anchor owned ore"),
                    copper_ppm: 450_000,
                },
                immediate_native_input,
                required_after_current,
                recovery_batch_limit,
            }
        }
        FocusedProbeRole::MaintainedCoverage => {
            if case.seed() == FOUNDRY_BOOTSTRAP_RECOVERY_COVERAGE_SEED {
                let shortfall = scaled_mass(
                    required_after_current,
                    250_000,
                    "recovery coverage shortfall",
                );
                let copper_ppm = 550_000;
                let required_feed = minimum_powered_ore_feed_for_target_recovery(
                    registries,
                    shortfall,
                    copper_ppm,
                    recovery_batch_limit,
                )
                .unwrap_or_else(|| panic!("first foundry recovery coverage projection overflowed"));
                let reserve = scaled_mass(required_feed, 100_000, "recovery coverage reserve");
                FoundryResourceOpportunity {
                    native: native_with_shortfall(
                        immediate_native_input,
                        required_after_current,
                        shortfall,
                    ),
                    owned_ore: FoundryOwnedOreOpportunity {
                        mass: required_feed.checked_add(reserve).unwrap_or_else(|| {
                            panic!("first foundry recovery coverage ore overflowed")
                        }),
                        copper_ppm,
                    },
                    immediate_native_input,
                    required_after_current,
                    recovery_batch_limit,
                }
            } else {
                let shortfall = scaled_mass(required_after_current, 350_000, "coverage shortfall");
                let copper_ppm = 350_000;
                let required_feed = minimum_powered_ore_feed_for_target_recovery(
                    registries,
                    shortfall,
                    copper_ppm,
                    recovery_batch_limit,
                )
                .unwrap_or_else(|| panic!("first foundry coverage recovery projection overflowed"));
                FoundryResourceOpportunity {
                    native: native_with_shortfall(
                        immediate_native_input,
                        required_after_current,
                        shortfall,
                    ),
                    owned_ore: FoundryOwnedOreOpportunity {
                        mass: insufficient_feed(required_feed, case.seed()),
                        copper_ppm,
                    },
                    immediate_native_input,
                    required_after_current,
                    recovery_batch_limit,
                }
            }
        }
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            let copper_ppm = grade();
            match mix64(case.seed() ^ 0x464F_554E_4452_5247) % 3 {
                0 => {
                    let surplus_ppm = 25_000
                        + u32::try_from(mix64(case.seed() ^ 0x464F_554E_4452_5355) % 175_001)
                            .unwrap_or_else(|_| unreachable!("bounded foundry surplus fits u32"));
                    FoundryResourceOpportunity {
                        native: immediate_native_input
                            .checked_add(required_after_current)
                            .and_then(|mass| {
                                mass.checked_add(scaled_mass(
                                    required_after_current,
                                    surplus_ppm,
                                    "organic surplus",
                                ))
                            })
                            .unwrap_or_else(|| {
                                panic!("first foundry organic native opportunity overflowed")
                            }),
                        owned_ore: FoundryOwnedOreOpportunity {
                            mass: scaled_mass(
                                required_after_current,
                                500_000,
                                "organic unused ore",
                            ),
                            copper_ppm,
                        },
                        immediate_native_input,
                        required_after_current,
                        recovery_batch_limit,
                    }
                }
                1 => {
                    let shortfall = make_shortfall(0x464F_554E_4452_5243);
                    let required_feed = minimum_powered_ore_feed_for_target_recovery(
                        registries,
                        shortfall,
                        copper_ppm,
                        recovery_batch_limit,
                    )
                    .unwrap_or_else(|| {
                        panic!("first foundry organic recovery projection overflowed")
                    });
                    let extra_ppm =
                        u32::try_from(mix64(case.seed() ^ 0x464F_554E_4452_4558) % 250_001)
                            .unwrap_or_else(|_| {
                                unreachable!("bounded foundry ore surplus fits u32")
                            });
                    FoundryResourceOpportunity {
                        native: native_with_shortfall(
                            immediate_native_input,
                            required_after_current,
                            shortfall,
                        ),
                        owned_ore: FoundryOwnedOreOpportunity {
                            mass: required_feed
                                .checked_add(scaled_mass(
                                    required_feed,
                                    extra_ppm,
                                    "recoverable ore surplus",
                                ))
                                .unwrap_or_else(|| {
                                    panic!("first foundry recoverable ore opportunity overflowed")
                                }),
                            copper_ppm,
                        },
                        immediate_native_input,
                        required_after_current,
                        recovery_batch_limit,
                    }
                }
                _ => {
                    let shortfall = make_shortfall(0x464F_554E_4452_5348);
                    let required_feed = minimum_powered_ore_feed_for_target_recovery(
                        registries,
                        shortfall,
                        copper_ppm,
                        recovery_batch_limit,
                    )
                    .unwrap_or_else(|| {
                        panic!("first foundry organic shortage projection overflowed")
                    });
                    FoundryResourceOpportunity {
                        native: native_with_shortfall(
                            immediate_native_input,
                            required_after_current,
                            shortfall,
                        ),
                        owned_ore: FoundryOwnedOreOpportunity {
                            mass: insufficient_feed(required_feed, case.seed()),
                            copper_ppm,
                        },
                        immediate_native_input,
                        required_after_current,
                        recovery_batch_limit,
                    }
                }
            }
        }
    };
    assert!(
        opportunity.native >= opportunity.immediate_native_input,
        "first foundry world must fund the disclosed immediate reinforcement order"
    );
    opportunity
}

pub(super) fn select_commodity_mass(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) -> Vec<deep_hearth::inventory::MaterialLotSelection> {
    select_stockpile_commodity_mass(state, stockpile, commodity, mass, context)
}

fn foundry_component_requirements(registries: &Registries) -> BTreeMap<CommodityKey, Mass> {
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    let dynamo_upgrade = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_TREADLE_DYNAMO)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("first foundry dynamo lost its treadle-drive upgrade"));
    for input in dynamo_upgrade.additions().inputs() {
        let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        *entry = entry
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("first foundry dynamo upgrade demand overflowed"));
    }
    for equipment in [
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        EQUIPMENT_STONE_INGOT_MOLD,
    ] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| {
                panic!("first foundry equipment lost its ordinary assembly profile")
            });
        for input in profile.inputs() {
            let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
            *entry = entry
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("first foundry component demand overflowed"));
        }
    }
    let electrical = registries
        .energy()
        .get_store(ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("first foundry electrical buffer lost assembly"));
    for input in electrical.inputs() {
        let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        *entry = entry
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("first foundry electrical component demand overflowed"));
    }
    requirements
}

pub(super) fn craft_foundry_components(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    workshop_tools: &[EquipmentId],
) -> FoundryFabrication {
    let mut fabrication = FoundryFabrication::default();
    for (commodity, required) in foundry_component_requirements(registries) {
        let available = state
            .inventory()
            .get_stockpile(parts)
            .map(|stockpile| stockpile.get_mass(commodity))
            .unwrap_or_else(|| panic!("first foundry parts stockpile disappeared"));
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("component shortfall was established"));
        let (request, _batches) = manual_craft_plan_with_available_equipment(
            registries,
            state,
            &[raw],
            workshop_tools,
            commodity,
            missing,
            "first foundry component shaping",
        );
        let equipment = request.equipment();
        let ticks = execute_manual_craft(
            registries,
            state,
            request,
            parts,
            "first foundry component shaping",
        )
        .value();
        fabrication.total_ticks = fabrication
            .total_ticks
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("first foundry fabrication attention overflowed"));
        let material_ticks = match commodity.material() {
            MATERIAL_STONE => &mut fabrication.stone_ticks,
            MATERIAL_WOOD => &mut fabrication.wood_ticks,
            MATERIAL_COPPER => &mut fabrication.copper_ticks,
            other => panic!(
                "first foundry component fabrication unexpectedly targets material {}",
                other.value()
            ),
        };
        *material_ticks = material_ticks
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("first foundry material fabrication attention overflowed"));
        match equipment.and_then(|equipment| {
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.definition())
        }) {
            Some(EQUIPMENT_TIMBER_FRAME_SAW_BENCH) => {
                fabrication.frame_saw_ticks = fabrication
                    .frame_saw_ticks
                    .checked_add(ticks)
                    .unwrap_or_else(|| panic!("first foundry frame-saw attention overflowed"));
            }
            Some(EQUIPMENT_TIMBER_TREADLE_HAMMER) => {
                fabrication.treadle_hammer_ticks = fabrication
                    .treadle_hammer_ticks
                    .checked_add(ticks)
                    .unwrap_or_else(|| panic!("first foundry treadle-hammer attention overflowed"));
            }
            Some(other) => panic!(
                "first foundry component planner selected undisclosed workshop equipment {}",
                other.value()
            ),
            None => {
                fabrication.hand_ticks =
                    fabrication
                        .hand_ticks
                        .checked_add(ticks)
                        .unwrap_or_else(|| {
                            panic!("first foundry hand fabrication attention overflowed")
                        });
            }
        }
    }
    assert_eq!(
        fabrication.stone_ticks + fabrication.wood_ticks + fabrication.copper_ticks,
        fabrication.total_ticks,
        "first foundry material-family fabrication accounting must cover every component job"
    );
    assert_eq!(
        fabrication.hand_ticks + fabrication.frame_saw_ticks + fabrication.treadle_hammer_ticks,
        fabrication.total_ticks,
        "first foundry tool-route fabrication accounting must cover every component job"
    );
    fabrication
}

fn settlement_mold_addition_mass(
    registries: &Registries,
    commodity: CommodityKey,
    context: &'static str,
) -> Mass {
    registries
        .equipment()
        .get_equipment(EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD)
        .and_then(|definition| definition.upgrade_profile())
        .and_then(|upgrade| {
            upgrade
                .additions()
                .inputs()
                .iter()
                .find(|input| input.commodity() == commodity)
                .map(|input| input.mass())
        })
        .unwrap_or_else(|| panic!("settlement foundry mold lost its {context} upgrade input"))
}

pub(super) fn settlement_mold_ingot_requirement(registries: &Registries) -> Mass {
    settlement_mold_addition_mass(
        registries,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        "cast-ingot bootstrap",
    )
}

pub(super) fn settlement_mold_stone_requirement(registries: &Registries) -> Mass {
    settlement_mold_addition_mass(
        registries,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        "stone",
    )
}
