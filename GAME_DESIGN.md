# GAME_DESIGN.md

# Game Design

## 1. Vision

A first-person open-world survival, crafting, settlement, and industrialization game set in a persistent voxel world.

The player begins as a nearly powerless individual dependent on local ecology, geology, weather, and primitive tools. Over time, they establish food security, develop metallurgy, recruit workers, domesticate and selectively breed animals, mechanize production, build factories, and eventually operate complex industrial settlements.

The central fantasy is not simply technological advancement. It is the gradual conversion of a dangerous, partially understood natural world into an increasingly controlled system of agriculture, infrastructure, labor, energy, logistics, and industry.

The game combines the strongest design principles of:

- TerraFirmaCraft: geology, climate, agriculture, metallurgy, survival, and meaningful terrain instability.
- Vintage Story: embodied crafting, seasonal survival, prospecting, food preservation, mechanical industry, and physical production processes.
- GregTech: deep industrial progression, meaningful energy systems, process chains, multistage manufacturing, and factory-scale optimization.
- Don't Starve: strong sources and sinks, seasonal disruption, maintenance pressure, competing uses for resources, and a survival loop that remains relevant after the early game.

The game should support extreme depth without becoming an exercise in arbitrary recipe nesting or constant micromanagement.

---

## 2. Core Design Principles

### 2.1 Realism Must Face the Player

Simulate systems when they produce consequences the player can perceive, predict, exploit, or suffer.

Examples:

- Rain saturates soil, fills reservoirs, floods mines, affects crops, and can destabilize slopes.
- Buildings carry loads and can fail visibly when poorly supported.
- Furnaces require appropriate fuel, temperature, airflow, and material preparation.
- Electrical systems have voltage, current, resistance, heat, and overload consequences.
- Livestock require food, water, shelter, breeding stock, and labor.
- Workers consume resources and require safe, viable living conditions.

Avoid realism that exists only as invisible numerical overhead.

The rule is:

> Simulate cause and consequence. Approximate everything between them.

### 2.2 Every Advantage Creates New Obligations

Progress should solve old constraints while introducing new ones.

Examples:

- Agriculture solves food gathering but creates planting, fertility, storage, and harvest requirements.
- Livestock provide meat, milk, fiber, manure, transport, or draft power but consume pasture, water, winter feed, and labor.
- Steam power reduces human labor but creates fuel, water, maintenance, pressure, and boiler-management requirements.
- Electricity simplifies power distribution but creates generation, wiring, transformers, protection, and maintenance requirements.
- Large settlements provide labor but require food, housing, sanitation, heating, security, and infrastructure.

Technology should change the player's limiting resource rather than remove limits entirely.

### 2.3 Solved Chores Should Become Automatable

The game should create pressure without forcing the player to repeat primitive chores forever.

A recurring task should generally progress through:

1. Player performs it manually.
2. Better tools reduce effort.
3. Animals or workers can perform it.
4. Mechanical systems automate it.
5. Industrial systems integrate it into a larger production chain.

The underlying resource cost remains, but the player's attention moves toward higher-level problems.

Before true automation exists, repeated identical hand work may be issued in integral batches.
Batching is an input convenience only: matter, active time, exertion, wear, and other physical costs
still scale with the amount of work performed.

Early mechanical progression may also let the player convert direct physical labor into a small,
finite store of mechanical work. This is a bridge, not automation: the player's time, food/water
reserves, and tool wear remain the limiting inputs. Wind, water, animals, engines, and later power
systems should progressively replace that attention while preserving the need to generate, transmit,
store, and spend real work.

Material progression should feed back into the player's existing problems instead of creating an
isolated collection of higher-tier outputs. The first useful metal can reinforce tools and primitive
power equipment before full metalworking exists. Better components also remain subject to the rest of
their physical path: a stronger crank cannot charge faster than its storage or transmission can
accept, and a better mining tool still depends on labor, deposit access, storage, and wear.

### 2.4 Systems Should Interlock

Major mechanics should not exist as isolated minigames.

Rain affects soil.  
Soil affects crops.  
Crops feed people and animals.  
Animals provide manure and labor.  
Population provides workers.  
Workers support mining and industry.  
Industry consumes fuel, water, materials, and labor.  
Mining changes terrain and groundwater.  
Deforestation changes runoff and habitat.  
Trade connects regions with different resources.

