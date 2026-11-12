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

- **look [n|s|e|w|up|down]**: current condition, visible resources, water, kobolds, smells and sounds. Sightings include relative compass offsets, not pathfinding. An optional aim uses the real bounded player-eye block ray.
- **status**, **inventory** (**inv**), **gather**: inspect health, Celsius weather, current hand, body motion, scheduled work duration, inventory and local gathering choices without spending time. Warmth warnings use the authored survival thresholds.
- **n**, **s**, **e**, **w** optionally followed by a count of control ticks: move using the real continuous body controller, up to 240 ticks. Movement stops after the first voxel crossing or an event. One control tick is 50 milliseconds.
- **wait [ticks]**: neutral input for up to 7200 control ticks, stopping for significant player-visible changes, important outcomes, or survival warnings. Defaults to 72 ticks (3.6 seconds). **wait 10** releases movement and allows the body to brake before gathering.
- **gather 1** or **gather 1 100**: attempt a listed exact-local resource, using its numbered row and optionally a whole-gram amount. Physical availability, ongoing work, collision, and storage admission are still validated by production.
- **use [direction]**: contextual secondary action. An empty hand may gather or drink when the local context allows it. Selected food is eaten even while standing at water; use **empty** first to drink. Selected materials support authored placement/combustion.
- **break**, **shove**, **strike**, **prepare**, **release** with optional direction; **cancel**: canonical pointed primary actions.
- **slot 1** through **slot 9**, **select 1**, **empty**, **drop**: quick bar, numbered carried stack binding, clear selection, and ordinary active drop.
- **respawn**: after death, use the real validated respawn path. The new carried pack is empty; the old pack and dropped tools remain recoverable in a death cache at the reported location. Attempting this while alive is rejected without changing state.
- **save campaign_01**, **quit**: save the current validated world to a *new* slot or end play. A save is not automatically created when quitting.

An input file passed to --script has one command per line. Blank and # comment lines are ignored; the transcript echoes each command and its actual result. A rejected action reports a typed production outcome, not a fabricated success. Work can remain active after a command, and may require further wait calls to complete.

## Save and scope

save NAME writes saves/NAME.sav as JSON through the current exact-schema SaveEnvelope. --load NAME validates the entire persisted state before the player resumes. Slot names are 1–48 ASCII alphanumeric, hyphens, or underscores. A slot never intentionally overwrites an existing save. The text client is responsible for local file IO; the core simulation remains headless and pure.

The text interface covers an **opening wilderness slice**, not a complete crafting UI, full industrial progression, spatial rendering, or sound reproduction. It cannot judge first-person motion feel or visual quality. Its observation model is deliberately limited to existing actor-local projections, never direct access to hidden geology, future events, or all remote resources.

Use text play to find unexpected obstacles, confusion, questionable pacing, missed cues, and ordinary abandonment; then add focused owner tests for confirmed production defects. Maintain the automated harness for bounded system regression and comparison evidence, rather than making a scripted policy the default gameplay reviewer.

See [STATUS.md](STATUS.md), [GAMEPLAY_EVALUATION.md](GAMEPLAY_EVALUATION.md), and [TESTING.md](TESTING.md) for scope, evidence rules, and verification.
