# Text Play

**Role:** Headless player interface and agent-operated gameplay review.

Deep Hearth has a terminal play client over the **production** world, player control, simulation, observation, and persistence paths. It is for playing, discovering friction, and tracing consequences. It does not replace mechanical contracts or invent alternate gameplay rules.

## Launch

From the Deep Hearth directory:

~~~text
cargo run --locked --bin text-play -- --seed 24318
cargo run --locked --bin text-play -- --seed 24318 --script path/to/commands.txt
cargo run --locked --bin text-play -- --load campaign_01
~~~

The default seed is 24318. Other seeds produce reproducible openings. The seeded opening uses canonical generated terrain and the authoritative finite wilderness trail, admits a normal survival player, and generates surrounding chunks on demand. The opening may include berries, nuts, water, timber, stone, and real kobold residents. Unobserved resources remain hidden.

## Commands

- **look [direction]**: condition, visible resources, water, kobolds, locally materialized framing, smells and sounds. Sightings include relative offsets, not pathfinding. Optional directions use the bounded player-eye block ray: **n**, **s**, **e**, **w**, **up**, **down**, and **n-down**, **s-down**, **e-down**, **w-down** (angled downward to reach neighboring floor faces). Angled directions aim; they are not movement commands.
- **status**, **inventory** (**inv**), **gather**: inspect health, Celsius weather, selected hand, stance/foot elevation, motion, scheduled work, carried equipment, and numbered local storage stacks without spending time. Survival warnings use authored thresholds; an active local fire reports its finite stored warmth.
- **n**, **s**, **e**, **w** optionally with control ticks: walk via the real continuous body controller. **sprint e [ticks]** and **sneak e [ticks]** apply the authored sprint and crouch-walk modes, with any cardinal direction. **crouch [ticks]** crouches in place; **jump** or **jump e [ticks]** applies genuine vertical impulse, optionally with horizontal motion. Every movement command is bounded to 240 ticks, stops on the first voxel crossing or important event, and releases its held input afterward. One tick is 50 ms.
- **wait [ticks]** or **wait work**: neutral 20 Hz stepping. Work waits use the current authoritative completion schedule, capped at 7200 ticks per command. Both stop for significant visible changes, actual outcomes, or survival warnings, so repeating may be necessary. Default is 72 ticks (3.6 seconds); **wait 10** lets walking momentum settle.
- **gather 1** or **gather 1 100**: attempt a listed exact-local resource, using its numbered row and optionally whole grams. Physical availability, ongoing work, and storage admission are validated by production.
- **recipes**, **recipes all**: show carried-input manual recipe choices or the complete authored recipe catalog. Rows use stable process IDs, current compatible input counts, tool requirements, and conserved products. Catalog visibility does not authorize work.
- **craft PROCESS_ID [batches] [from STACK] [with TOOL]**: start a manual recipe through production-backed personal crafting. Default is one batch. Freshness-sensitive inputs require a numbered carried stack (**from 1**); optional or required tools use numbered carried equipment (**with 1**) from **inventory**. Admission rejects invalid requests without mutation. Completion deposits exact physical outputs.
- **gear** and **gear all**: list currently material-backed equipment assembly choices and the full authored assembly catalog, with exact missing-material requirements and local stationary gear. **assemble DEFINITION_ID** consumes carried parts and constructs a stationary tool. **take N** picks up a local tool; **equip N** holds the numbered carried tool; **recover N** disassembles a local stationary tool into its recoverable parts. Material constraints, location, capacity, condition, and attention remain production-owned.
- **storage**, **storage all**, **place-store DEFINITION_ID**: inspect placeable storage, inspect all authored storage and blockers, and construct an enclosure at the player's actual grounded voxel from carried parts. **stash CONTAINER STACK** and **withdraw CONTAINER STACK** move the maximum physically fitting amount between a numbered local container and a numbered material stack, under canonical access/capacity checks. Inventory lists both sides; remote containers cannot be accessed.
- **fire**, **build-fire**, **feed-fire FIRE STACK**: inspect local campfires and fuel charges, construct a stone-ring fire from 1 kg of carried stone, or feed a local fire from a numbered carried stack. One charge consumes 1 kg of timber log (12 MJ) or 500 g of wood chips (5 MJ). Fuel and energy remain finite, and warmth depends on the real local lit fire.
- **use [direction]**: contextual secondary action. An empty hand may gather or drink when the local context allows it. Selected food is eaten even while at water; use **empty** first to drink. Selected logs can form timber framing when aimed at a neighboring floor with **use e-down**; selected boards need an authored supported structural target. These start real timed work and may buffer until the next slow tick. Selected materials also support other authored placement/combustion.
- **break**, **shove**, **strike**, **prepare**, **release** with optional direction; **cancel**: canonical pointed primary actions.
- **slot 1** through **slot 9**, **select 1**, **empty**, **drop**: quick bar, numbered carried stack binding, clear selection, and ordinary active drop.
- **respawn**: after death, use the real validated respawn path. The new carried pack is empty; the old pack and dropped tools remain recoverable in a death cache at the reported location. Attempting this while alive is rejected without changing state.
- **save campaign_01**, **quit**: save the current validated world to a *new* slot or end play. A save is not automatically created when quitting.