The game should generate complexity through interaction between understandable systems.

---

## 3. Core Player Loop

The long-form loop is:

1. **Explore**  
   Learn local terrain, climate, geology, wildlife, and nearby peoples.

2. **Survive**  
   Secure water, shelter, food, warmth, tools, and storage.

3. **Extract**  
   Acquire timber, stone, clay, ores, fibers, food, fuel, and other natural resources.

4. **Settle**  
   Establish agriculture, preservation, permanent structures, workshops, and storage.

5. **Specialize**  
   Develop metallurgy, domestication, trade, skilled labor, and dedicated production spaces.

6. **Organize**  
   Recruit workers, train specialists, breed animals, assign work, and establish logistics.

7. **Mechanize**  
   Replace manual labor with water, wind, animal, and steam power.

8. **Industrialize**  
   Build electrical, chemical, metallurgical, and automated production systems.

9. **Expand**  
   Develop mines, farms, roads, trade routes, new settlements, and distant resource operations.

10. **Adapt**  
    Respond to seasons, depletion, ecological change, infrastructure failures, population growth, and new bottlenecks.

11. **Optimize**  
    Improve throughput, resilience, efficiency, specialization, and automation.

Exploration remains relevant at every stage. Early exploration searches for food and shelter. Late exploration searches for strategic deposits, trade partners, rare materials, breeding stock, or suitable industrial locations.

---

## 4. Fundamental Economies

The game is built around six interacting economies.

### Matter

Sources:

- Geology
- Biomass
- Agriculture
- Animals
- Trade
- Recycling

Sinks:

- Construction
- Wear
- Waste
- Spoilage
- Exports
- Irrecoverable processing losses

### Energy

Sources:

- Food
- Direct human mechanical work
- Wood
- Charcoal
- Coal
- Wind
- Flowing water
- Steam
- Fuels
- Electricity
- Advanced energy systems

Sinks:

- Metabolism
- Heating
- Mechanical work
- Processing
- Transportation
- Friction
- Transmission loss

### Ecology

Sources:

- Sunlight
- Water
- Soil fertility
- Reproduction
- Migration
- Nutrient cycling

Sinks:

- Predation
- Harvest
- Habitat loss
- Disease
- Starvation
- Environmental stress

### Labor

Sources:

- Player time
- Sentient workers
- Domestic animals
- Machines
- Automation

Sinks:

- Subsistence
- Travel
- Fatigue
- Maintenance
- Training
- Injury
- Inefficient organization

### Knowledge

Sources:

- Observation
- Experimentation
- Experience
- Specialists
- Teaching
- Instruments
- Surveying

Sinks:

- Worker death
- Cultural loss
- Obsolete techniques
- Lack of documentation
- Poor communication

### Risk

Productivity can often be increased by accepting greater risk.

Examples:

- Deeper mines offer greater access but greater collapse and flooding risk.
- Higher-pressure boilers produce more power but increase failure severity.
- Dense livestock operations improve handling efficiency but increase disease transmission.
- Large centralized factories improve efficiency but create major single points of failure.
- Monoculture simplifies harvesting but increases synchronized crop-failure risk.

The game should rarely have a universally optimal solution.

---

## 5. Sources and Sinks

Resources should usually have several competing uses.

Examples:

### Wood

- Construction
- Fuel
- Charcoal
- Mine supports
- Tool handles
- Furniture
- Paper
- Chemical feedstock

### Grain

- Human food
- Animal feed
- Seed
- Brewing
- Trade
- Industrial feedstock

### Animal Fat

- Food
- Soap
- Candles
- Lubricants
- Leather processing

### Sulfur

- Fertilizers
- Metallurgy
- Explosives
- Industrial chemistry

### Salt

- Food preservation
- Livestock nutrition
- Chemical production
- Trade

A resource should rarely have only one purpose.

The economy should continuously transform materials rather than simply accumulate them forever.

Common sinks include:

- Food consumption
- Spoilage
- Fuel consumption
- Tool wear
- Machine wear
- Building maintenance
- Fertility depletion
- Livestock feed
- Worker compensation
- Waste and process loss
- Corrosion
- Replacement parts
- Infrastructure expansion

Sinks should normally have a physical or social explanation rather than exist as arbitrary taxes.

---

## 6. Seasons and Time

Seasons reorganize the economy rather than merely changing visuals.

### Spring

Typical pressures:

