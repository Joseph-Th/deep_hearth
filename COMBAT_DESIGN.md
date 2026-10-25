# Combat and Kobold Cooperation

**Role:** Combat experience and integration specification. This document describes intended behavior; [STATUS.md](STATUS.md) is the implementation and reachability authority. Mechanical decisions must remain compatible with [GAME_DESIGN.md](GAME_DESIGN.md) and [TECHNICAL_DESIGN.md](TECHNICAL_DESIGN.md).

## Comparative findings

Minecraft offers exceptionally direct left-click interaction, readable impact reactions, and useful quick-access switching. Its Java attack-charge mechanic restrains click spamming, but a clock-based damage multiplier alone often feels like waiting, while target damage-immunity windows make faster weapons lose contacts. Strong shields and critical-hit movement recipes create narrow dominant patterns instead of rich positional decisions. The 2019–20 combat tests experimented with held attacks, miss recovery, reach, and shield thresholds, illustrating that responsiveness, defensive counterplay, and attack tempo are separate problems.

Vintage Story contributes valuable grounded survival, tangible weapon reach, meaningful protective equipment, and the possibility of crafting varied armament. But its current melee can be dominated by retreat-kiting and knockback, while some players report weak telegraphs, surprise encounters, long-feeling input latency, and enemies whose durability does not match their danger. Modded directional combat adds variety but may punish confined-space fighting by repeatedly colliding weapon arcs with caves. A complex weapon system must not force players to memorize elaborate moves just to defend a camp.

These are design observations, not a claim that every player agrees. References:

- [Minecraft melee and cooldown mechanics](https://minecraft.wiki/w/Combat), [experimental combat variants](https://minecraft.wiki/w/Java_Edition_Combat_Tests)
- [Vintage Story combat basics](https://wiki.vintagestory.at/index.php/Combat), [weapon comparison](https://wiki.vintagestory.at/Guide%3AAdvanced_combat)
- [Vintage Story developer combat objectives, June 2026](https://www.vintagestory.at/blog.html/news/anego-studios-development-update-june-2026-r446/)
- [Player feedback on retreat-kiting](https://www.reddit.com/r/VintageStory/comments/1w8k4s6/newish_players_thoughts_on_combat/), [concerns about surprise spawns](https://www.reddit.com/r/VintageStory/comments/1w8nocj/i_cant_be_the_only_one_who_thinks_the_enemies_in/), [confined-space weapon collisions](https://www.reddit.com/r/VintageStory/comments/1r09qq9/combat_overhaul_hate/)

## Operating principles

**Easy entry, deep decisions.** Primary attack is useful immediately. Direction/aim, movement, reach, timing, positioning, shield facing, choice of weapon, and preparing the terrain add depth. There is no mandatory combo vocabulary or hidden optimal click cadence. Holding attack should eventually repeat valid attacks, but a tapped attack should be at least as responsive; a missed swing has a bounded cost, never permanent lockout.

**Read before react.** An actor prepares and commits to an attack before the damaging contact, with an audiovisual tell and a visible line of threat. A dodge or guard is possible within a practical reaction window. Attacks from behind require actual quiet approach, blocked sensing, or distraction; ordinary enemies have locomotion sounds, visible equipment, and no arbitrary adjacent respawn. Wounds and defensive pressure have readable local feedback.

**Bodies, not abstract reach bubbles.** Sample the weapon's actual damaging volume along one continuous, deterministic control-time trajectory against generated terrain, structure and actor bodies. The *first* valid collision owns the contact. A thin post should not block an entire sword sweep if the physical volume passes alongside it. A solid wall cannot be attacked through. Tight tunnels favor thrusting weapons; slash arcs that clip stone degrade effectiveness without trapping controls in an endless bouncing animation. Arcs and point traces should use shared fixed-point spatial primitives.

**Consequences beyond HP.** Resolve energy delivery and coverage into penetration, blunt trauma, cuts, stability loss, bleeding and pain, with bounded permanent outcomes. Ordinary hits do not all knock targets backward. Footing and shield orientation alter outcomes. Armor reduces harm through physically supported material, geometry, quality, condition, padding and encumbrance; no unexplained absolute tier immunity. Equipment wear becomes repairable damage or recoverable residual matter through its owner.

**Fear matters.** Most wild and lightly armed beings prefer food and survival to a fight to the death. Demonstrating reach, wounding, pinning an escape, darkness, numbers and recent losses affect confidence. A frightened individual retreats over real traversable routes, can drop held matter, yield, hide, bargain or disengage. Heavy temple castes may stand their ground for specific duties but must not have foreknowledge of distant raids.

**Scale adds dependency rather than magic damage.** Craftsmanship, metallurgy, maintenance, formation discipline, ammunition supply and command throughput change the *options* available, not merely a rising damage integer. An advanced weapon can still be defeated by terrain, surprise, exhaustion, empty magazines, broken supply or poor morale.

## Player control contract

A coherent melee action has **request → preparation → contact → recovery**. The first contact of a quick light attack should be prompt; committed heavier attacks trade that responsiveness for reach, impulse, armor defeat or disruption. Each weapon authors explicit preparation, active contact, recovery, stamina/exertion, effective range, collision geometry, and movement constraints. Windows are integer 20 Hz control ticks, not client frame timing.

- **Primary:** tap or hold attack with the selected ready weapon or empty hand. The selected tool/material is not automatically a weapon: eligibility comes from an authored weapon profile bound to physical equipment and custody.
- **Secondary:** hold guard with a valid hand/shield, or contextual use. Guard has startup, facing coverage, stability cost, and a finite guard-break threshold; it is not a zero-cost invulnerability toggle.
- **Movement:** sidestep, sprint, crouch, and jump use existing continuous body physics. Backpedaling has a material mobility/footing tradeoff. Dodging is displacement with a commitment, not invincibility frames. An attacker cannot endlessly gain reach by jumping while retaining full stability.
- **Contact:** the player-eye ray is a targeting aid, not a damage volume. Contact physics resolves world obstruction and actual weapon/head/body surfaces independently. If the entity is hidden or world authority is missing, no hit is possible.
- **Timing:** buffer a *single* requested follow-up within a short authored window; do not enqueue arbitrary commands. Switching gear cannot erase the recovery of a committed swing. The target's stagger does not create a general damage-immunity interval that nullifies fast weapons.
- **Feedback:** distinguish out-of-reach, obstructed, missed, glanced, guarded, staggered, injured and incapacitated. Show posture/breath instead of requiring numeric stamina spreadsheets.

### Immediate integration

The canonical `PrimaryControlIntent::shove` uses an **empty-hand nonlethal contact** with a 1.6 m ray and 16-control-tick recovery. `PrimaryControlIntent::strike` requires a **real selected and carried weapon** whose equipment definition declares a melee profile. The rough wooden club uses a 1.5 m reach, 22-control-tick recovery and 3,000 ppm condition wear per swing; the hafted stone spear uses a 2.75 m reach, 28-control-tick recovery and 4,500 ppm wear. Both are assembled from exact existing wood/stone commodities, can be picked up and serviced through normal equipment custody, and expend recovery and (when armed) condition on misses. Every attempt also spends exact, finite actor survival reserves: unarmed 90 J/6 µL, club 180 J/12 µL, spear 240 J/16 µL; prepared release costs twice its weapon's ordinary exertion. Too little energy or hydration rejects the attempt atomically. A weapon committed to a prepared strike cannot be disassembled until the stance is cancelled, preserving loadable real-equipment identity. Weapon switching does not cancel committed recovery. A wall stops the common bounded eye-ray, missing terrain beyond weapon reach is not consulted, and a co-located pair at the den is ambiguous rather than silently choosing a victim. Melee contact checks the authoritative generated eye voxel first, allowing an unambiguous resident sharing the player's cell to be struck before a more distant resident; the ordinary block and air-placement ray operations still require an entered face before selecting a target.

Contact reports the struck kobold's actual foot position and induces local caution. The club delivers up to 280,000 ppm of reversible balance disruption per hit (the spear 150,000; empty-hand shove 80,000), scaled down by actual equipment condition. The scout and watchman each retain separate balance in trusted state. Below 350,000 ppm, a resident is staggered and cannot move over its next physical patrol edge until it recovers by 120,000 ppm per five slow simulation ticks. The sighting projection reveals balance only for these opening residents; a distinct `Staggered` contact outcome reports remaining balance. This is a temporary footing model with no injury, knockback, or invented matter.

An optional **committed prepared strike** adds a ten-control-tick windup to the existing immediate quick strike. `PrepareStrike` records the selected weapon and original direction in combat state; `ReleaseStrike` must remain within approximately 37 degrees of the original aim and cannot fire early. A valid release re-traces the currently generated and physically obstructed world, delivers 150% of normal balance disruption, doubles equipment wear and adds twelve 20 Hz recovery ticks; `CancelStrike` discards the stance with no new contact, wear or recovery reset. The windup persists through save/load with bounded timestamps and validated fixed-point facing. Switching or dropping the weapon prevents release until the same carried equipment is restored or the stance is cancelled. Preparation itself spends no reserves, but release spends doubled authored exertion; windup can still be held indefinitely, and no heavy attack damage volume or player guarding is claimed. Validated hits also reject stale world, structure and active player-work revisions before applying wear, recovery or kobold balance.

Contact can still force real retreat over available patrol edges once the resident is steady enough. Save/load, stale commits, material accounting, occlusion, differing reach and cooldown continuation have focused executable coverage. This **primitive armed-contact slice is still nonlethal** and does not claim hit-volume animations, wound physics, armor, guarding, projectiles, lethal attacks, defensive kobold strikes or recruited allies. Later damaging attacks must use swept weapon geometry and proper physical harm rather than equating the provisional eye-ray with a damage hitbox.

## Technological progression

| Stage | Battlefield decisions | Industry and custody constraints |
| --- | --- | --- |
| **Survival** | Bare hands (push/disengage), wooden club, knapped stone edge, fire-hardened wooden spear, thrown stones; poor armor, short reach, ambush or escape often safest | Stone/wood shapes must be crafted and remain finite, weapon condition and hand availability matter |
| **Settled stone/copper** | Stone-tipped spear, wooden shield, sling, bow, copper heads, improved sidearms; guarding lines, ranged threat and cover | Haft bonding, fletching, strings, leather/hide, copper supply, repair skills, limited ammunition |
| **Bronze/iron** | More reliable edges and armor penetration, spears in formation, axes against shields, polearms, helmets, mail, crossbows; advantage from spacing and unit cohesion | Casting, forging, heat treatment, padding, spare heads, trained crews, fatigue and provisioning |
| **Early gunpowder** | Slow match/wheel/flint firearms, smoke-concealed volleys, reload vulnerability, pikes protecting shooters, field guns with long setup | Saltpeter, sulfur, charcoal, safe powder handling, barrels, shot, primer, dry storage, blacksmithing and maintenance |
| **Industrialized war** | Breech-loaders, repeaters, automatic fire, trench cover, suppression, indirect artillery, communications, combined arms and engineering | Standardized ammunition, measured tolerances, rails/roads, fuel, spare parts, crew training, heat, barrel wear, misfires, transport and command reliability |

No instant technology-tier win: primitive weapons can deter a poorly supplied army in steep terrain, while good discipline can defeat superior individual equipment. Do not add industrial firearms as bare inventory actions without the production and supply chain that makes their advantage interesting.

### Damage and ammunition conservation

A damaging strike transfers only physically justified momentum/energy and authors condition loss. Flesh, blood, lost material and broken armor need conserved material custody before persistent lethal injuries are added. Projectiles are finite material bodies with origin, velocity, drag/impact and terminal custody; penetration can embed, ricochet, deform, fragment or lodge ammunition, never clone it. Gunpowder consumption and gases/heat require compatible material and thermal/chemical owners. Dry firing does not consume ammo; a dud may still retain recoverable material.

## Kobold coexistence, recruitment and command

A visible living kobold is a **person with agency**, not a recruit button or summoned follower. Villages and dens own finite food, tools and places to sleep. Scouts, skilled tinkerers, watchers, and hereditary temple castes have distinct needs and relationships; temple protectors do not automatically become player soldiers on contact. Different individuals may be wary, curious, neutral, allied or hostile according to witnessed conduct and actual local reports.

Recruitment requires a locally witnessed offer with understandable terms: safety, food/shelter, compensation or shares, work scope, duration and right to depart. Consent is explicit and reversible. A hungry resident can accept help without owing allegiance; threats, theft and assault reduce trust. A fair arrangement starts with food/repair trade or safe passage; trust and proven ability to provide create an opportunity for a voluntary labor or defense agreement. Terms are persisted with the resident's identity, location and accrued obligations; payment transfers *existing* stockpile or meal matter atomically through owner ingress/egress, never creates virtual rations.

An ally may **work** (haul, forage, repair, scout, guard) or **fight** (hold, escort, patrol, disengage) within real skill, equipment, capacity, terrain, daylight, food and risk constraints. Work/fight is not a binary permanent caste. Orders are requests with feasibility and risk assessment; an ally can refuse a suicidal attack, retreat, defect over repeated broken promises or temporarily stop work to care for injury. Give the player clear local status and reports, not global enemy positions. A small loyal team should be useful for watch rotations and escape cover long before a battle line exists. Specialized industrial squads require ammunition carriers, medics, armorers, trainers and supply crews.

**Do not implement a generic slave/ownership field** or an infinite recruit/mercenary vending machine. Agreements are bounded relationships between concrete actors. Noncombatants and surrendering opponents remain noncombatants.

## AI and military logistics

A defender needs **sensing → interpretation → intention → telegraphed action → physical movement/contact → recovery**. Locally observed light, sound, tracks, wounds and dropped items inform confidence. Alerts travel by shout, drum, messenger, horn, line of sight or later wire/radio with actual physical reach and time. Threat assessment should include ammunition, injury, number of allies, defensible positions, refuge and loyalty.

Squads have bounded attention and order latency. Formations are geometry and path obstruction, not an abstract damage buff. Suppression changes available movement and willingness to expose oneself, not automatic damage. Ranged combat requires projectile tracing and cover; explosive hazards must obey shared world consequences and conservation. Medical evacuation, wounded personnel, captives, surrender, replacement equipment and civilian safety matter as much as direct kill counts.

## Vertical proof requirements

A coherent combat slice requires the same proof route as other authoritative systems:

1. **Contact:** a strike through open space at one resident can hit; the same input into a stone wall, beyond real reach, through missing world truth, or at an ambiguous co-located pair cannot.
2. **Timing:** held attacks recover at authored control ticks; misses have bounded cost; one missed input cannot create repeated future effects; gear switching and slow-tick boundaries cannot reset recovery.
3. **Reaction:** a struck scout retreats over actual patrol edges and may abandon exposed cargo. A remote or unseen kobold does not react to information it cannot physically receive.
4. **Conservation:** no attack invents ammo, armor matter, equipment, food or salvage; later injuries and blood loss balance at their owner boundary.
5. **Continuation:** each contact/guard/morale/contract state survives save/load and resumes bit-for-bit on the next control/simulation step. Forged identity, future deadlines and unsatisfied obligations fail trusted load.
6. **Gameplay:** test a primitive camp defense, spear-versus-crowded-tunnel choice, shield-versus-spear decision, provisioned kobold escort, and ammunition-supply-dependent ranged defense before scaling to armies.

Choose the smallest *connected* slice before adding additional weapon catalogs. The game currently has no lethal combat, battle AI, armor, ranged ammunition or recruitment system, and this design does not imply their existence.
