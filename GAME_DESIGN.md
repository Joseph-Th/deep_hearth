# Game Design

This page owns intended player experience and progression. It is not implementation evidence. Use
[`STATUS.md`](STATUS.md) for current capability and [`README.md`](README.md) for project routing.

## Design map

| Design question | Read |
| --- | --- |
| What is the core experience and what laws govern it? | [Core experience](#core-experience); [Design laws](#design-laws) |
| How should systems remain understandable and controllable across scale? | [Control-oriented legibility](#control-oriented-legibility) |
| What does the player repeatedly do and which economies interact? | [Player loop](#player-loop) |
| What experience should each major system eventually create? | [System direction](#system-direction) |
| How should capability and industrial scale progress? | [Progression](#progression) |
| What information must decisions expose and when does a mechanic belong? | [Player information](#player-information); [Mechanic acceptance](#mechanic-acceptance) |
| What future-development preference follows from the design, and what is outside this page? | [Development direction](#development-direction); [Boundary](#boundary) |

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

## Control-oriented legibility

Every important mechanic should support the same player/agent reasoning loop:

`observe -> diagnose -> compare -> act -> verify -> adapt`

A decision surface is strong when the player can determine, at an appropriate precision:

- the relevant current state and pressure;
- the causal reason the pressure exists;
- the legal levers available now;
- the important predicted tradeoffs of those levers;
- the committed consequence after acting;
- the recovery path if the result is poor or conditions change.

The game should not require privileged hidden-state knowledge, memorized implementation order, or repeated
trial-and-error to operate a system competently. Better tools and instruments may increase precision, but they
should refine the same underlying causal model rather than expose a separate ruleset.

Where practical, human UI, automated actors, and behavior evaluation should be able to consume the same
canonical projections and action semantics. An automation-only shortcut that knows more than the player, or a
UI-only formula that disagrees with authoritative simulation, weakens both game legibility and system coherence.

### One causal language across scale

Progression should deepen the same control vocabulary rather than replace it with tier-specific rules. A player
who understands that a process is constrained by feed state, capacity, power, condition, support, time, and
output custody should be able to carry that model from hand tools to workshops to automated plants. Later
technology may add sensing precision, routing choices, buffers, controllers, workers, and failure modes, but it
should not require learning an unrelated legality model for the same physical relationships.

This is also the basis for competent automation. A human, worker AI, planning agent, or evaluator may use
different policy, memory, and search depth, but should reason from the same observable facts, predictions,
blockers, action boundaries, and outcomes. Differences in intelligence belong in strategy, not privileged
simulation semantics.

### Composable planning

Prefer actions whose preconditions and consequences compose through shared physical dimensions: matter,
energy, labor, time, space/support, information, capacity, condition, and risk. New technology should mostly add
new providers, transformations, routing, or scale to those dimensions rather than one-off exceptions.

This makes long-horizon planning possible without requiring the player or an automated actor to learn a new
reasoning model for every machine. The interesting complexity should come from interacting constraints and
changing circumstances, not from inconsistent control semantics.

Long-horizon causality should be discoverable in the same language. A player who identifies a desired material,
capability, storage function, or process should be able to work backward through plausible physical providers,
transformations, construction requirements, and intermediate dependencies. This does not mean every path is
currently available or fully known: world resources, acquired information, infrastructure, access, and current
condition may still block it. The design should distinguish "this kind of route exists" from "you can execute
this route now" rather than hiding both behind trial and error.

Planning tools, worker AI, and automation may search this causal topology more efficiently than a player does,
but they should not receive a different topology or privileged current-state facts. Better instruments and
organization improve observation, estimation, search, scheduling, and execution of the same physical world.

### System controllability

An important mechanic should form a closed control loop before additional hidden complexity is layered onto it:

| Property | Design requirement |
| --- | --- |
| Observability | The player can detect the relevant state or symptom at a precision appropriate to current tools and knowledge. Hidden truth may remain hidden, but actionable evidence must have a legitimate acquisition path. |
| Causality | The player can connect the important symptom to a bounded set of plausible causes rather than treating outcomes as arbitrary rolls or undocumented exceptions. |
| Predictability | Before committing scarce matter, energy, time, or risk, the player can estimate the important direction and scale of consequences closely enough to make a reasoned choice. Better instruments may narrow uncertainty. |
| Intervention | At least one physical, informational, organizational, or strategic lever can materially change the future state. A simulated pressure with no meaningful response path is usually ambient bookkeeping, not gameplay. |
| Feedback | After intervention, the player can tell what changed and whether the action addressed the intended cause. Delayed effects need readable intermediate state or eventual attribution. |
| Recovery | Important failures expose repair, replacement, rerouting, fallback, learning, or deliberate abandonment where the physical model permits it. Failure should create a new problem state, not silently erase the decision space. |
| Delegation | Once a loop is understood and repeated, progression can transfer observation, triggering, execution, or monitoring to tools, workers, controls, or automation while preserving the same physical costs and failure semantics. |

These properties should deepen together. Adding precision without intervention creates surveillance rather than
agency; adding automation without legible feedback creates opaque optimization; adding failure without recovery
creates punishment rather than a managed system. The desired progression is from coarse manual control to
instrumented, buffered, delegated, and eventually automated control of the same causal world.

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
4. What important consequence should I expect if I change it?
5. What actually changed after I acted?

Symptoms should reveal the direction of a problem; better instruments increase precision.

Information should be local enough to support action and composable enough to support planning. Summary views
may aggregate many owners, but they should be projections of authoritative facts and should preserve drill-down
to the causal owner when a decision matters.

## Mechanic acceptance

A mechanic belongs when its important causes and effects are perceivable, it creates a meaningful decision or
obligation, it interacts with another major system, and the player can improve, delegate, mitigate, or automate
it. Matter, energy, fluid, labor, and information transitions need explicit physical or social authority.

Simplify or remove mechanics that depend on repetitive input, hide their causes, or do not create useful
decisions or world coherence.

## Development direction

The design preference is a dense connected simulation: close useful control loops before multiplying
disconnected content, and prefer reusable physical dimensions over feature-specific exceptions. The concrete
future integration sequence and vertical-slice completion criteria live in [`DIRECTION.md`](DIRECTION.md) so
product intent does not become mixed with current implementation priority.

## Boundary

This page does not own architecture, runtime scheduling, persistence, rendering implementation, networking,
or verification policy. Use [`README.md`](README.md) for those contracts.