- Flooding
- Mud
- Planting
- High water availability
- Animal births
- Difficult ground transport

### Summer

Typical pressures:

- Crop growth
- Irrigation demand
- Heat
- Construction
- Food spoilage
- Grazing

### Autumn

Typical pressures:

- Harvest
- Food preservation
- Fuel preparation
- Winter feed storage
- Major logistical demand

### Winter

Typical pressures:

- Heating
- Stored food consumption
- Snow and ice
- Reduced plant growth
- Livestock feed demand
- Indoor production
- Potentially easier frozen-ground transport

The goal is to create seasonal planning.

Bad weather should encourage a change in activity, not simply prevent play.

---

## 7. World and Climate

The environment is governed by interacting conditions rather than arbitrary biome labels.

Important factors include:

- Latitude
- Elevation
- Rainfall
- Temperature
- Seasonality
- Continentality
- Prevailing wind
- Rain shadow
- Soil
- Geology
- Water availability

Recognizable environments emerge from these conditions.

Climate affects:

- Crop viability
- Growing seasons
- Water balance
- Animal populations
- Clothing requirements
- Building design
- Snow
- Soil
- Forest composition
- Disease pressure
- Industrial requirements

---

## 8. Weather and Hydrology

Weather materially affects the world.

Rain can:

- Water crops
- Saturate soil
- Produce runoff
- Fill reservoirs
- Raise streams
- Flood low ground
- Flood mines
- Reduce slope stability
- Accelerate erosion
- Replenish groundwater
- Extinguish exposed fires

Snow can:

- Accumulate to varying depth
- Slow movement
- Insulate soil
- Load roofs
- Feed spring runoff
- Provide seasonal water storage

Water management becomes a major form of infrastructure.

Players may construct:

- Ditches
- Drains
- Irrigation channels
- Wells
- Reservoirs
- Levees
- Culverts
- Pumps
- Sewers
- Water-treatment systems

---

## 9. Terrain Stability and Structural Failure

Terrain and construction should respond plausibly to gravity and support.

Materials fall into behavioral categories such as:

### Granular Materials

Examples:

- Sand
- Gravel
- Loose soil
- Snow
- Tailings

These avalanche and seek stable slopes.

### Cohesive Earth

Examples:

- Clay
- Packed soil
- Mud

These tolerate steeper slopes but weaken when saturated.

### Brittle Materials

Examples:

- Stone
- Brick
- Concrete
- Ore

These tolerate compression but may fail across unsupported spans.

### Structural Materials

Examples:

- Timber
- Steel
- Reinforced concrete

These deliberately transfer loads.

Failure should usually progress through readable stages:

1. Stable
2. Strained
3. Cracking or deformation
4. Failure
5. Collapse

Warning signs may include:

- Creaking
- Cracks
- Dust
- Falling fragments
- Bent supports
- Water seepage

Physics should have economic consequences.

A collapse can:

- Injure or kill
- Destroy supports
- Damage equipment
- Bury tunnels
- Reduce ore recovery
- Block transport
- Create flooding hazards

---

## 10. Geology

The underground world should feel geologically structured rather than randomly populated with ore blobs.

Broad geological relationships include:

- Sedimentary layers
- Igneous intrusions
- Metamorphic zones
- Faults
- Veins
- Coal seams
- Salt beds
- Hydrothermal deposits
- Surface placer deposits

Different resources occur in plausible geological contexts.

Players should gradually learn that certain rocks, structures, elevations, and landforms imply particular resources.

Geology therefore becomes knowledge rather than background decoration.

---

## 11. Prospecting

Resource discovery is a progression of increasing information quality.

### Primitive

- Surface outcrops
- Loose mineral indicators
- Panning
- Visual rock identification

### Early Metalworking

- Test pits
- Sampling
- Basic prospecting tools
- Local geological inference

### Industrial

- Core drilling
- Assays
- Geological maps

### Advanced

- Magnetic surveys
- Electrical surveys
- Seismic methods
- Detailed subsurface models

Late-game technology should improve information quality rather than simply reveal resources magically.

---

## 12. Mining

Mining is an engineering discipline.

Important concerns include:

- Ore grade
- Geometry of the deposit
- Supports
- Ventilation
- Drainage
- Groundwater
- Transportation
- Lighting
- Explosives
- Waste rock
- Tailings
- Equipment access

Progression moves from:

