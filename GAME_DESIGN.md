# Game Design

This document defines the intended player experience and progression. It is a forward design target,
not an implementation inventory. Use [`STATUS.md`](STATUS.md) for current runtime capability and
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) for implemented technical contracts.

## Vision

Deep Hearth is a first-person survival, settlement, and industrialization game in a persistent voxel
world. The player begins as a vulnerable individual constrained by local climate, ecology, geology,
food, water, and primitive tools. Progress comes from learning those constraints and building systems
that handle them at increasing scale.

The player's role should move through:

`direct labor -> reliable household/settlement systems -> organized labor -> mechanization -> industrial networks -> optimization`

Depth comes from interacting causes and constraints, not recipe depth or repetitive input.

## Design laws

- **Model legible consequences.** Simulate detail when the player can observe, predict, exploit, avoid,
  or recover from its effects. Approximate detail that creates no decision.
- **Progress transforms constraints.** Improvements remove one pressure by introducing different costs,
  obligations, or risks rather than deleting the system.
- **Solved repetition becomes delegable.** Repeated work should progress from direct labor to better
  tools, batching, workers/animals, powered machines, and automation. Automation removes attention cost,
  not matter, energy, time, maintenance, or logistics cost.
- **Technology is physical capability.** Processes become possible through materials, tooling, heat,
  pressure, power, precision, infrastructure, labor, control, and knowledge. Abstract unlocks do not
  substitute for missing capability.
- **Infrastructure is embodied investment.** Buildings, machines, stores, networks, and transport occupy
  space, contain matter, take construction effort, and create operating obligations.
- **Materials feed back into existing problems.** Better materials improve tools, structures, storage,
  transport, machines, and controls. Upgrades should preserve existing object history unless repair or
  replacement explicitly changes it.
- **Information is progression.** Exploration, prospecting, instruments, experiments, and specialists
  should improve future decisions by narrowing uncertainty or exposing tradeoffs.
- **Systems interlock.** Major systems should exchange matter, energy, labor, information, risk, or
  environmental consequences rather than behave as isolated minigames.
- **Failure is readable and recoverable.** Important failure should have understandable causes, useful
  warning signs where physically plausible, and a path to repair, adaptation, or replacement.
- **Fallbacks remain physical.** Earlier methods may remain usable when their prerequisites still exist,
  but later infrastructure should make them increasingly expensive in attention, throughput, safety, or
  survival reserve.

## Player loop

The long-form loop is:

1. Explore terrain, climate, geology, ecology, and nearby societies.
2. Secure water, food, shelter, warmth, tools, and storage.
3. Extract timber, stone, clay, ores, fibers, fuel, and food.
4. Establish preservation, agriculture, permanent structures, and workshops.
5. Specialize through metallurgy, skilled work, domestication, trade, and dedicated production.
6. Organize workers, animals, schedules, stock targets, and logistics.
7. Mechanize repetitive work with stored work, water, wind, animals, steam, and machinery.
8. Industrialize with larger processing chains, electrical systems, chemistry, and automation.
9. Expand mines, farms, transport links, trade routes, and settlements.
10. Adapt to seasons, depletion, environmental change, and failure.
11. Optimize throughput, resilience, efficiency, specialization, and player attention.

Exploration remains useful throughout progression: early for survival resources, later for deposits,
trade partners, breeding stock, transport corridors, and infrastructure sites.

Player decisions operate across six interacting economies:

| Economy | Examples |
| --- | --- |
| Matter | finite resources, construction, consumption, waste, recycling |
| Energy | food, heat, mechanical work, fuels, electricity, storage, losses |
| Labor | player time, workers, animals, machines, skill, organization, automation |
| Ecology | water, fertility, reproduction, disease, habitat, nutrient cycles |
| Knowledge | observation, surveying, teaching, instruments, documentation |
| Risk | structural, environmental, biological, operational, economic failure |

Resources should usually have competing uses. Sinks should have physical or social explanations.

## System direction

