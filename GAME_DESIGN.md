# Game Design

**Role:** Intended player experience and progression authority.

This page is not implementation evidence. Use
[`STATUS.md`](STATUS.md) for current capability and [`README.md`](README.md) for project routing.

## Design map

| Design question | Read |
| --- | --- |
| What is the core experience and what laws govern it? | [Core experience](#core-experience); [Design laws](#design-laws) |
| How should the game stay immediately familiar while deeper systems remain learnable? | [Familiar interaction shell](#familiar-interaction-shell); [Mechanical parity floor](#mechanical-parity-floor); [Control-oriented legibility](#control-oriented-legibility) |
| What does the player repeatedly do and which economies interact? | [Player loop](#player-loop) |
| What experience should each major system eventually create? | [System direction](#system-direction) |
| How should enemies behave as persistent world inhabitants? | [Living-world adversaries](#living-world-adversaries) |
| How should capability and industrial scale progress? | [Progression](#progression) |
| What information must decisions expose and when does a mechanic belong? | [Player information](#player-information); [Mechanic acceptance](#mechanic-acceptance) |
| What future-development preference follows from the design, and what is outside this page? | [Development direction](#development-direction); [Boundary](#boundary) |

## Core experience

Deep Hearth is a first-person survival, settlement, and industrialization game in a persistent voxel world.
The player learns local physical and ecological constraints, then builds systems that handle them at increasing
scale.

The player fantasy is to be a self-reliant field engineer: one capable individual who, through their own effort,
judgment, and strategy, progresses from primitive survival to technologically advanced resource management. The
player starts with their own body, knowledge, tools, local resources, and the surrounding environment, then
decides what to learn, gather,
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
- **Residue creates a logistics decision.** Cutting, grinding, dressing, firing and industry leave physically owned output alongside their useful products. Storage capacity, sorting, hauling, reuse and disposal sites should compete for space, labor and infrastructure. A byproduct with a recoverable value remains material, not a free deletion or a mandatory nuisance gesture. A waste stream matters when its amount, location, chemistry, recovery prospects or accumulation alters a future decision.
- **Waste storage is not general-purpose stock.** Containment should have material and capacity costs, and should not reward a player for storing food or finished parts in a refuse bay. Material recovery should remain a strategic alternative to permanently occupying finite dump space.
- **Dumping occupies the worksite.** A substantial loose residue heap can make its footprint unsuitable for ground storage. Cleanup, separation, containment, and choosing another site should compete with productive building rather than waste vanishing into an unlimited background.
- **Residue has situational utility.** Wood offcuts can supplement camp fuel, but a lower-heat charge is not equivalent to a full log. Recycling should save valuable feedstock or storage capacity at an observable cost, rather than yield free resources.
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

### Mechanical parity floor

Deep Hearth may replace Minecraft-like internal rules with deeper physical models, but it must not replace the
basic player verb with simulation bookkeeping. A stronger underlying model is an improvement only when the
familiar action remains at least as direct and the difference produces a concrete benefit in causality,
decision-making, recovery, scale, or performance.

The foundational block-survival shell therefore requires ordinary play to support, through production paths:

- continuous first-person looking and locomotion through a persistent voxel world, including collision,
  gravity, jumping, crouching, sprinting, falling, and terrain contact;
- visible generated terrain made from runtime block states rather than an abstract opportunity graph;
- crosshair/raycast targeting, hold-to-break work, contextual placement/use, and direct entity interaction;
- world drops, automatic pickup when capacity permits, active hotbar context, familiar stack manipulation,
  carried inventory, container transfer, and personal crafting;
- block geometry and collision shapes sufficient for full and common partial blocks, plus world lighting and
  enough environmental feedback to make darkness, shelter, fire, and exposure readable;
- deterministic world streaming and persistence so exploration, alteration, and return to the same place are
  ordinary gameplay rather than controlled setup.

Physical depth layers underneath that shell. Exact mass may constrain a familiar inventory; finite hydrology may
replace infinite water sources; structural support may make an apparently valid build fail; actual material
hardness may replace tool-tier tags. Those differences are desirable when they preserve the familiar action and
make its consequence more coherent. A one-voxel traversal command, a domain validator, or an abstract resource
record is not mechanical parity merely because it can prove the same downstream owner transition.

[`BLOCK_SURVIVAL_PARITY.md`](BLOCK_SURVIVAL_PARITY.md) owns the comparative checklist against Minecraft's
mechanical substrate. It is a design/audit reference, not evidence of current implementation.

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
| Creatures and adversaries | Hunger, shelter, territories, finite supplies, and social pressures make wildlife and hostile groups respond to world changes. Scouting, theft, defense, negotiation, flight, and eventual combat have physical causes rather than arbitrary player-centered spawns. |
| Workers and settlements | Skill, organization, specialization, trade, transport, and continuing economic relationships shift work away from direct player input. |
| Sanitation and environment | Dense settlement and industry create physical waste, pollution, water-quality, and habitat constraints that require management. |

The current mineral/metallurgy implementation covers more than the initial copper
line but not the complete industrial progression in this table. Deep host rock
contains finite lenses of hematite, cassiterite and sphalerite, distinguishable
as actual mineral compounds. Mining yields their ore-lump forms; crushing and
particle-size operations prepare these materials without magically releasing
iron, tin or zinc. Oxide and sulfide reduction must later explicitly account for
reductants, energy, oxygen/sulfur byproducts, waste and material yields. Separately,
existing iron, tin and zinc stock or reclaimed scrap can be remelted with finite
powered equipment and cast into new ingots. Metals differ in density, melting
threshold, heat storage and structural behavior. This is a partial material
chain, not a claim of complete smelting, alloying or GregTech-scale automation.

### Living-world adversaries

Enemies are residents of the same world as the player, not combat encounters materialized for convenience.
A camp, animal territory, or hostile settlement has a real location, resources, routes, needs, and consequences
while the player is elsewhere. Foraging removes finite food from its source; taking supplies relocates owned
matter; eating transfers it to a conserved biological-consumption boundary. A stolen parcel remains recoverable
until it is consumed, destroyed by an authored process, or carried beyond the player's reach through real travel.
Storage capacity, spoilage, weather, lighting, access, injuries, fear, and scarcity should matter to adversaries
through shared world facts, not separate scripted exceptions.

Adversarial behavior should have a legible progression: establish safety and food, observe and explore local
paths, take unattended opportunity, defend or retreat when challenged, and return to habitation to recover.
The player should be able to recognize tracks and activity, interrupt a raid, secure storage, guard a route,
use illumination or terrain, recover supplies, or choose to coexist. Future combat should extend the same
physical actor, inventory, injury, equipment, and observation boundaries instead of replacing this ecology
with a separate encounter simulation. Species and factions may have different incentives, but no actor
may know undiscovered remote player inventory or ignore a blocked real route.

Kobolds are a reptilian, stratified civilization, not a forever-static low-tier nuisance.
Their surface castes are unruly scavenger-engineers, while their deep temple castes retain
discipline, ritual, technical specialization, and access to substantially more sophisticated
machinery. The contradiction is intentional: one society can field noisy lash-ups made from
stolen scrap and maintain solemn chambers of advanced engineering. Their inherited service
traditions and uncertain technological ancestry leave room for earlier builders, without
making every surviving kobold merely a helpless servant awaiting absent masters. Distinguish
what each caste actually possesses, understands, maintains, remembers, or covets. Neither
ancestry nor fervent improvisation grants free machinery, supernatural information, or an
invisible army.

### Two faces of kobold technology

Surface kobolds are inventive wreckers and scavenger-engineers. They cobble together machinery from
parts stolen from the player, old mechanisms, stones, timber and anything else they can actually
carry home. Their constructions should be ingenious but badly matched: a copper winding becomes a
rattling alarm core, a timber handle braces a ratchet, and stone chips serve as an unreliable
striker. Different substitutions change performance, noise, reliability, upkeep and the value of
raiding versus defending local supplies. An unattended machine is an opportunity to steal
components, not an instantly copied blueprint. Improvised devices do not benefit from inexplicable
technology a scout has never observed or components it has not physically recovered. The player
may reclaim the loose materials from a den, disabling its contraption without deleting matter.
After recovering a copper core, a scout should seek the missing striker on actual visible
routes, even passing a nearer worthless stone pile when sufficiently provisioned. A
fully assembled rig should survive routine cache rearrangement unless the player deliberately
intercepts one of its finite, recoverable components.

Improvisation is a practical craft, not merely a visual style. Low castes should physically
assemble rough digging implements, pry bars, cutting edges, shields, traps, and eventually
weapons from recoverable wood, stone, metal and machine fragments, subject to real component
availability, work time, durability and skill. The same scrap may be more valuable as food
acquisition equipment, a warning device, a construction brace, or a weapon; scarcity and the
player's interventions determine that choice. Poor alignment, fragile fastening and crude
materials impose measurable limitations, but an inventive scavenger can achieve useful,
surprising functionality through substitute parts. Equipment must be carried, stashed,
degraded, repaired or lost as physical matter. A stolen tool is not automatically a learned
recipe, and a makeshift weapon must not grant damage before the combat and injury owners
can represent its actual consequences.

Far below those camps lie sealed temple districts, the industrial and ceremonial heart of
the surviving caste civilization. They are not crude shacks upgraded by a difficulty
modifier: stone precincts preserve mnemonic archives, motive engines, conduits and
machine-rooms with expertise unevenly retained by their resident custodians. Some
installations may be dormant or damaged; others should eventually be actively operated
and maintained by kobolds using finite energy, parts, tools and labor. Whether all the
original science was invented by kobolds remains an archaeological question, not a denial
of the present castes' technical competence. Chambers exist as actual subterranean terrain
and devices as embodied salvageable material. A closed archive is not automatically readable,
and a dead engine is not automatically usable. Excavating or repairing them should require
geology, access, power, material reconstruction and interpretation rather than a species-specific
magical unlock.
One sparse seed-selected precinct per broad district has an engraved, mineable copper threshold
at the surface and a buried stair approach that still requires excavation of intervening rock.
The hall connects two dormant machinery rooms and four small service bays,
each containing a physically distinct engineering component. One contains a
**tin identification plate array**; tin's low melting temperature and workability
suit thin marking stock, not load-bearing bearings. A second holds an **iron bearing
housing**, with much greater structural strength than the plate material. The third
contains a **zinc corrosion-protection panel**, representing replaceable sacrificial
metal used to protect harder assemblies. The fourth has a **stone heat-treatment
fixture**, whose mass, refractory location and mechanical clearance are more relevant
than ornamentation. Each is finite, excavatable world matter with authored density,
melting properties where applicable, hardness, and distinct rendered material. The
four bays have physically connected approaches and seeded placement variations,
without moving the existing wardens, archivists or supported stair. Removed components
yield recoverable tin, iron or zinc scrap, or stone chips; they do not grant new
recipes, power sources or an unlocked technology. The original copper machinery
remains finite legacy content rather than the default composition of every new device.

The subterranean castes differ physically and socially from surface raiders. Small **scouts**
and **watchmen** forage and improvise. Heavy **vault wardens** are temple-bred defenders
accustomed to narrow ceremonial passages and guarding machinery. Massive **archivists** are
older, stronger-bodied custodians of partial technical memory, not merely stronger scouts with
larger health bars. The hierarchy also governs access to tools, resources, ritual obligations,
workshops, knowledge and authority: low castes improvise and gather; high castes direct, interpret,
maintain and defend. Those roles may create material dependencies and social tensions rather than
perfect obedience. Their origins and the ancestry of their most advanced machines remain uncertain.
Temple inhabitants remain in real rooms; they must not emerge from nowhere whenever the
player's technology advances. Temple wardens react locally to living intruders:
they advance through supported hall cells without entering the player's occupied voxel and
withdraw one step at a time after contact is lost. Their visible defensive posture and position
survive save/load, and displaced wardens complete their physical retreat even when the player
leaves the temple chunk. Heavy wardens now obstruct the player's continuous walking footprint,
while massive archivists occupy a real two-by-two-by-three-cell chamber volume at their
inherited machines. Neither can be bypassed by walking through a sighted or concealed body;
player admission, trusted loads and respawn must also respect occupied space. Combat,
caste-specific physiology, negotiation, general subterranean movement and relic restoration
remain future systems, not hidden exceptions to the present simulation.

One early interaction bridges these societies: a mobilized den with physically co-located
precision copper salvage and a suitable striker can sound a faulty mechanical warning over
one adjacent route edge. Bare stone clatters less reliably than a timber-braced ratchet; missing
parts or an unsuccessful beat leave the warning untransmitted. A successfully warned scout
retreats using the ordinary route rules. A second copper core and shaped striker can now
make a separate junction repeater: on a synchronized clatter beat, the den's warning reaches
a scout two connected route edges away. An industrially mobilized scavenger physically
delivers the extra copper to the junction, keeps the staged part there, then seeks the
missing striker. It does not strip its own assembled repeater while the den source works.
The player can intercept or take any component, breaking the relay without destroying or
duplicating matter; an unpowered junction device remains locally visible as scrap machinery.
Actual warning strikes now provide local audible clues: a nearby observer can hear the den
clatter even without seeing the distant scout, and can distinguish a synchronized junction
clatter when the two-hop warning succeeds. Off-beat or remote activity does not reveal
hidden enemy locations. Unlike sight, clatter can carry around nearby corners along
short, open air passages; solid rock and unloaded terrain block it. Bare stone
clappers carry farther than damped wooden ratchets. The listener hears the kind
of mechanism but cannot identify the exact position of a source that is hidden
from view. Walking over exposed relay parts can collect them through the ordinary
player pickup path, physically silencing that branch of the network. When the
player departs, a scavenger with reachable spare parts can physically rebuild
the damaged repeater on subsequent patrols; repeated sabotage therefore has a
material and travel cost rather than instantly making equipment reappear.
This establishes the scavenged-technology vocabulary without granting kobolds free ranged
communication, energy, weapons or distant knowledge.

Enemy pressure must respond to the player's demonstrated progress, not just elapsed time or
arbitrary level scaling. At first a lone kobold steals food and exposed scraps; after witnessing
engineered equipment, repeated defenses, and increasingly capable industry, its colony should
recognize that petty raids no longer suffice. It must mobilize a proportional, persistent
response: salvage and provision drives, actual tool and weapon assembly, coordinated groups,
defended caches, adapted routes and tactics, and eventually attempts to reopen temple-era
workshops or reconnect with surviving higher-caste authority. Escalation requires witnessed
local evidence or physical intelligence brought home, real population and equipment, finite
food and material, and travel along routes. Do not instantly spawn units, teleport stolen goods,
grant invented weapons, or reveal concealed player technology. The player must see warning signs, be able to disrupt
mobilization, and have options beyond simply fighting ever-stronger enemies. If a player
outgrows a threat, the world and the faction react rather than remaining irrelevant.

The complete faction response follows an observable physical chain: a resident witnesses a
player action or its local consequences; information travels home by a resident or functioning
signal; the colony compares its material and survival needs with the new risk or opportunity;
workers move recoverable inputs and spend time making or maintaining equipment; deployed
residents change routes, defenses, claims or negotiations; and the player can discover,
intercept, trade with, avoid or undo the resulting changes. Repeated theft, reclaimed scrap,
damaged alarms, lighted approaches, excavated temple thresholds, broken relics and working
player industry should have distinguishable consequences when actually observed. Warnings
from a surface den do not instantly inform a buried archive; temple response needs real
routes, signals, witnesses and its own finite capacity. Calm coexistence or leaving valuable
infrastructure intact must be viable outcomes, not merely a delay until a fixed combat wave.

The first material-backed worksite is a crude pry lever: a wooden handle and a
stone wedge or reclaimed solid copper ingot stay at the junction as recoverable
parcels. The same scavenger can physically upgrade its crude stone head from
player-shaped copper, if a route to the material remains visible and safe.
A copper head must be fitted to its exact handle by a scout who spends two
consecutive patrol work slots at the junction, foregoing movement and hauling.
Witnessed intrusion, a working alarm or hunger can interrupt fitting, so the
player has a real window to intervene. A nearby observer can see unfinished
work; parts are still loose and recoverable throughout. A stone head loses
10 g of recoverable chips per assisted split; a copper head
lasts longer, deforming into 5 g of recoverable copper scrap instead. When
copper becomes unusable, a staged stone fallback can take over without new matter.
An active alarm relay retains priority over using its components as tools, but an
inert copper relic cannot claim the worksite by mere proximity. Surplus materials
remain contestable. Players can take any part or guard an upgrade route to deny
the resulting work advantage. Replacing a fitted ingot or handle requires new
labor rather than inheriting a previous assembly. This remains a stationary
arrangement, not yet a portable manufactured tool or combat weapon.

Further faction milestones are ordered by causality rather than combat tier: meaningful
work time, tool portability and choices between implements, alarm parts and provisions; then a
sustained surface group with finite residents and report-driven defense; finally
functioning temple work, higher-caste command and interactions across the actual
underground access route. Each milestone needs an ordinary player-observable
counterfactual, exact custody, and deterministic save/load continuation before
expanding the number of creatures or devices.

Simulation cost must follow relevant activity, not total world history. Schedule slow needs, population
transitions, and remote settlement obligations at bounded authoritative deadlines; run fine-grained motion,
sensing, and tactical decisions only for actors that need local physical resolution. Search spatially indexed
nearby opportunities and traversable paths with explicit work budgets. Persist the minimal state needed
for deterministic continuation, including offscreen deadlines and conserved custody, and promote dormant
actors without teleporting resources, rerolling outcomes, or simulating every absent frame. Observability
and performance are acceptance criteria alongside world conservation and compelling behavior.

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

### Opening wilderness phase

The opening should spend real time in a useful stone-age economy rather than treating stone tools as disposable
tutorial prerequisites for copper. The player's first problems are immediate physical ones: obtain water and
food, make a safe place to work and sleep, establish fire and warmth, and turn nearby stone, wood, earth, and plant
matter into useful capability. Storage and tools are investments, not opening checklist items: make them when
carried capacity, perishability, safety, or a repeated workload creates a reason to pay their embodied matter and
attention cost. Making a tool should matter because it improves a repeated task the player is already doing, not
because a technology ladder demands that item once.

Opening locality should present a small problem space rather than march the player through a resource checklist.
Visible opportunity is not obligation. If drinkable water is nearby but current hydration is secure, continuing
toward useful material work should be correct; under thirst pressure, taking the water detour should be correct.
Likewise, visible stone or timber should become valuable because a real project, carrying constraint, preservation
need, or repeated workload creates demand, not because discovery itself requires immediate collection.

The first local food and water opportunities should buy enough breathing room for engineering choices instead of
forcing the actor back into subsistence every few hours. That does not mean the player should automatically harvest
or carry the whole opportunity. A useful opening pattern is to discover a finite local reserve, secure only the
portable amount current needs justify, and leave the remainder as a known future option. Survival pressure should
then grow from elapsed time, exertion, distance, consumption, spoilage, or a decision to exploit more of that
reserve rather than from an opening supply too small to support any other work.

The first roughly fifteen minutes are a pacing envelope, not a quota of infrastructure to fill. In an ordinary
start they should still be dominated by orientation, gathering, provisioning, fire/shelter work when those
pressures apply, and whichever first improvements the local situation actually justifies. A player who has enough
carried capacity, a known local food reserve, a sufficient portable ration, and no repeated cutting workload should
be correct to defer a box or an adze rather than harvest and store everything merely to satisfy opening pacing.
A visible copper clue may give the player a future objective, but the default opening should not already be
delivering useful copper, copper upgrades, or ore-processing machinery. First useful copper should normally come
after the player has a functional wilderness baseline and has spent additional attention locating, extracting,
and processing a real opportunity. Exceptional geography or expert play may shorten that path; an abstract unlock
must not lengthen it.

This phase should support several worthwhile stone-and-timber improvements before metal is necessary: cutting or
hewing, digging, stone extraction, food preparation, storage, fire making, and shelter construction. Add those
capabilities only through physical owners that support their real gameplay consequences. Do not add placeholder
recipes for fire, shelter, cordage, containers, or tools before the corresponding world interaction, thermal,
structural, survival, or resource-acquisition edge exists.

The stone age should also have a useful second layer after the first camp is stable. Bulk soft-rock work can
justify heavier stone quarry tools; repeated timber work can justify dedicated turning equipment; worn stone
components can justify a treadle grindstone; and simple human-powered flywheels or drives can become worthwhile
where a real non-metal workload exists. Copper should improve hardness access, throughput, durability, precision,
or delegation. It should not be the first material that makes sustained craft specialization possible.

Industrialization shifts the dominant cost of work:

`human attention -> organized labor -> machinery -> energy + maintenance + logistics + control`

Progression should preserve meaningful sources, sinks, bottlenecks, and failure modes while increasing scale.
Scarce resources should create investment choices; delegated processes should return attention; processed
matter and better information should open further physical capability.

### Pacing constraints

- The wilderness phase is a real progression era. Ordinary copper progression begins after, not instead of, the
  basic food/water baseline and the situationally relevant fire/shelter, storage, or stone-tool work that makes the
  local start genuinely sustainable. Do not require infrastructure whose current benefit does not justify its cost.
- Unaided carried mass is a body capability, not a scenario-sized inventory parameter. Early storage and later
  transport should become valuable because real loads exceed or compete with that capability, not because an
  episode resizes the player's pockets to manufacture pressure.
- A rested start should accumulate visible food/water pressure through ordinary work without being driven into a
  warning state merely to make survival relevant. Provisioning should account for the actual food and drink being
  used, including hydration carried by food, and should not force redundant serving actions just to top off a
  nearly full reserve.
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
