# Game Design

This document defines the intended player experience, progression, and simulation behavior. It is a
forward design target, not an implementation inventory. Use `STATUS.md` to determine what exists in
the current runtime and `TECHNICAL_DESIGN.md` for implemented technical contracts.

## Vision

Deep Hearth is a first-person survival, settlement, and industrialization game in a persistent voxel
world. The player begins as a vulnerable individual constrained by local weather, ecology, geology,
food, water, and primitive tools. Progress comes from understanding those constraints and building
physical, social, and industrial systems that make them manageable at larger scales.

The long-term fantasy is a visible transformation of both the world and the player's role:

- survive with direct personal labor;
- understand local resources and risks;
- establish reliable shelter, food, storage, and workshops;
- organize workers, animals, transport, and logistics;
- mechanize repetitive labor;
- build industrial networks for matter, energy, water, and information;
- optimize a landscape whose systems remain active and capable of failure.

Depth should come from interacting causes and constraints, not arbitrary recipe nesting or repetitive
input.

## Design laws

### Simulate legible cause and consequence

Model a system when the player can observe, predict, exploit, mitigate, or suffer its consequences.
Approximate intermediate detail that does not create meaningful decisions.

Examples include rain changing soil moisture and runoff, structural load producing visible strain,
furnace operation depending on heat and material preparation, and electrical overload producing loss,
heat, protection trips, or fire.

Hidden numerical overhead without a player-facing cause/effect chain is not useful realism.

### Progress changes constraints instead of deleting them

An improvement should solve one pressure while introducing or exposing another. Agriculture reduces
foraging dependence but creates fertility, planting, harvest, storage, and preservation work. Steam
reduces direct labor but creates fuel, water, pressure, maintenance, and distribution requirements.
Larger settlements provide labor but increase food, housing, sanitation, heat, and logistics demand.

There should rarely be a universally best solution independent of context.

### Solved repetition becomes delegable or automatable

Recurring work should tend to progress through:

1. direct player labor;
2. better tools and batching;
3. workers or animals;
4. powered machinery;
5. integrated industrial automation.

Automation removes attention cost, not physical cost. Matter, energy, time, maintenance, space,
transport, and waste remain real.

Early autonomous equipment does not need to outperform an improved hand tool at the same operation to
be valuable. Its first payoff may be attention: a slow material-backed machine can keep transforming
matter while the player mines, builds, provisions, or maintains something else. Later industrial
machinery should add a distinct scale/throughput transition rather than making the first primitive
machine implausibly fast just to signal progress.

Earlier methods should remain usable as emergency fallbacks when their physical prerequisites still
exist. A mechanized workshop may still be hand-powered through a compatible drive, for example, but
the player then trades industrial continuity against direct labor, survival reserve, equipment wear,
and lost attention. Progress should make such fallback less attractive, not arbitrarily forbidden.

Batching identical manual work is an input convenience only. It scales physical inputs, active time,
exertion, and wear with the requested amount.

### Technology is physical capability

The production chain is the technology tree. A process becomes practical because the player can
produce and operate the necessary materials, tooling, temperature, pressure, power, precision,
infrastructure, labor, and control.

Abstract unlocks may organize information or progression, but they should not substitute for missing
physical capability.

### Infrastructure is embodied investment

Machines, stores, shafts, buildings, networks, and transport infrastructure represent acquired and
shaped matter, occupied space, construction effort, and continuing operating obligations. Their payoff
must be a visible improvement in throughput, scale, reliability, safety, or player attention.

Exact dismantling is appropriate only while physical state is actually reversible. Wear, damage,
contamination, stored energy, and other changed state require corresponding salvage, repair, discharge,
or waste mechanics rather than a reset button.

### Material progression feeds back into existing problems

Better materials should improve tools, structures, machines, storage, transport, and control already
relevant to the player. Additive upgrades should preserve the identity and wear of the existing object
unless repair is separately performed.

A native or genuinely pure metal occurrence may support direct cold working before full reduction
metallurgy. Mineralized or mixed ore must not inherit that shortcut.

### Information is a progression resource

Exploration, prospecting, surveying, experimentation, instruments, and specialists should improve the
quality of future decisions. Successful information-gathering narrows uncertainty, exposes tradeoffs,
or reveals useful relationships. Repeated opaque searching with no improvement in decision quality is
not desirable difficulty.