| System | Intended gameplay |
| --- | --- |
| Climate and seasons | Weather and seasons reorganize work, transport, food, water, heating, construction, and risk. Bad conditions redirect play more often than they simply disable it. |
| Hydrology | Rain, snowmelt, runoff, streams, groundwater, reservoirs, drainage, irrigation, pumps, sewers, and treatment make water both a resource and a force. |
| Terrain and structures | Material behavior, gravity, support, load, saturation, and damage create readable stability and failure. Structural failure has economic consequences, not only visual effects. |
| Geology | Deposits follow learnable geological relationships. Surface evidence, rock type, structure, landform, and surveys support inference rather than exact hidden-resource revelation. |
| Prospecting | Methods progress from observation and simple sampling to assays, drilling, mapping, and geophysics by increasing precision and coverage. |
| Mining | Deposit geometry, access, support, ventilation, drainage, haulage, lighting, waste, and safety determine mine layout and progression. |
| Materials | Forms and material properties determine what objects can do. Substitution is meaningful because density, strength, hardness, thermal behavior, corrosion, conductivity, and other properties differ. |
| Manufacturing | Production is process-based: shaping, joining, firing, crushing, grinding, screening, reduction, casting, heat treatment, machining, separation, chemistry, and electrolysis depend on real capability. |
| Metallurgy | Ore preparation, reduction/refining, alloy control, forming, and heat treatment have distinct physical roles. Gangue, slag, tailings, wastewater, gases, and byproducts remain material/environmental streams. |
| Mechanical power | Human, animal, water, wind, and steam power use shafts, gears, belts, pulleys, flywheels, clutches, and bearings with real torque/speed/power limits and maintenance. |
| Steam | Boilers, feedwater, pressure control, fuel, distribution, engines, exhaust/condensation, corrosion, scale, leaks, heat loss, and overpressure create the transition to high-power industry. |
| Electricity | Generators, motors, storage, transformers, conductors, switchgear, protection, and loads form physical networks governed by recognizable electrical relationships. |
| Maintenance | Wear changes performance, reliability, precision, safety, and spare-part demand. Technology changes maintenance work; it does not eliminate it. |
| Survival | Energy, hydration, temperature, fatigue, wetness, injury, and infection should be legible pressures solved increasingly through preparation and infrastructure rather than constant meter attention. |
| Food and preservation | Food remains physical and perishable. Preservation changes future spoilage; food surplus enables winter survival, population, specialist labor, expeditions, and trade. |
| Agriculture | Crop choice, climate, soil, nutrients, moisture, pH, compaction, rotation, irrigation, harvest, seed, storage, and labor create a managed changing land system. |
| Ecology and animals | Populations respond to food, water, habitat, seasons, predation, disease, and human pressure. Training affects individuals; domestication changes populations across generations. |
| Workers | Workers join through continuing economic relationships. Skill affects throughput, waste, defects, diagnosis, process control, and safety. The player issues goals and policies rather than omniscient unit commands. |
| Settlements and trade | Settlements have independent production, consumption, growth, decline, specialization, and trade. Regional advantages emerge from geography, climate, resources, transport, skills, and infrastructure. |
| Sanitation | Dense settlement creates wastewater, refuse, manure, crowding, and water-quality problems; sanitation converts some wastes into managed hazards or useful streams. |
| Environment | Industry changes terrain, water, air, habitat, and material flows. Pollution and disturbed land create engineering constraints, not morality meters. |

## Progression

Progression is a continuous expansion of physical, economic, informational, and organizational
capability. These eras are descriptive milestones, not mandatory tier gates.

| Era | Characteristic capability |
| --- | --- |
| Wilderness | shelter, fire, water, foraging, hunting, stone tools, basic clothing |
| Settlement | pottery, preservation, agriculture, storage, charcoal, early trade, animal management |
| Copper | prospecting, mining, ore preparation, copper metallurgy, metal tools |
| Bronze | alloy control, improved tools, larger mines, stronger agriculture, mechanical workshops |
| Iron | bloomery iron, forging, mine engineering, structural construction, larger settlements |
| Steel | high-temperature furnaces, controlled metallurgy, precision components, machine tools |
| Steam | boilers, engines, pumps, rail transport, mechanized factories |
| Electricity | generation, motors, transformers, distribution, protection, electrochemistry |
| Industrial chemistry | acids, fertilizers, petroleum processing, advanced separation, large chemical plants |
| Precision industry | advanced machine tools, bearings, instrumentation, automation, electronics |
| Advanced industry | advanced alloys, computer control, semiconductors, nuclear and other high-energy systems |

Industrialization shifts the dominant cost of work:

`human attention -> organized labor -> machinery -> energy + maintenance + logistics + control`

Each transition should increase scale while preserving meaningful sources, sinks, bottlenecks, and
failure modes.

## Player information

Complex systems must explain themselves. Basic problems should normally have visible, audible, or
behavioral symptoms; instruments increase precision.

Information should help answer:

1. What is happening?
2. Why is it happening?
3. What can the player change?

The player should be able to connect structural cracks, crop condition, animal health, smoke, machine
noise, gauges, geological clues, water quality, worker behavior, and inventory/process readouts to the
systems causing them.

## Mechanic review

A proposed mechanic should usually satisfy these conditions:

- important causes and effects are perceivable;
- it creates a meaningful decision, constraint, obligation, or risk;
- it interacts with at least one other major system;
- the player can improve, delegate, mitigate, or automate it over time;
- progress transforms the problem rather than simply deleting it;
- failure is understandable and normally recoverable;
- repetitive input is not the primary source of difficulty;
- matter, energy, fluid, labor, and information transitions have explicit physical or social authority.

Simplify, integrate, or remove mechanics that do not create useful decisions or world coherence.

## Scope

This document owns gameplay intent, progression, and player-facing system behavior. It does not own
engine architecture, rendering implementation, simulation scheduling, persistence mechanics, networking,
or performance policy. Those contracts belong to the engineering documents listed in `README.md`.
