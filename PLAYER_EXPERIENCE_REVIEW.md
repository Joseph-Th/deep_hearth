# Player Experience Review

**Evidence:** local gameplay reports and replayable simulation, October 2026.
**Scope:** gameplay and evaluation quality, not implementation authority. Current feature
reachability remains in [STATUS.md](STATUS.md); intent remains in [GAME_DESIGN.md](GAME_DESIGN.md).

## What the player gets to be

The distinctive fantasy is **a resourceful field engineer taking ownership of a stubborn
physical world**. The player notices clues, anticipates problems, chooses investments, works with
scarce materials, learns from failure, and eventually creates physical systems that multiply their
personal reach. The reward is *earned agency*: a roof makes exposure manageable; a tool changes
which rock can be mined; a cache changes what can be left unattended; a flywheel and machine
return time for other work. Survival, crafting, prospecting, and mechanization are means to that
identity, not the identity themselves.

Successful play should repeatedly move from an observed inconvenience or opportunity to a
self-chosen improvement that changes what the player can do next. More steps in a recipe chain,
larger ledgers, longer waits, and more forced survival tasks do not automatically serve it.

## What the harness actually showed

| Episode | Observed experience | Player-design conclusion |
| --- | --- | --- |
| Maintained rested opening | 500 g berries collected in 1.3 minutes; 5.10 kg remained; the player deferred water, storage and tools. At resting rates the hunger and thirst warnings were about 45 and 24 hours away. | Defer is rational. Gathering resources that lack a purpose would not improve the opening. |
| Same actor's continuation | The next meaningful intervention followed **10.7 minutes without a player action**: 89% of elapsed time to warmth discomfort. A local stone-and-timber fire restored warmth; preparing it before the warning preserved much more comfort. | This is not an engaging twelve-minute sequence. The forecast enables useful planning, but the opening still needs an aspirational project or discoverable local problem rather than a manufactured starvation timer. |
| Long-stay camp | Timber, stone, tools, supports and a finished cover create a material, spatial shelter. Unheated exposure loss changes from 1,000 to 300 ppm/tick; unfinished construction grants no cover. | A physical project gives a tangible advantage and creates a genuine fuel-versus-construction alternative. This is close to the desired fantasy. |
| Scavenger encounter | A scout may relocate unattended food, can be driven away by presence and local firelight, and leaves actionable route evidence. Stolen matter stays recoverable. | World inhabitants become interesting through custody and risk, not just damage. Most such encounters are currently sampled separately from the maintained opening. |
| Fieldwork | A 33 kg ore demand met a localized 0..5 kg estimate; first recovered ore arrived only after about 17 minutes of setup/search/work. Six further searches cost another 324 ticks and still left 15.88 kg unmet. A matched assay reduced uncertainty at a real attention cost. | Exploration, uncertainty, adaptation and sunk-cost decisions exist. A nominal order cannot turn a small local seam into an adequate supply. Make this limitation evident before large tool investment, without leaking hidden neighboring geology. |
| Generated exact-local expedition | Surveyed barren and productive sites, walked 12 edges, mined sixty batches, hauled twice under a 25 kg personal capacity, returned 30 kg, conserved matter. | Persistent locality and loading produce decisions which the spatial-proxy versions cannot establish alone. |
| Liberation kit acquisition in generated terrain | An 8 kg return haul on organic seed 0x212772DBE279D35B arrived at a narrow passage off-center and required a real lateral maneuver. It took nine slow ticks to traverse an eight-edge route. | A shortest-path estimate is a floor, not actual player travel time. The witness now executes physical correction without teleportation, charges its elapsed time, and identifies that extra tick separately instead of declaring the world unplayable or hiding the cost. |
| Shared settlement workshop | 160 productive batches, ten charges, 935 ticks delegated; 70 ticks of drawplate work and 12 ticks of drill-bit work overlapped, but 853 delegated ticks had no observed productive use. | Mechanization meaningfully releases player attention, yet the evaluator mostly proves a resource-economic advantage rather than showing how that liberated time changes the lived world. |
| Direct-use opening | Contextual hand actions queue up to the next slow boundary, with observed wait around 3.15 seconds in a control sample. | The familiar interaction can feel slow even when the owner transaction is perfectly correct. Control-rate work admission exists elsewhere; a consistent direct-use timing contract is worth a deliberate subsystem-wide change. |

The maintained wilderness replay is runnable with:

```text
python ci.py report --scope wilderness --verbose --focus-seed 0x57494C4445524E53 --focus-episode maintained/rested/stay-mobile
```

Use `python ci.py report` for the broader tradeoffs. These are executable player experiences
only where an episode actually executes them; the harness also contains capability probes,
modeled handoffs, and independent worlds. Those are useful but **do not prove one uninterrupted
character grew from the first berry patch into the foundry**.

## What works and what does not yet

**Works:** exact finite ownership, material conservation, differentiated labor and transport,
observable equipment economics, local shelter, heat and light, scout reactions, mineral
information, depletion, recovery and retooling, maintenance, and real machine delegation.
The strongest moments cross two or more owners in one persistent scenario: a fire is warmth,
fuel demand and deterrence; a roof consumes scarce timber but reduces future fuel dependence;
a mining tool is preparation expense that may not be justified by the found deposit.

**Not yet convincing:** the maintained starting actor can be full, warm and supplied with no
immediate personally meaningful project. A bounded six-edge opening and abundant local food
offer too little discovery to sustain a field-engineer identity. Several downstream challenges
are supplied as externally selected work orders or staged world setups. Passing samples alone
therefore overstate the continuity, curiosity and initiative of ordinary play. Real 20 Hz
movement does not by itself fix slow contextual-action feedback.

The present thermal test has a strong calibration warning: outdoor comfort turns chilly in
about eleven minutes while rested hunger and thirst are far away. With no equivalent
player-experienced weather or thermal context in that episode, the cold timer is a convenient
way to force a fire, not yet a convincing environmental cause. Do not rebalance it in isolation;
connect environmental conditions, personal protection and shelter first so a temperature
decision expresses physics instead of a naked countdown.

## Design changes and next highest-value experiment

The gameplay read surface now exposes conservative *conditional* resting hunger/thirst and
cover-aware unheated warmth runways. The wilderness harness reads those exact production
assessments and prints its action-free time as dead time. A completed roof changes the actual
forecast; unknown terrain does not invent cover. This increases both useful player planning
and the honesty of automated evaluation. It does not create a new world objective.

The next decisive experiment should follow **one generated-world player**, without resetting
to an endowed workshop, from visible local clues through a player-chosen useful capability:
discover a physically located resource, acquire/make the right tool, face an unforeseen shortfall
or scavenger response, establish custody or shelter, then return to exploit the improved
position. A good outcome is not compulsory completion; a legible, recoverable change of plan
is equally valuable. Report time spent choosing/acting/traveling versus passive waiting.
Then extend the same state into a genuinely unattended machine task while the player does
something productive elsewhere, with real travel, materials and conservation.

Do not make the actor busy merely for coverage. If the world gives a rational actor nothing
worth doing, that is evidence to improve **world opportunities**, not to lower hunger reserves
or fabricate a high-pressure work order.
