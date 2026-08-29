# Game Design

This page owns intended player experience and progression. It is not implementation evidence. Use
[`STATUS.md`](STATUS.md) for current capability and [`README.md`](README.md) for project routing.

## Core experience

Deep Hearth is a first-person survival, settlement, and industrialization game in a persistent voxel world.
The player learns local physical and ecological constraints, then builds systems that handle them at increasing
scale.

`observe -> infer -> prepare -> extract -> invest -> delegate -> reinvest`

Responsibility expands through:

`direct labor -> settlement systems -> organized labor -> mechanization -> industrial networks -> optimization`

Depth comes from interacting causes and constraints, not recipe nesting or repetitive input.

## Design laws

- **Model legible consequences.** Simulate detail when the player can observe, predict, exploit, avoid, or recover from it.
- **Progress transforms constraints.** Improvements replace pressures with new costs, obligations, or risks rather than deleting the system.
- **Solved repetition becomes delegable.** Tools, batching, workers, machinery, storage, and automation reduce attention cost while preserving physical and economic costs.
- **Technology is physical capability.** Materials, tools, heat, pressure, power, precision, infrastructure, labor, control, and knowledge enable processes. Abstract unlocks do not replace missing capability.
- **Infrastructure is embodied investment.** Buildings, machines, stores, networks, and transport occupy space, contain matter, require construction, and create operating obligations.
- **Materials matter across systems.** Material properties affect tools, structures, storage, transport, machines, and controls. Upgrades preserve object identity unless a physical process replaces it.
- **Information is progression.** Broad evidence guides attention; targeted observation and better instruments buy precision. Discovery should reward inference rather than repetitive probing or hidden-state revelation.
- **Process depth needs physical purpose.** A process stage should change material state, recovery, purity, byproducts, energy, throughput, safety, maintenance, precision, or automation.
- **Systems interlock.** Major systems exchange matter, energy, labor, information, risk, or environmental consequences.
- **Failure is readable and recoverable.** Important failures have understandable causes, useful warning signs where plausible, and a repair, adaptation, or replacement path.
- **Fallbacks remain physical.** Earlier methods may remain usable, but later infrastructure should make them relatively expensive in attention, throughput, safety, or survival reserve.

## Player loop

1. Read terrain, climate, geology, ecology, and nearby societies.
2. Secure water, food, shelter, warmth, tools, and storage.
3. Extract and manage finite local resources.
4. Establish preservation, agriculture, structures, and workshops.
5. Specialize through materials, skills, domestication, trade, and dedicated production.
6. Delegate repeated work through workers, animals, schedules, logistics, and machinery.
7. Industrialize with larger process chains, power, chemistry, transport, and automation.
8. Expand and adapt to seasons, depletion, failures, and regional constraints.

Resources should usually have competing uses. Every material or social sink needs an intelligible cause.

| Economy | Examples |
| --- | --- |
| Matter | finite resources, construction, consumption, waste, recycling |
| Energy | food, heat, mechanical work, fuels, electricity, storage, losses |
| Labor | player attention, workers, animals, machines, skill, organization |
| Ecology | water, fertility, reproduction, disease, habitat, nutrient cycles |
| Knowledge | observation, surveying, teaching, instruments, documentation |
| Risk | structural, environmental, biological, operational, economic failure |

## System direction