- Hand-dug pits
- Timber-supported tunnels
- Shafts
- Ore carts
- Pumps
- Mechanical drilling
- Rail haulage
- Powered crushers
- Large underground operations

Mine design should reflect the deposit being exploited.

---

## 13. Materials

Objects should be understood as forms made from materials.

Examples of forms:

- Plate
- Rod
- Wire
- Beam
- Gear
- Pipe
- Blade
- Sheet
- Block

Materials provide properties such as:

- Density
- Melting point
- Heat capacity
- Strength
- Hardness
- Toughness
- Corrosion behavior
- Electrical resistance
- Thermal conductivity

This supports meaningful material substitution.

A component made from bronze, wrought iron, steel, or an advanced alloy should behave differently because of the material, not because it is a completely unrelated recipe.

---

## 14. Crafting and Manufacturing

Crafting should increasingly involve physical operations rather than abstract recipe grids.

### Primitive Processes

- Knapping
- Carving
- Splitting
- Weaving
- Clay forming
- Pottery firing

### Metalworking

- Crushing
- Smelting
- Casting
- Hammering
- Punching
- Drawing
- Welding
- Heat treatment

### Industrial Processes

- Sawing
- Turning
- Milling
- Drilling
- Pressing
- Rolling
- Extrusion

### Chemical Processes

- Washing
- Filtering
- Distilling
- Roasting
- Reacting
- Centrifuging
- Electrolysis

A production process may depend on:

- Inputs
- Material properties
- Tooling
- Temperature
- Pressure
- Energy
- Time
- Skill
- Equipment capability

The challenge should come from establishing capable production systems, not merely navigating enormous arbitrary recipe trees.

---

## 15. Metallurgy

Metallurgy is a continuous technological progression.

### Copper

- Mining
- Ore preparation
- Smelting
- Casting
- Working

### Bronze

- Alloy control
- Better casting
- Improved tools and weapons

### Iron

- Iron ore
- Charcoal
- Bloomery
- Bloom consolidation
- Wrought iron

### Steel

- Carbon control
- Higher-temperature furnaces
- Specialized processing
- Heat treatment

### Industrial Metallurgy

- Coke
- Blast furnaces
- Pig iron
- Steelmaking
- Alloying
- Controlled heat treatment
- Specialized refractory materials

Metal quality should emerge from material composition and processing quality.

---

## 16. Ore Processing

Ore is not equivalent to pure metal.

Deposits contain mixtures of useful minerals, gangue, and trace materials.

Processing may include:

1. Mining
2. Crushing
3. Screening
4. Grinding
5. Washing
6. Gravity separation
7. Flotation or other concentration
8. Smelting
9. Refining

Improved processing increases recovery and may expose valuable byproducts.

Industrial progression should make low-grade deposits economically useful rather than simply multiplying output through arbitrary bonuses.

---

## 17. Mechanical Power

Before electricity, mechanical power is a major industrial system.

Potential sources include:

- Human effort
- Animal power
- Waterwheels
- Windmills
- Steam engines

Mechanical systems use:

- Shafts
- Gears
- Belts
- Pulleys
- Clutches
- Flywheels
- Bearings

Important tradeoffs include:

- Torque
- Speed
- Power
- Friction
- Slip
- Mechanical limits
- Maintenance

Workshop layout should matter because power must physically reach machines.

---

## 18. Steam

Steam represents the first major leap into high-power industry.

Steam systems require:

- Boilers
- Fuel
- Water
- Pressure control
- Feedwater
- Steam distribution
- Engines
- Condensation or exhaust handling
- Maintenance

Potential problems include:

- Scale
- Low-water conditions
- Poor fuel
- Overpressure
- Corrosion
- Leaks
- Heat loss

Safe and efficient operation requires infrastructure rather than a single magical steam machine.

---

## 19. Electricity

Electrical systems should use recognizable physical concepts.

Important elements include:

- Generators
- Motors
- Batteries
- Transformers
- Cables
- Switchgear
- Fuses
- Breakers
- Loads

Important concepts include:

- Voltage
- Current
- Power
- Resistance
- Energy
- Heat loss

Higher voltage should become useful because it allows efficient transmission rather than because a technology tier arbitrarily requires it.

Incorrect system design can cause:

- Overheating
- Power loss
- Machine failure
- Tripped protection
- Fire

---

## 20. Industrialization

Industry transforms the player's problems.