### Systems interlock

Major systems should create consequences for one another. Rain affects soil and waterways; soil affects
crops; crops feed people and animals; animals provide food, materials, manure, transport, and power;
population supplies labor and consumes infrastructure; industry consumes matter, water, energy, and
labor; mining changes terrain and drainage; settlement and industry change ecology and trade.

Complexity should emerge from understandable interactions rather than isolated minigames.

### Failure is readable and usually recoverable

Important failure should normally have warning signs, understandable causes, and a path to recovery.
Failure creates new work and tradeoffs rather than functioning only as a random reset.

Examples include draining and reopening a flooded mine, repairing around structural damage, trading or
rationing after a failed harvest, rebuilding a transport route, or replacing lost breeding stock.

## Core loop and economies

The long-form player loop is:

1. **Explore** terrain, climate, geology, ecology, and nearby societies.
2. **Survive** through water, shelter, food, warmth, tools, and storage.
3. **Extract** timber, stone, clay, ores, fibers, fuel, food, and other natural resources.
4. **Settle** with preservation, agriculture, permanent structures, workshops, and storage.
5. **Specialize** through metallurgy, skilled work, domestication, trade, and dedicated production.
6. **Organize** labor, animals, work orders, stock targets, and logistics.
7. **Mechanize** with stored work, water, wind, animals, steam, and machinery.
8. **Industrialize** with larger processing chains, electrical systems, chemistry, and automation.
9. **Expand** through mines, farms, transport links, trade routes, and additional settlements.
10. **Adapt** to seasons, depletion, environmental change, failures, and new bottlenecks.
11. **Optimize** throughput, resilience, efficiency, specialization, and attention.

Exploration remains relevant throughout progression. Early exploration finds immediate survival
resources; later exploration finds deposits, trade partners, breeding stock, transport corridors, and
sites suited to specialized infrastructure.

Player decisions operate across six interacting economies:

- **matter:** finite resources, transformed forms, construction, consumption, waste, recycling;
- **energy:** food, heat, mechanical work, fuels, electricity, losses, and storage;
- **labor:** player time, workers, animals, machines, organization, skill, and automation;
- **ecology:** water, fertility, reproduction, predation, habitat, disease, and nutrient cycles;
- **knowledge:** observation, experience, teaching, instruments, surveying, and documentation;
- **risk:** faster or larger production may increase exposure to structural, environmental, biological,
  operational, or economic failure.

Resources should usually have competing uses. Sinks such as metabolism, spoilage, fuel consumption,
wear, maintenance, fertility loss, waste, compensation, corrosion, and infrastructure growth should
have physical or social explanations rather than exist as arbitrary taxes.

## World, seasons, water, and structures

### Climate and seasons

Environment should emerge from conditions such as latitude, elevation, rainfall, temperature,
seasonality, continentality, wind, soil, geology, and water availability rather than fixed biome labels
alone.

Seasons reorganize activity and resource flows. Spring may emphasize mud, floods, planting, and animal
births; summer growth, heat, irrigation, construction, and spoilage; autumn harvest, preservation,
fuel preparation, and logistics; winter heating, stored food, snow load, livestock feed, and indoor
production.

Bad weather should usually redirect play rather than simply disable it.

### Hydrology

Water is both a resource and a force. Rain, snowmelt, runoff, streams, groundwater, reservoirs, and
drainage can affect crops, terrain stability, mines, transport, structures, sanitation, and industry.

Water infrastructure may include ditches, drains, wells, channels, reservoirs, levees, culverts,
pumps, sewers, filtration, and treatment. Managing water should become a major settlement and
industrial discipline.

### Terrain and structural failure

Terrain and structures should respond to material behavior, gravity, support, load, and environmental
conditions. Granular material can avalanche, cohesive earth can weaken when saturated, brittle
materials can fail across unsupported spans, and structural materials deliberately carry load.

Important failure should progress through readable states such as stable, strained, cracking or
deforming, and failed. Visual or audible evidence should precede major collapse where the physical
system allows it.

Structural failure has economic consequences: injury, blocked access, damaged equipment, lost
throughput, reduced recovery, flooding, repair demand, and transport disruption.