| System | Intended gameplay |
| --- | --- |
| Climate and seasons | Weather and seasons redirect work, transport, food, water, heating, construction, and risk. |
| Hydrology | Water is both resource and force through rain, runoff, groundwater, storage, drainage, irrigation, pumping, wastewater, and treatment. |
| Terrain and structures | Material behavior, gravity, support, load, saturation, and damage create readable stability and costly failure. |
| Geology and prospecting | Geological relationships and increasingly precise observation support inference from regional clues to actionable local evidence. |
| Mining | Deposit geometry, access, support, ventilation, drainage, haulage, lighting, waste, and safety shape extraction. |
| Materials and manufacturing | Form and physical properties govern substitution, shaping, joining, comminution, separation, firing, casting, machining, and chemistry. |
| Metallurgy | Ore preparation, reduction/refining, alloy control, forming, and heat treatment have distinct physical roles; gangue and byproducts remain material streams. |
| Power | Human, animal, water, wind, steam, and electrical systems use finite generation, transmission, storage, losses, and maintenance. |
| Maintenance | Wear changes performance, reliability, precision, safety, and spare-part demand. Technology changes maintenance work rather than removing it. |
| Survival and food | Survival pressures are legible and increasingly managed through preparation, preservation, stable supply, and infrastructure. Dietary breadth affects recovery. |
| Agriculture and ecology | Crop choice, climate, soil, nutrients, moisture, populations, disease, habitat, and domestication form changing managed systems. |
| Workers and settlements | Skill, organization, specialization, trade, transport, and continuing economic relationships shift work away from direct player input. |
| Sanitation and environment | Dense settlement and industry create physical waste, pollution, water-quality, and habitat constraints that require management. |

## Progression

Progression expands physical, economic, informational, and organizational capability. Eras are milestones,
not mandatory tier gates.

| Era | Characteristic capability |
| --- | --- |
| Wilderness | shelter, fire, water, foraging, hunting, stone tools, clothing |
| Settlement | preservation, agriculture, storage, pottery, charcoal, trade, animal management |
| Copper | prospecting, mining, ore preparation, copper metallurgy, metal tools |
| Bronze | alloy control, improved tools, larger mines, stronger agriculture, mechanical workshops |
| Iron | bloomery iron, forging, mine engineering, structural construction, larger settlements |
| Steel | high-temperature furnaces, controlled metallurgy, precision components, machine tools |
| Steam | boilers, engines, pumps, rail transport, mechanized factories |
| Electricity | generation, motors, transformers, distribution, protection, electrochemistry |
| Industrial chemistry | acids, fertilizers, petroleum processing, advanced separation, chemical plants |
| Precision industry | advanced machine tools, bearings, instrumentation, automation, electronics |
| Advanced industry | advanced alloys, computer control, semiconductors, nuclear and other high-energy systems |

Industrialization shifts the dominant cost of work:

`human attention -> organized labor -> machinery -> energy + maintenance + logistics + control`

Progression should preserve meaningful sources, sinks, bottlenecks, and failure modes while increasing scale.
Scarce resources should create investment choices; delegated processes should return attention; processed
matter and better information should open further physical capability.

### Pacing constraints

- Critical resources have legible clues and reliable first uses. Richer or deeper resources require better information, access, or infrastructure rather than search randomness.
- Geological search moves coarse-to-fine. Broad evidence guides attention; local evidence resolves actionable targets without revealing hidden owners.
- Repeated manual input becomes delegable before it dominates play.
- Manual processing may remain as a physical fallback, but mechanization should improve throughput, recovery, durability, safety, or returned attention.
- Stable supply, preservation, and storage should replace repeated survival emergencies with preparation decisions and finite reserves.
- Preservation strength should have an explicit physical cause and visible benefit.
- Long processes should leave room for useful parallel work, preparation, observation, or logistics.
- Earlier infrastructure remains useful as a component, backup, branch, or lower-scale solution when physics permits.
- Add process stages only when they create a measurable physical or economic consequence.

## Player information

Player-facing information should answer:

1. What is happening?
2. Why is it happening?
3. What can I change?

Symptoms should reveal the direction of a problem; better instruments increase precision.

## Mechanic acceptance

A mechanic belongs when its important causes and effects are perceivable, it creates a meaningful decision or
obligation, it interacts with another major system, and the player can improve, delegate, mitigate, or automate
it. Matter, energy, fluid, labor, and information transitions need explicit physical or social authority.

Simplify or remove mechanics that depend on repetitive input, hide their causes, or do not create useful
decisions or world coherence.

## Boundary

This page does not own architecture, runtime scheduling, persistence, rendering implementation, networking,
or verification policy. Use [`README.md`](README.md) for those contracts.