Early limits are dominated by:

- Player time
- Food
- Tool quality

Later limits shift toward:

- Labor
- Fuel
- Power
- Materials
- Transportation
- Maintenance
- Logistics
- Knowledge
- Control complexity

Examples of industrial systems include:

- Sawmills
- Crushers
- Concentrators
- Foundries
- Machine shops
- Steelworks
- Chemical plants
- Rail systems
- Power stations
- Automated warehouses

Industrial production should be physically located and dependent on surrounding infrastructure.

---

## 21. Maintenance and Wear

Durability should usually appear as degradation rather than objects randomly vanishing.

Examples:

- Cutting tools become dull.
- Bearings wear.
- Lubricants are consumed.
- Furnaces lose refractory material.
- Boilers accumulate scale.
- Steel corrodes.
- Wood rots when persistently wet.
- Belts stretch or slip.
- Machinery gradually loses precision.

Poor maintenance should first reduce performance or create warning signs before producing major failure.

Technology changes the form of maintenance rather than eliminating it.

---

## 22. Agriculture

Agriculture should depend on environment, soil, weather, and labor.

Important soil properties include:

- Texture
- Organic matter
- Nitrogen
- Phosphorus
- Potassium
- Moisture
- Temperature
- pH
- Compaction

Agricultural systems include:

- Crop selection
- Planting windows
- Irrigation
- Drainage
- Crop rotation
- Fertility management
- Manure
- Compost
- Harvest timing
- Seed saving
- Mechanization

Soil should change over long periods.

Farming can improve or degrade land depending on management.

---

## 23. Food and Preservation

Food is perishable.

Important variables include:

- Food type
- Temperature
- Moisture
- Storage method
- Processing
- Preservation

Preservation methods may include:

- Drying
- Smoking
- Salting
- Brining
- Fermentation
- Pickling
- Cellars
- Sealed containers
- Cooling
- Refrigeration

Food surplus is strategically important because it enables:

- Winter survival
- Larger settlements
- Specialized workers
- Trade
- Military or expeditionary activity

Agricultural surplus is one of the foundations of industrialization.

Dietary variety should reward resilient provisioning rather than turn every meal into meter
micromanagement. Broad food groups contribute to recent nutritional balance. A varied diet improves
recovery and long-term resilience, while a repetitive diet remains usable for basic energy instead
of imposing an arbitrary hard health cap. Preservation and future cooking should therefore create
meaningful choices about which foods to store and combine, not merely increase calorie density.

Preservation changes the rate of future spoilage; it does not erase past exposure. Moving old food
into a cellar, sealed vessel, or later refrigerated store must retain the spoilage already accumulated
before that move. Splitting or recombining compatible food lots must likewise retain a conservative
history rather than allowing inventory management to manufacture freshness.

The player should be able to consume several selected foods as one meal rather than repeat the same
eat interaction for every ingredient. One meal still validates freshness, matter ownership, and
physiological absorption for every portion. Convenience groups decisions; it never bypasses food
quantity, spoilage, or metabolic limits.

---

## 24. Player Survival

Player survival should be demanding but understandable.

Important needs include:

- Energy
- Hydration
- Core temperature
- Fatigue
- Wetness
- Injury
- Blood loss
- Infection risk

Clothing and shelter interact with:

- Temperature
- Wind
- Rain
- Humidity
- Activity
- Nearby heat

The player should learn to manage environmental exposure through preparation and infrastructure rather than constant meter maintenance.

---

## 25. Creature Ecology

Wildlife exists as part of a living ecological system.

Animals should:

- Eat
- Drink
- Reproduce
- Migrate
- Compete
- Hunt
- Avoid predators
- Use habitat
- Respond to seasons
- Die from starvation, predation, disease, or age

Ecological relationships should continue without player involvement.

Examples:

- Predator populations depend on prey.
- Herbivores depend on vegetation.
- Heavy grazing suppresses plant recovery.
- Stored grain attracts rodents.
- Rodents attract predators.
- Deforestation changes habitat.
- Overhunting changes population structure.
- Livestock attract predators.

Animals should not simply respawn because the player needs more.

Local extinction is possible, while migration or deliberate reintroduction can restore populations.

---

## 26. Domestication, Training, and Breeding

These are separate systems.

### Taming

An individual animal learns to tolerate humans.

### Training

An individual animal learns useful behaviors.