## Geology, resources, and mining

### Geological structure

The underground world should have learnable geological relationships rather than arbitrary ore blobs.
Deposits may relate to sedimentary layers, intrusions, metamorphic zones, faults, veins, coal seams,
salt beds, hydrothermal systems, and surface placer processes.

Geology is both resource distribution and player knowledge. Rock type, structure, landform, elevation,
and surface evidence should support useful inference.

### Prospecting

Prospecting is progression in information quality. Methods may range from outcrops, loose indicators,
panning, and test pits through sampling, assays, drilling, geological mapping, and geophysical surveys.
Advanced methods increase precision and coverage rather than magically reveal exact hidden resources.

### Mining

Mining is an engineering system shaped by deposit geometry, grade, access, support, ventilation,
drainage, groundwater, haulage, lighting, equipment, waste rock, tailings, and worker safety.

Progression moves from hand pits and supported tunnels toward shafts, carts, pumps, powered drilling,
rail haulage, mechanized crushing, and large underground operations. Mine layout should follow the
resource and local physical constraints rather than a universal template.

## Materials, crafting, and metallurgy

### Materials and forms

Objects are physical forms made from materials. Forms may include plate, rod, wire, beam, pipe, blade,
sheet, block, powder, ingot, or other process-relevant geometry. Material properties such as density,
fusion behavior, heat capacity, strength, hardness, toughness, corrosion, conductivity, and resistance
should influence what those forms can do.

Material substitution should be meaningful because properties differ, not because each material is a
disconnected recipe tier.

### Crafting and manufacturing

Crafting should become increasingly process-based rather than grid-based. Relevant operation families
include shaping, cutting, joining, forming, firing, crushing, grinding, screening, smelting, casting,
hammering, heat treatment, sawing, turning, milling, drilling, pressing, rolling, filtering,
distillation, roasting, reaction, separation, and electrolysis.

A process may depend on input form and composition, material properties, tooling, temperature,
pressure, energy, time, skill, and equipment capability. Difficulty should come from establishing and
maintaining capable production systems, not from arbitrary intermediate items.

### Ore processing and metallurgy

Ore is not pure metal. A metallurgical chain may require mining, crushing, screening, grinding,
washing, concentration, smelting or reduction, refining, alloy control, forming, and heat treatment.
Each stage should have an understandable physical role.

Better ore preparation should improve recovery or enable lower-grade deposits rather than multiply
output through unexplained bonuses. Gangue, slag, tailings, wastewater, gas, and byproducts are material
streams or environmental consequences, not invisible yield loss.

Metallurgy should progress continuously through native/cold-workable metals, copper, bronze, iron,
steel, industrial steelmaking, controlled alloys, refractory systems, and advanced materials as the
player acquires the required heat, chemistry, precision, and infrastructure.

## Power, industry, and maintenance

### Mechanical power

Before electrical distribution, useful power may come from human effort, animals, water, wind, and
steam. Mechanical systems use shafts, gears, belts, pulleys, clutches, flywheels, and bearings.
Torque, speed, power, friction, slip, component limits, layout, and maintenance all matter.

Early direct player power can be accumulated in finite storage and spent by a machine. This is a
bridge from hand labor to mechanization, not free automation. Later sources replace the player's
attention while retaining generation, transmission, storage, and operating constraints.

### Steam

Steam is a major transition to high-power industry. A usable steam system requires fuel, water,
boilers, pressure control, feedwater, distribution, engines, exhaust or condensation handling, and
maintenance. Scale, corrosion, leaks, heat loss, low-water conditions, fuel quality, and overpressure
create design and maintenance pressure.

### Electricity

Electrical systems use recognizable voltage, current, power, resistance, energy, and heat-loss
relationships. Generators, motors, batteries, transformers, cables, switchgear, fuses, breakers, and
loads form a physical network.

Higher voltage is valuable when it improves transmission or equipment design, not because a tier flag
requires it. Poor design may cause losses, overheating, protection trips, equipment damage, or fire.

### Industrialization

Industry shifts limiting resources from direct player time and simple tools toward labor, fuel, power,
materials, transport, maintenance, logistics, knowledge, and control complexity.

