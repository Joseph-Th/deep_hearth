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
| Maintained rested opening | 500 g berries collected in 1.3 minutes; 5.10 kg remained; the player deferred water, storage and tools. At resting rates the hunger and thirst warnings were about 45 and 24 hours away. | Deferring unnecessary capital is rational. The observed junction also offers a distinct durable protein opportunity. |
| Same actor's forward provisions | Using the exact-local, revision-bound gathering food-value projection, the actor verified current source freshness, took 250 g nuts (1,750 kJ and a longer authored fresh shelf-life than berries) and returned to spawn. This required 1.8 minutes of actual gathering and travel. Later, a scout removed 100 g from the unguarded source. The player saw an adjacent parcel, reclaimed the exact 100 g with two more physical travel ticks, and retained conservation and trusted save/load. | A recoverable opportunity now appears inside one *continuous* character story, rather than only in a separate scout test. It makes finite food, actual spoilage, nutrition, scouting and movement causally related. |
| Same actor's thermal/infrastructure choice | The reactive branch observed the cold warning after **8.9 minutes of passive time** (74% of elapsed time to the warning), then returned, recovered food and made a fire. From the same post-provision state, a forecast-driven branch built its fire from actually gathered materials, **left it unattended during the scout patrol**, and used that window for real reconnaissance. The fire protected all 585 g of the remaining nut patch and yielded 974,500 ppm comfort instead of 802,000 ppm at the matched endpoint. It burned fuel earlier, leaving 957 versus 1,108 ticks of fire runway. | Advance planning saves a return chase and turns heat into infrastructure that deters wildlife. A fire-only matched branch proves the protection is due to illumination, not the player's presence at the source. The worksite only warms the player on return. |
| Unattended camp reconnaissance | The same proactive player walked spawn → forage → water → copper/clay outcrop and back using normal continuous-body control. A 10-tick hand inspection acquired positive local copper evidence, a timed collection secured **20 g of finite native copper**, and strict save/load preserved both knowledge and custody. The trip occupied 19 simulation ticks; the proactive branch still waited **6.4 minutes** after returning, against 8.9 minutes in the reactive branch. The idle fire-only comparison retained the same protected food. | The player is beginning to **be a field engineer** rather than a fire tender: investing in sheltering infrastructure creates freedom to learn and return with useful physical stock. Yet discovering one resource is not a lasting technological improvement; the game needs subsequent independently motivated projects and better opportunity discovery. |
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

**Not yet convincing:** despite now performing a paid reconnaissance and retaining physical
metal and acquired geology, the maintained starting actor has little to explore outside the
known bounded opening. **74%** of the elapsed time to its first cold warning is passive in
the reactive branch; even the forward-planning branch still has **6.4 minutes of idle exposure**
after its actual 19-tick reconnaissance. A six-edge opening and abundant local food offer too
little unscripted discovery to sustain a field-engineer identity.
Several downstream challenges
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

The gameplay read surface exposes conservative *conditional* resting hunger/thirst and
cover-aware unheated warmth runways, alongside exact-local gathering options with projected
direct-batch dietary energy, category, hydration tendency, source-history freshness and the
authored fresh shelf-life baseline.
The wilderness harness consumes those ordinary player-facing projections, executes one
self-chosen forward-provisioning action, and prints its active and passive time separately.
A completed roof changes the actual warmth forecast; unknown terrain does not invent cover.
These decisions improve player planning and the honesty of automated evaluation. They do
not solve world exploration.

The current maintained report now includes a fourth physical journey phase rather than
silently dropping the productive reconnaissance from its concise account. The evidence
is cross-checked against the same branch's canonical save, material custody and explicit
fire-only counterfactual. The report rejects inconsistent activity or metal claims; reduced
idle time is not inferred from elapsed fire duration alone.

The immediate material challenge is to move beyond the finite, pre-shaped opening graph:
**one generated-world player** should discover a physical clue or work opportunity not already
laid along the bounded starter trail, choose and make a useful tool, face a genuinely unknown
shortfall or route obstacle, and establish a lasting capability. The current post-opening
nut/scout/fire continuation proves a smaller local version of that reasoning loop; it does not
prove discovery at world scale. The next progression experiment should extend the same state
into a genuinely unattended machine task while the player does something productive elsewhere,
with real travel, materials and conservation.

Do not make the actor busy merely for coverage. If the world gives a rational actor nothing
worth doing, that is evidence to improve **world opportunities**, not to lower hunger reserves
or fabricate a high-pressure work order.