### Domestication

A population changes genetically over generations through selective breeding.

Domestic animal roles may include:

- Meat
- Milk
- Fiber
- Eggs
- Draft power
- Riding
- Hauling
- Guarding
- Pest control

Important heritable traits may include:

- Mature size
- Growth rate
- Feed efficiency
- Fertility
- Milk production
- Fiber production
- Docility
- Endurance
- Speed
- Cold tolerance
- Heat tolerance
- Disease resistance
- Temperament

There should be no universal animal "quality" score.

Different breeding goals should produce different useful breeds.

Examples:

- Dairy cattle
- Draft cattle
- Hardy meat cattle
- Fast riding animals
- Heavy hauling animals
- Guard animals

Phenotype depends on both genetics and environment.

Excellent genetics cannot compensate completely for poor nutrition or disease.

---

## 27. Pedigree and Inbreeding

Player-managed breeding stock can have persistent pedigrees.

Useful records include:

- Parents
- Birth date
- Growth
- Production
- Health history
- Offspring
- Performance

Close inbreeding increases the risk of undesirable inherited traits.

This creates demand for unrelated breeding stock and makes animal trade economically meaningful.

Wild populations do not require the same degree of individual pedigree detail.

---

## 28. Sentient Peoples

The world contains sentient primitive societies that exist independently of the player.

Settlements may have:

- Population
- Households
- Food reserves
- Territory
- Fields
- Livestock
- Craftspeople
- Specialists
- Leadership
- Customs
- Technologies
- Trade relationships
- Security concerns

They should:

- Farm
- Hunt
- Herd
- Gather
- Build
- Craft
- Trade
- Fight
- Migrate
- Reproduce
- Age
- Die

Their settlements can prosper, decline, move, divide, or disappear.

The player enters an existing world rather than creating the only functioning society in it.

---

## 29. Trade and Regional Economies

Different regions should naturally produce different goods.

Examples:

A coastal settlement may export:

- Salt
- Fish
- Shell
- Marine products

A forest settlement may export:

- Timber
- Charcoal
- Furs
- Game

A mining settlement may export:

- Ore
- Metal
- Stone

Trade routes should exist independently of the player.

Physical changes to the world can affect trade:

- Roads improve transport.
- Bridges reduce travel cost.
- Floods interrupt routes.
- Landslides block passes.
- Conflict disrupts commerce.
- New settlements create demand.

The player can participate in, support, redirect, compete with, or dominate regional trade.

---

## 30. Hiring and Employment

Sentient workers are hired through continuing economic relationships rather than permanent one-time purchases.

Compensation may include:

- Food
- Housing
- Clothing
- Tools
- Currency
- Livestock
- Valuable goods
- Shares of production

A worker provides:

- Labor
- Skill
- Knowledge
- Combat ability
- Consumption demand

A worker consumes:

- Food
- Water
- Housing
- Heat
- Clothing
- Tools
- Compensation
- Medical resources
- Social infrastructure

Hiring additional workers therefore expands productive capacity while increasing settlement demand.

---

## 31. Work and Skill

Workers should become better through experience.

Relevant skills may include:

- Farming
- Mining
- Smithing
- Carpentry
- Machining
- Animal handling
- Construction
- Medicine
- Combat
- Surveying
- Chemistry
- Electrical work

Skill should have physical consequences rather than simply displaying RPG bonuses.

Experienced workers may:

- Waste less material
- Work faster
- Produce fewer defects
- Damage tools less often
- Recognize problems earlier
- Maintain better process control
- Operate dangerous equipment more safely

Losing highly skilled workers should matter.

---

## 32. Apprenticeship and Human Capital

Knowledge can be transferred through work and teaching.

Workplaces can include:

- Masters
- Experienced workers
- Apprentices

Traditional crafts may require long training.

Industrial machinery may reduce the skill required for some tasks while creating new specialist roles.

Industrialization can therefore shift demand from:

- Artisan skill

toward:

- Mechanics
- Machinists
- Electricians
- Chemists
- Engineers
- Instrument technicians
- Foremen

Human capital is a form of progression.

---

## 33. Worker Autonomy

The player should not control workers as omniscient units.

The player creates goals, policies, and infrastructure.

Examples:

- Field assignments
- Production orders
- Warehouse stock targets
- Patrol zones
- Maintenance priorities
- Work shifts