Factories and infrastructure are physically located. Sawmills, mines, concentrators, foundries,
machine shops, steelworks, chemical plants, rail systems, power stations, and warehouses depend on
surrounding transport, storage, utilities, and workers.

### Maintenance and wear

Durability is normally degradation, not random disappearance. Tools dull, bearings wear, lubricants
are consumed, refractory erodes, boilers scale, metal corrodes, timber rots, belts stretch, and
machinery loses precision.

Maintenance should first create observable performance loss or warning states where plausible.
Technology changes maintenance work and spare-part requirements; it does not eliminate them.

## Survival, food, agriculture, and ecology

### Player survival

Survival should be demanding but legible. Relevant pressures may include metabolic energy, hydration,
temperature, fatigue, wetness, injury, blood loss, and infection. Clothing, shelter, wind, rain,
humidity, activity, and nearby heat should interact with exposure.

The desired progression is preparation and infrastructure, not permanent meter babysitting.

### Food and preservation

Food is perishable and storage history matters. Preservation methods such as drying, smoking, salting,
brining, fermentation, cellaring, sealed storage, cooling, and refrigeration change future spoilage
rate; they do not erase prior exposure.

Food surplus enables winter survival, population, specialist labor, expeditions, and trade. Dietary
variety should reward resilient provisioning and recovery without turning each meal into excessive
micromanagement. Meal convenience may group several foods but must preserve quantity, spoilage, and
physiological constraints.

### Agriculture

Agriculture depends on crop choice, climate, soil texture, organic matter, nutrient availability,
moisture, temperature, pH, compaction, planting window, irrigation, drainage, rotation, fertility
management, harvest timing, seed, storage, and labor.

Soil changes over time. Farming can improve or degrade land depending on management. Mechanization
should reduce attention and labor while creating machinery, fuel or power, maintenance, and logistics
requirements.

### Ecology and animals

Wild populations eat, drink, reproduce, migrate, compete, hunt, use habitat, and respond to seasons.
Population outcomes should follow ecology rather than arbitrary respawn demand. Overhunting, grazing,
deforestation, stored food, predators, disease, and habitat change can alter local populations.

Taming, training, and domestication are distinct. Training changes an individual's learned behavior;
domestication changes populations over generations. Useful animal roles include food, fiber, manure,
transport, draft power, riding, guarding, and pest control.

Breeding traits should be multidimensional. Size, growth, fertility, feed efficiency, production,
docility, endurance, climate tolerance, disease resistance, and temperament create different useful
breeding goals rather than one universal quality score. Phenotype reflects genetics and environment.
Pedigree and inbreeding can make unrelated breeding stock and animal trade strategically important.

## People, trade, labor, and settlements

### Independent societies and trade

Sentient societies should exist independently of the player. Settlements can farm, hunt, herd, gather,
build, craft, trade, fight, migrate, reproduce, prosper, decline, divide, or disappear.

Regions should develop comparative advantages from climate, ecology, geology, transport, skills, and
infrastructure. Trade routes move physical goods and can be improved or disrupted by roads, bridges,
flooding, landslides, conflict, settlement growth, and changing demand.

The player may participate in, support, redirect, compete with, or dominate regional economies, but is
not the only functioning economic actor.

### Employment and skill

Workers join through continuing economic relationships, not one-time ownership. Compensation may
include food, housing, clothing, tools, currency, valuable goods, livestock, or production shares.
Workers add labor, skill, knowledge, and demand while consuming resources and infrastructure.

Skill should have physical consequences such as throughput, waste, defects, tool damage, diagnosis,
process control, or safety. Knowledge can transfer through work, teaching, apprenticeship, and
documentation. Industrialization may reduce the skill burden of one task while creating demand for
mechanics, machinists, electricians, chemists, engineers, technicians, and supervisors.

### Worker autonomy

The player should issue goals, priorities, areas, schedules, and stock or production policies rather
than control every worker as an omniscient unit. Workers choose local actions required to satisfy those
goals using available paths, tools, materials, and infrastructure.

This shifts the player's role from doing individual tasks to designing reliable systems.

### Worker conditions and settlement growth

Workers have food, water, housing, heat, clothing, safety, compensation, health, social, and family
needs. Poor conditions can reduce effectiveness, drive refusal or departure, or make dangerous work
hard to staff.

