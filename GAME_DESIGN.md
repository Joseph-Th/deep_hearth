# Game Design

**Role:** Intended player experience and progression authority.

This page is not implementation evidence. Use
[`STATUS.md`](STATUS.md) for current capability and [`README.md`](README.md) for project routing.

## Design map

| Design question | Read |
| --- | --- |
| What is the core experience and what laws govern it? | [Core experience](#core-experience); [Design laws](#design-laws) |
| How should the game stay immediately familiar while deeper systems remain learnable? | [Familiar interaction shell](#familiar-interaction-shell); [Control-oriented legibility](#control-oriented-legibility) |
| What does the player repeatedly do and which economies interact? | [Player loop](#player-loop) |
| What experience should each major system eventually create? | [System direction](#system-direction) |
| How should capability and industrial scale progress? | [Progression](#progression) |
| What information must decisions expose and when does a mechanic belong? | [Player information](#player-information); [Mechanic acceptance](#mechanic-acceptance) |
| What future-development preference follows from the design, and what is outside this page? | [Development direction](#development-direction); [Boundary](#boundary) |

## Core experience

Deep Hearth is a first-person survival, settlement, and industrialization game in a persistent voxel world.
The player learns local physical and ecological constraints, then builds systems that handle them at increasing
scale.

The player fantasy is to be one capable individual who, through their own effort, judgment, and strategy,
progresses from primitive survival to technologically advanced resource management. The player starts with their
own body, knowledge, tools, local resources, and the surrounding environment, then decides what to learn, gather,
build, preserve, improve, mechanize, and eventually automate. Every later capability should feel like the result
of choices the player made and infrastructure they personally caused to exist. Settlements, machines, stores,
workers, and industrial networks extend that individual's agency; they do not replace the player as the source of
progress. The satisfaction comes from using the environment intelligently and turning scarce resources into
increasing control, resilience, productivity, and technological capability.

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

## Familiar interaction shell

Deep Hearth should feel like a block-survival game before the player understands any of its deeper simulation.
Use established Minecraft/Vintage Story interaction grammar wherever the physical model does not require a
different action:

- the active hotbar item is the default tool/item context;
- primary action attacks, breaks, harvests, or uses the held tool on the pointed world target;
- secondary/use action places, opens, consumes, or interacts contextually;
- one ordinary inventory view presents carried items, hotbar, personal crafting, and opened-container slots;
- familiar stack manipulation and quick-transfer gestures move items without exposing lot IDs, reservation tokens,
  custody revisions, or other simulation bookkeeping;
- ordinary fixed-input crafting asks for a recipe and amount; exact lot slicing is automatic unless freshness,
  temperature, provenance, or another physical distinction creates a meaningful stack-level choice;
- using held food or a drink source derives a sensible physical portion from the requested reserve rather than
  asking the player to type grams or milliliters;
- one direct food or drink use represents a human-scale serving; deep deficits take additional servings and their
  attention time rather than turning one familiar use action into a bulk refill;
- contextual help/handbook views answer "what is this?", "what can I do with it?", and "how do I make it?"
  from the same authoritative process, assembly, upgrade, maintenance, recovery, and capability data used by simulation;
- obvious direct actions remain attemptable. Better knowledge should improve prediction, route choice, recovery,
  safety, or efficiency rather than require a separate ritual before the player may try an action whose target is
  already visibly identified.

The familiar shell is a presentation and action-composition contract, not a simplification of physics. A slot or
stack may represent material that still has exact mass, temperature, composition, provenance, spoilage state,
and storage history. Slots/stacks are the manipulation grammar, not the sole capacity model: carried mass,
volume, and encumbrance may independently limit what fits, and physically incompatible lots may remain distinct
stacks even when they share a visible item identity. Amounts should use familiar counts where a form has a real
unit identity and physical mass/volume where matter is naturally bulk; do not invent universal item counts.
Containers can still alter preservation and tools can still
wear. The player should encounter those consequences as readable limits, tooltips, overlays, and feedback on
familiar interactions rather than as new manipulation verbs.

Ordinary harvested/mined drops should transfer into carried inventory automatically when slot and physical
capacity permit; otherwise they remain as world matter the player can pick up later. This lets an internal output
claim remain a transaction boundary without turning it into a second player command after every successful
break/harvest action.

Implementation transactions are not automatically player actions. Validation/commit tokens, output claims,
lot selection, reservations, and owner revisions may be necessary atomic boundaries internally; the ordinary UI
should compose them behind one expected action when no meaningful player decision exists between the steps.

## Control-oriented legibility

Every important mechanic should support one reasoning loop:

`observe -> diagnose -> compare -> act -> verify -> adapt`

The player should be able to identify the relevant state, its plausible cause, available levers, important
tradeoffs, the result of an action, and a recovery path. Better instruments narrow uncertainty; they do not expose
a separate ruleset or privileged hidden truth.

Progression should deepen one causal vocabulary across hand tools, workshops, and automated plants. Matter,
energy, labor, time, space/support, information, capacity, condition, and risk remain composable planning
dimensions while technology adds better providers, transformations, routing, sensing, buffering, and scale.

Long-horizon planning should let the player work backward from a desired material, capability, storage function,
or process through plausible dependencies. Authored possibility, current opportunity, and executable action must
remain distinguishable. Human UI, workers, automated actors, and evaluators may use different strategy and search
depth, but should reason from the same observable facts and physical consequences.

An important mechanic should close this control loop before gaining more hidden complexity:

| Property | Design requirement |
| --- | --- |
| Observability | Actionable state has a legitimate signal at the precision supported by current tools and knowledge. |
| Causality | Symptoms point to a bounded set of plausible causes. |
| Predictability | Scarce commitments have useful directional and scale estimates before action. |
| Intervention | At least one meaningful lever can change the future state. |
| Feedback | The player can identify what changed, including delayed effects where relevant. |
| Recovery | Important failures support repair, replacement, rerouting, fallback, learning, or deliberate abandonment. |
| Delegation | Repeated understood work can move to tools, workers, controls, or automation without erasing its physical costs or failure semantics. |

These properties deepen together. Precision without intervention is observation without agency; automation
without feedback is opaque; failure without recovery is punishment rather than a managed system.

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
- Geological search moves coarse-to-fine. Broad evidence guides attention; local evidence resolves actionable targets without revealing hidden owners. A visibly localized target can still be tried directly; sampling buys advance knowledge of hardness/resource scale rather than permission to swing a tool.
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

Prefer a dense connected simulation over disconnected feature count. [`DIRECTION.md`](DIRECTION.md) owns
integration priority and slice completion criteria.

## Boundary

This page owns intended player experience, not implementation or current capability. Use [`README.md`](README.md)
to route implementation, scope, and verification questions.