Workers determine the individual actions necessary to complete those goals.

A production order for wrought iron may create work for:

- Miners
- Haulers
- Charcoal burners
- Furnace workers
- Smiths
- Warehouse workers

The player's role evolves from performing tasks to organizing systems.

---

## 34. Worker Conditions

Workers are not machines.

They may resist, leave, refuse, or become less effective when conditions are unacceptable.

Relevant factors include:

- Food availability
- Compensation
- Housing
- Safety
- Work hours
- Injury risk
- Social relationships
- Cultural expectations
- Family obligations
- Conflict

Unsafe operations can therefore create labor problems as well as physical risk.

A mine that repeatedly kills workers should become difficult to staff.

---

## 35. Settlement Growth

Population creates a second progression axis alongside technology.

Possible settlement scale:

1. Individual
2. Homestead
3. Family farm
4. Hamlet
5. Village
6. Town
7. Industrial settlement
8. City

Larger settlements enable:

- Greater specialization
- Larger mines
- More complex industry
- Permanent defense
- Trade hubs
- Large infrastructure projects

But also demand:

- More food
- More housing
- More water
- More fuel
- More sanitation
- More logistics
- More security
- More administration

Population growth should never be free.

Births take years to become productive workers.

Immigration depends on conditions and opportunity.

Hiring requires compensation.

---

## 36. Sanitation and Public Health

Growing settlements create waste.

Important concerns include:

- Clean water
- Wastewater
- Human waste
- Animal waste
- Refuse
- Crowding

Progression may include:

- Latrines
- Manure pits
- Composting
- Drainage
- Wells
- Sewers
- Pumping
- Filtration
- Water treatment

Poor sanitation can create disease risk.

Waste can also become a resource through:

- Compost
- Fertilizer
- Biogas or later industrial uses

---

## 37. Labor and Industrial Progression

Industrialization gradually substitutes machines and energy for human labor.

Example mining progression:

### Manual

- Player mines
- Player hauls
- Player processes ore

### Organized Labor

- Miners
- Haulers
- Charcoal workers
- Furnace workers
- Smiths

### Mechanical

- Powered hoists
- Pumps
- Rail carts
- Crushers
- Mechanical workshops

### Steam

- Steam hoists
- Steam pumps
- Powered processing
- Larger mines

### Electrical

- Electric drills
- Electric pumps
- Conveyors
- Central processing
- Automated control

Technology does not remove cost.

It transforms:

> human labor → machines → energy + maintenance + logistics

---

## 38. Environmental Consequences of Industry

Industry changes the world.

Potential consequences include:

- Tailings
- Slag
- Smoke
- Wastewater
- Deforestation
- Soil disturbance
- Erosion
- Water contamination
- Habitat loss
- Noise
- Heat

These consequences create engineering demands such as:

- Settling ponds
- Drainage
- Water treatment
- Chimneys
- Retaining walls
- Waste storage
- Reclamation
- Ventilation

Environmental effects are gameplay systems, not morality meters.

---

## 39. Failure and Recovery

Failure should create new problems rather than simply end the game.

Examples:

- A collapsed mine may require rescue, drainage, and re-excavation.
- A failed harvest may force hunting, trade, rationing, or migration.
- A factory fire may destroy production but leave recoverable materials.
- A disease outbreak may reduce labor and livestock.
- A bridge washout may isolate a settlement.
- A dead breeding animal may force the player to acquire new genetics through trade.

Recovery should be expensive but usually possible.

The strongest stories should emerge from systems interacting under stress.

---

## 40. Progression Eras

### Age 0: Wilderness

Core challenges:

- Fire
- Shelter
- Water
- Foraging
- Hunting
- Stone tools
- Basic clothing

### Age 1: Settlement

Core developments:

- Pottery
- Agriculture
- Food preservation
- Animal taming
- Permanent storage
- Charcoal
- Basic trade

### Age 2: Copper

Core developments:

- Prospecting
- Mining
- Smelting
- Casting
- Metal tools

### Age 3: Bronze

Core developments:

- Alloying
- Better tools
- Larger mines
- Improved agriculture
- Mechanical workshops

### Age 4: Iron

Core developments:

- Bloomery iron
- Forging
- Mine engineering
- Structural construction
- Larger settlements

### Age 5: Steel

Core developments:

- High-temperature furnaces
- Improved metallurgy
- Machine tools
- Precision components