Population is a progression axis alongside technology. Growth enables specialization and larger
projects while increasing demand for food, water, housing, fuel, sanitation, logistics, security, and
administration. Births take time to become workers; immigration depends on opportunity and conditions;
hiring requires compensation.

### Sanitation and public health

Dense settlements create wastewater, human and animal waste, refuse, crowding, and water-quality
problems. Progression can include latrines, manure handling, composting, drainage, wells, sewers,
pumping, filtration, and treatment.

Waste should be both a hazard and a potential material stream for fertilizer, compost, fuel, or later
industrial use.

## Environmental consequences and recovery

Industry changes terrain, air, water, habitat, and material flows. Tailings, slag, smoke, wastewater,
deforestation, erosion, contamination, noise, heat, and disturbed land create engineering constraints
such as drainage, settling ponds, retaining works, treatment, ventilation, waste storage, and
reclamation.

Environmental effects are physical gameplay systems, not morality meters.

Centralization can improve efficiency while increasing correlated risk. Deep mines, high-pressure
boilers, dense livestock, monoculture, and large factories may offer high productivity but create
larger failure consequences or harder recovery.

The strongest emergent situations should come from several systems becoming stressed at once, while
remaining understandable enough for the player to respond.

## Progression

Progression is a continuous expansion of physical capability rather than a strict sequence of content
locks. Useful broad eras are:

| Era | Characteristic capabilities |
| --- | --- |
| Wilderness | shelter, fire, water, foraging, hunting, stone tools, basic clothing |
| Settlement | pottery, agriculture, preservation, storage, charcoal, early trade, animal management |
| Copper | prospecting, mining, ore preparation, copper metallurgy, metal tools |
| Bronze | alloy control, improved tools, larger mines, stronger agriculture, mechanical workshops |
| Iron | bloomery iron, forging, mine engineering, structural construction, larger settlements |
| Steel | high-temperature furnaces, controlled metallurgy, precision components, machine tools |
| Steam | boilers, engines, pumps, rail transport, mechanized factories |
| Electricity | generation, motors, transformers, distribution, protection, electrochemistry |
| Industrial chemistry | acids, fertilizers, petroleum processing, advanced separation, large chemical plants |
| Precision industry | advanced machine tools, bearings, instrumentation, automation, electronics |
| Advanced industry | advanced alloys, computer control, semiconductors, nuclear and other high-energy systems |

These are descriptive milestones, not mandatory tier gates. The player advances when their physical,
economic, informational, and organizational systems can support the next capability.

Industrialization gradually transforms the dominant cost of work:

`human attention -> organized labor -> machinery -> energy + maintenance + logistics + control`

Each transition should open new scale while preserving meaningful sources, sinks, risks, and failure
modes.

## Player information

Complex systems must explain themselves. Basic problems should normally have visible, audible, or
behavioral symptoms; instruments increase precision.

Examples include structural cracks and sound, crop discoloration, animal body condition, smoke quality,
machine noise, gauges, geological host-rock clues, water turbidity, worker behavior, and explicit
inventory or process-state readouts.

Information should answer three questions where possible:

1. What is happening?
2. Why is it happening?
3. What can the player change?

## Design review checklist

A proposed mechanic should satisfy most of these questions:

1. Are its important causes or effects perceivable?
2. Does it create a meaningful decision or tradeoff?
3. Does it interact with another major system?
4. Does it create or modify a source, sink, constraint, obligation, or risk?
5. Can the player improve, delegate, mitigate, or automate it over time?
6. Does technology transform the problem instead of simply deleting it?
7. Is failure understandable and normally recoverable?
8. Does it strengthen world coherence?
9. Does it avoid unnecessary repetitive input?
10. Does it remain relevant at an appropriate stage of progression?
11. Is its physical or social authorization explicit rather than implied by an abstract tier?
12. If it moves or transforms matter, energy, fluid, labor, or information, is that transition legible
    to the player?

Mechanics that fail these tests should be simplified, integrated with another system, or removed.

## Scope

This document owns gameplay intent, progression, and player-facing system behavior. It does not own
engine architecture, chunk layout, rendering implementation, simulation scheduling, persistence
implementation, networking, or performance architecture. Those contracts belong to the engineering
documents listed in `README.md`.