An input file passed to --script has one command per line. Blank and # comment lines are ignored; the transcript echoes each command and its actual result. A rejected action reports its production error, not a fabricated success. Work may require further waits, and unrelated visible events can interrupt waiting early.

An ordinary flint-crafting example, on seed 24318, is: **e 20**, **wait 10**, **s 20**, **wait 10**, **s 20**, **wait 10**, **gather 3 300**, **wait work**, **recipes**, **craft 99**, **wait work**, **inventory**. It consumes 300 g of flint nodule and produces 220 g of knapped flint cutting head and 80 g of flakes.

To make and use a flint woodworking knife from the same seed: move to the timber log at (10,2,9) with **e 20**, **wait 10**, **e 20**, **wait 10**, **s 20**, **wait 10**; use **gather 1 1000**, **wait work**, **craft 8**, **wait work** for the handle. Move **w 20**, **wait 10**, **s 20**, **wait 10** to flint; use **gather 3 300**, **wait work**, **craft 99**, **wait work**, **gear**, **assemble 58**, **take 1**, **equip 1**. The assembled 420 g knife can replace hand timing in compatible recipes such as **craft 14 with 1**, with actual condition wear.

To make a 10 kg field box from timber: gather 2 kg at (10,2,9), use **craft 14 2**, **wait work**, **craft 27**, **wait work**, then move to the clear (9,2,10) site with **w 20**, **wait 10**, **s 20**, **wait 10**. **storage** shows whether the site is placeable; **place-store 5** creates the enclosure. **stash 1 1** puts its first carried stack into the box; **withdraw 1 1** retrieves its first local stack. The timber-source site is resident-occupied and correctly rejects placement.

For a campfire: from (8,2,8) go east three voxel crossings, braking with **wait 10** after each, then **gather 1 1000**, **wait work**, **build-fire** at the stone source. Retrieve 1 kg of timber from the (10,2,9) source and return to the fire, then **fire**, **feed-fire 1 1**, **status**. A lit fire consumes finite fuel and can restore cold-weather comfort. For a timber post, gather 3,250 g of log, select its inventory stack, **look e-down**, **use e-down**, then wait for the buffered construction and **wait work**. **look** shows nearby materialized framing.

## Save and scope

save NAME writes saves/NAME.sav as JSON through the current exact-schema SaveEnvelope. --load NAME validates the entire persisted state before the player resumes. Slot names are 1–48 ASCII alphanumeric, hyphens, or underscores. A slot never intentionally overwrites an existing save. The text client is responsible for local file IO; the core simulation remains headless and pure.

The text interface covers an **opening wilderness slice with manual crafting, tool assembly, local storage, campfire warmth, basic pointed framing and 20 Hz body movement**, not a complete powered-crafting interface, industrial progression, full building-plan UI, spatial rendering, or sound reproduction. It cannot judge first-person motion feel or visual quality. Its observation model is deliberately limited to existing actor-local projections, never direct access to hidden geology, future events, or all remote resources.

Use text play to find unexpected obstacles, confusion, questionable pacing, missed cues, and ordinary abandonment; then add focused owner tests for confirmed production defects. Maintain the automated harness for bounded system regression and comparison evidence, rather than making a scripted policy the default gameplay reviewer.

See [STATUS.md](STATUS.md), [GAMEPLAY_EVALUATION.md](GAMEPLAY_EVALUATION.md), and [TESTING.md](TESTING.md) for scope, evidence rules, and verification.
