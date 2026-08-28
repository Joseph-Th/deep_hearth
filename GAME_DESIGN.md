# Game Design

This page owns intended player experience and progression. It is a product target, not implementation
evidence. Use [`STATUS.md`](STATUS.md) for current capability and [`README.md`](README.md) for project routing.

## Core fantasy

Deep Hearth is a first-person survival, settlement, and industrialization game in a persistent voxel world.
The player learns local physical and ecological constraints, then builds systems that handle them at increasing
scale.

`observe -> infer -> prepare -> extract -> invest -> delegate -> use returned attention -> reinvest`

Responsibility expands through:

`direct labor -> settlement systems -> organized labor -> mechanization -> industrial networks -> optimization`

Depth comes from interacting causes and constraints, not recipe nesting or repetitive input.

## Design laws

- **Model legible consequences.** Simulate detail when the player can observe, predict, exploit, avoid, or recover from it.
- **Progress transforms constraints.** Improvements replace pressures with new costs, obligations, or risks instead of deleting the system.
- **Solved repetition becomes delegable.** Tools, batching, labor, machinery, and automation should reduce attention cost while preserving matter, energy, time, maintenance, and logistics costs.
- **Technology is physical capability.** Materials, tools, heat, pressure, power, precision, infrastructure, labor, control, and knowledge make processes possible; abstract unlocks do not substitute for missing capability.
- **Infrastructure is embodied investment.** Buildings, machines, stores, networks, and transport occupy space, contain matter, require construction, and create operating obligations.
- **Materials matter across systems.** Better materials change tools, structures, storage, transport, machines, and controls; upgrades preserve object history unless a physical process changes it.
- **Information is progression.** Observation, prospecting, instruments, experiments, and specialists narrow uncertainty and expose tradeoffs.
- **Systems interlock.** Major systems exchange matter, energy, labor, information, risk, or environmental consequences.
- **Failure is readable and recoverable.** Important failures have understandable causes, useful warning signs where plausible, and a repair/adaptation/replacement path.
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

Exploration stays relevant from local survival through deposits, trade, breeding stock, transport corridors,
and infrastructure sites.

| Economy | Examples |
| --- | --- |
| Matter | finite resources, construction, consumption, waste, recycling |
| Energy | food, heat, mechanical work, fuels, electricity, storage, losses |
| Labor | player time, workers, animals, machines, skill, organization, automation |
| Ecology | water, fertility, reproduction, disease, habitat, nutrient cycles |
| Knowledge | observation, surveying, teaching, instruments, documentation |
| Risk | structural, environmental, biological, operational, economic failure |

Resources should usually have competing uses. Every material or social sink needs an intelligible cause.

## System direction

| System | Intended gameplay |
| --- | --- |
| Climate and seasons | Weather and seasons redirect work, transport, food, water, heating, construction, and risk. |
| Hydrology | Rain, runoff, groundwater, storage, drainage, irrigation, pumping, wastewater, and treatment make water both resource and force. |
| Terrain and structures | Material behavior, gravity, support, load, saturation, and damage create readable stability and costly failure. |
| Geology and prospecting | Learnable geological relationships plus increasingly precise observation, sampling, surveys, drilling, assays, and geophysics support inference rather than hidden-state revelation. |
| Mining | Deposit geometry, access, support, ventilation, drainage, haulage, lighting, waste, and safety shape mine layout and progression. |
| Materials | Form and physical properties make substitution meaningful across tools, structures, storage, transport, and machines. |
| Manufacturing | Shaping, joining, firing, comminution, screening, reduction, casting, heat treatment, machining, separation, chemistry, and electrolysis depend on real capability. |
| Metallurgy | Ore preparation, reduction/refining, alloy control, forming, and heat treatment have distinct physical roles; gangue and byproducts remain material streams. |
| Mechanical power | Human, animal, water, wind, and steam systems use physical transmission with torque/speed/power limits and maintenance. |
| Steam | Boilers, feedwater, pressure control, fuel, engines, distribution, heat loss, corrosion, scale, leaks, and overpressure enable high-power industry. |
| Electricity | Generation, motors, storage, transformers, conductors, switchgear, protection, and loads form physical networks. |
| Maintenance | Wear changes performance, reliability, precision, safety, and spare-part demand; technology changes maintenance work rather than removing it. |
| Survival | Energy, hydration, temperature, fatigue, wetness, injury, and infection are legible pressures increasingly solved through preparation and infrastructure. |
| Food and preservation | Food is physical and perishable; preservation changes future spoilage, dietary breadth affects recovery, and surplus enables population, specialists, expeditions, and trade. |
| Agriculture | Crop choice, climate, soil, nutrients, moisture, rotation, irrigation, harvest, seed, storage, and labor form a managed changing land system. |
| Ecology and animals | Populations respond to resources, habitat, seasons, predation, disease, and human pressure; domestication acts across generations. |
| Workers | Continuing economic relationships and skill affect throughput, waste, defects, diagnosis, control, and safety; the player sets goals and policies. |
| Settlements and trade | Independent production, consumption, specialization, transport, and geography create regional advantages and exchange. |
| Sanitation | Dense settlement creates wastewater, refuse, manure, crowding, and water-quality problems that require physical management. |
| Environment | Industry changes terrain, water, air, habitat, and material flows; environmental effects become engineering constraints. |

## Progression

Progression expands physical, economic, informational, and organizational capability. Eras describe
milestones; they are not mandatory tier gates.

| Era | Characteristic capability |
| --- | --- |
| Wilderness | shelter, fire, water, foraging, hunting, stone tools, clothing |
| Settlement | pottery, preservation, agriculture, storage, charcoal, trade, animal management |
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
Scarce resources should create consequential investment choices; delegated processes should return attention;
processed matter and better information should open the next physical capability.

Pacing constraints:

- Critical resources have legible clues and reliable first uses; richer/deeper resources require better information, access, or infrastructure rather than search randomness.
- Repeated manual input becomes delegable before it dominates play.
- Survival and industry overlap: infrastructure reduces some survival burdens while survival still constrains work.
- Long processes leave room for useful parallel work, preparation, observation, or logistics.
- Earlier infrastructure remains useful as a component, backup, branch, or lower-scale solution when physics permits.
- Complexity comes from interacting physical constraints, routing, quality, energy, maintenance, and information.

## Player information

Complex systems must explain themselves. Symptoms should reveal the existence and direction of a problem;
instruments increase precision. Player-facing information should answer:

1. What is happening?
2. Why is it happening?
3. What can I change?

## Mechanic acceptance

A mechanic belongs when its important causes/effects are perceivable, it creates a meaningful decision or
obligation, it interacts with another major system, and the player can improve, delegate, mitigate, or
automate it. Matter, energy, fluid, labor, and information transitions need explicit physical or social
authority.

Simplify or remove mechanics that depend on repetitive input, hide their causes, or do not create useful
decisions or world coherence.

## Boundary

This page does not own architecture, runtime scheduling, persistence, rendering implementation, networking,
or verification policy. Use [`README.md`](README.md) for those contracts.