### Age 6: Steam

Core developments:

- Boilers
- Engines
- Pumps
- Rail transport
- Mechanized factories

### Age 7: Electricity

Core developments:

- Generators
- Motors
- Transformers
- Electrical distribution
- Electrochemistry

### Age 8: Industrial Chemistry

Core developments:

- Acids
- Fertilizers
- Petroleum processing
- Advanced ore processing
- Polymers
- Large chemical plants

### Age 9: Precision Industry

Core developments:

- Advanced machine tools
- Bearings
- Instrumentation
- Automation
- Electronics

### Age 10: Advanced Industry

Potential developments:

- Nuclear power
- Advanced alloys
- Semiconductors
- Computer control
- Highly automated production

Technology beyond this point can be defined later.

---

## 41. Progression Philosophy

Technology should generally become available because the player develops the physical capability to perform it.

Avoid progression that depends mainly on abstract unlocks.

For example:

A steam engine becomes possible because the player can produce:

- Pressure-resistant metal
- Accurate cylinders
- Valves
- Boilers
- Suitable fuel
- Reliable water supply

Electric power becomes useful because the player can produce:

- Conductors
- Insulation
- Generators
- Motors
- Transformers
- Control equipment

The production chain itself is the technology tree.

---

## 42. Challenge Philosophy

The game should be difficult because the world is interconnected, not because every individual action is tedious.

Good difficulty:

- Seasonal planning
- Resource competition
- Geological uncertainty
- Dangerous engineering
- Logistics
- Maintenance
- Labor constraints
- Ecological consequences
- Industrial bottlenecks
- Tradeoffs
- Recovery from failure

Bad difficulty:

- Excessive clicking
- Giant arbitrary recipe chains
- Repetitive manual feeding of mature systems
- Unreadable random failures
- Hidden penalties with no observable cause
- Constant trivial survival chores after they should be solved

---

## 43. Player Information

Complex systems must explain themselves through the world.

Examples:

- Cracks and creaking indicate structural stress.
- Crop discoloration indicates nutrient or water problems.
- Animal body condition indicates nutrition.
- Smoke color indicates combustion quality.
- Machine noise indicates wear.
- Gauges communicate pressure and temperature.
- Ore texture and host rock provide geological clues.
- Water turbidity indicates erosion or contamination.
- Worker behavior and complaints indicate labor problems.

Advanced instruments improve precision, but basic problems should usually have visible symptoms.

---

## 44. Long-Term Player Fantasy

The game should allow a recognizable transformation.

At the beginning:

- The player fears rain.
- Food is uncertain.
- A broken tool matters.
- A wolf is dangerous.
- Copper is precious.
- Winter is a major threat.

Later:

- Roofs and drains control rain.
- Farms and storage control food.
- Workers and workshops produce tools.
- Managed ecosystems and guards control wildlife risk.
- Mines provide industrial quantities of metals.
- Heating and logistics make winter manageable.

Eventually:

- Rivers are dammed.
- Railways connect settlements.
- Mines reach deep deposits.
- Livestock populations have been deliberately bred for specialized roles.
- Workers operate factories.
- Power grids connect industrial districts.
- Chemical plants refine materials unavailable to primitive societies.
- The player manages flows of matter, energy, labor, and information across an engineered landscape.

The world never becomes inert.

The player's relationship with it changes from:

> survival

to:

> understanding

to:

> organization

to:

> control

to:

> optimization.

---

## 45. Design Test

Every proposed mechanic should pass most of these questions:

1. Does the player perceive its causes or effects?
2. Does it create a meaningful decision?
3. Does it interact with another major system?
4. Does it create or modify a source, sink, constraint, or risk?
5. Can the player eventually mitigate or automate it?
6. Does technology transform the problem rather than simply delete it?
7. Can failure be understood and recovered from?
8. Does it make the world feel more coherent?
9. Does it avoid unnecessary repetitive input?
10. Does it remain relevant at an appropriate stage of progression?

Mechanics that fail these tests should generally be simplified or removed.

---

## 46. Scope Boundary

This document defines gameplay, progression, simulation behavior, and player-facing mechanics.

It intentionally does not define:

- Engine architecture
- Chunk structure
- Rendering
- Threading
- Simulation scheduling
- ECS usage
- Persistence implementation
- Networking architecture
- Physics implementation details
- Performance architecture

Those belong in separate technical design documents.
