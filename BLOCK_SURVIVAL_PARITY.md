# Block-Survival Mechanical Parity

**Role:** Comparative design and implementation audit against Minecraft's block-survival substrate.

This page is a comparison checklist, not current implementation authority. Use [`STATUS.md`](STATUS.md) for what
exists today, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended player experience, [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md)
for implemented semantics, and [`DIRECTION.md`](DIRECTION.md) for integration priority.

The reference game is the current stable Minecraft: Java Edition. This audit was verified against Java Edition
26.3 and the corresponding implementation descriptions at [How Java Minecraft Works](https://minecraftdocs.dev/).
Future work that depends on a version-sensitive Minecraft detail should refresh that detail rather than treating
this page as permanent external truth.

## Parity rule

Deep Hearth is not trying to clone Minecraft's content, code, balance, fiction, or implementation. It does need
the mechanical substrate that makes a first-person block-survival game immediately understandable.

The governing question for every foundational mechanic is:

> Can the player do the familiar thing at least as directly, and if Deep Hearth models it differently, does the
> different model produce a concrete benefit without removing the familiar verb?

The comparison uses these verdicts:

| Verdict | Meaning |
| --- | --- |
| `PARITY+` | The familiar mechanic exists or the current owner model is materially stronger while preserving the familiar action. |
| `PARITY` | The mechanic is equivalent enough for the block-survival shell. |
| `DIFFERENT+` | Deep Hearth should deliberately differ because its physical model gives a concrete benefit, while still preserving the familiar action grammar. |
| `GAP-P0` | Missing substrate blocks ordinary first-person block-survival play. Close before further breadth is treated as the main integration frontier. |
| `GAP-P1` | Missing familiar system materially weakens the first-day or sandbox loop, but does not block walking around and manipulating a basic world. |
| `GAP-P2` | Important later sandbox breadth. It can follow a coherent first-day shell. |
| `OUT-OF-SCOPE` | Minecraft has the feature, but Deep Hearth has a deliberate replacement or does not need that content family for mechanical parity. |

`Better` never means "has more fields." It means the model gives the player more coherent causes, decisions,
recovery, scale, or performance while remaining legible and usable. An internally sophisticated system that
removes a basic player action is a regression, not an improvement.

## Overall verdict

Deep Hearth is substantially deeper than Minecraft in several simulation domains, but it is not yet mechanically
at parity with Minecraft's basic sandbox substrate. Its strengths are downstream of a world shell that does not
exist yet.

Strong current foundations:

- exact matter custody, composition, temperature, provenance, particle state, preservation history, and
  conservation;
- finite fluids and energy stores, explicit losses, reservations, occupancy, and delayed-work custody;
- material-backed equipment, wear, service, upgrades, disassembly, and physical capability resolution;
- structural support, load, failure, and material-backed construction;
- deterministic owner boundaries, checked integer physics, strict trusted-load validation, and replayable work;
- renderer-neutral indexed textures where each authored texel is one byte before shared palette lookup, with
  pattern and palette-row deduplication;
- player physiology that already has stronger causal vocabulary than Minecraft's hunger-only survival pressure.

Foundational blockers:

- no general block mutation, rich block shapes, block updates, scheduled block behavior, or sparse voxel-to-owner attachments;
- continuous fixed-point body position, persisted horizontal/vertical velocity, 20 Hz horizontal walking control,
  gravity, falling, and swept full-cube collision now exist, but jumping, crouching, sprinting, swimming, and
  general shape collision do not;
- no camera/input/raycast action shell;
- no generic hold-to-break or contextual place/use pipeline over world blocks;
- no generic dropped-item world entity, hotbar, familiar stack projection, or container UI shell;
- no terrain generation, chunk streaming, biome/climate field, or general world discovery;
- no world lighting, sky light, darkness gameplay, weather, or environmental heat transport;
- no entity runtime for creatures, combat, projectiles, item entities, vehicles, or AI;
- no world-space fluid field or fluid flow;
- no graphics backend, section mesher, visibility system, audio, input adapter, or save-file storage adapter.

The first compact block-world storage/generation slice now exists, so the practical conclusion has narrowed:
Deep Hearth should stop using the bounded five-edge opening graph as the shape of future world interaction. It
remains useful evidence for existing owner interactions, but ordinary play still needs demand streaming and
continuous player control over the generated cells. Deeper industrial work should build on that substrate, not
continue to get farther ahead of it.

## 1. Runtime, time, and authority

Minecraft's server owns the world at a fixed 20 ticks per second. The client renders independently, predicts
selected actions, and is corrected by server authority. Deep Hearth now has a persisted 20 Hz direct-control
clock while retaining a slower exact accounting clock for its materially richer long-running simulation.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Authoritative world tick | 20 Hz, 50 ms game tick | Persisted `ControlTick` advances at 20 Hz; one canonical orchestrator runs a slow `SimulationTick` transaction every 72 control ticks | `DIFFERENT+` foundation | Put continuous movement/collision and direct world interaction on `ControlTick`; keep one canonical mutation path. |
| Calendar scale | Day/night is a world clock layered on the tick | Calendar remains 24,000 slow ticks per 86,400 represented seconds; the 50 ms control clock is independent | `DIFFERENT+` foundation | Keep direct-control cadence independent from accelerated calendar/slow physical accounting. |
| Long-running work | Furnaces, growth, effects, block ticks advance over many game ticks | Persisted jobs/work retain exact 3.6-second `TickSpan` schedules and can start only at exact control/simulation boundaries | `PARITY+` | Keep durable exact work; new genuinely responsive mechanics should use control-time schedules rather than fractionalizing every slow owner. |
| Client frame cadence | Independent of server tick | No client/render loop in project scope | `GAP-P0` | Adapter must render independently from simulation and interpolate authoritative transforms. |
| Prediction | Client predicts movement, breaking, inventory and selected interactions | No interactive client | `GAP-P1` | Singleplayer can begin without network prediction, but the adapter boundary should not prevent prediction later. |
| Server/client split | Real logical client/server separation even in integrated singleplayer | Headless core plus future adapters, no network protocol | `DIFFERENT+` | Do not copy the packet architecture prematurely. Preserve a clean command/read boundary so a client/server split can be added without rewriting simulation ownership. |
| Determinism | Seeded worldgen but runtime behavior includes RNG and thread-sensitive implementation details | Explicit deterministic continuation, stable ordering, no ambient RNG | `PARITY+` | Preserve this advantage. Introduce state-owned or position-derived PRNG streams when ecology/random ticking actually needs them. |
| Tick overload | Can fall behind or skip timing obligations in implementation-specific ways | Canonical tick semantics are not skipped by batch helpers | `PARITY+` | Keep semantic correctness as authority. Rendering may drop frames; simulation should not silently drop state transitions. |
| Background work | Chunk generation, lighting, meshing and IO use worker lanes | Core currently mostly synchronous ownership | `GAP-P1` | Parallelize derived/generative work only after owner boundaries exist. Deterministic commit order stays authoritative. |

### Time decision

The timing blocker is closed at the substrate level. `ControlTick` is authoritative at 20 Hz, persists through
save/load, and is advanced only by `advance_control_tick`. Every 72nd control step executes the existing
3.6-second `SimulationTick` transaction. Generated-world body gravity/collision now runs on those control steps;
the headless `advance_tick` helper fails closed there instead of skipping active control-rate effects. Trusted
load rejects incoherent clock pairs and unrepresentable terminal substeps.

This deliberate multi-rate design is stronger for Deep Hearth than forcing all industrial and physiological
accounting to 20 Hz. Existing slow work can start only on an exact 72-step boundary, validated schedule tokens
bind the exact control tick, and generated-world slow work additionally requires body ground contact and zero
horizontal body velocity. Responsive movement, targeting, breaking, combat windows, and similar mechanics belong
on control time. Horizontal walking now uses that path; the remaining P0 is demand streaming/access, richer
locomotion verbs, and direct world interaction rather than the clock substrate.

## 2. World address space, chunks, sections, and streaming

Minecraft stores a huge world in chunk columns and 16x16x16 sections. Each section palette-compresses 4,096 block
states and tracks cheap counters so empty or inactive sections can be skipped. Deep Hearth now uses the same
proven spatial granularity for authoritative storage while retaining wider coordinates and its own ownership model.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Integer voxel coordinates | Integer block positions | `VoxelCoord` uses checked-friendly signed `i64` axes | `PARITY+` | Keep the wider coordinate vocabulary. |
| World cells | Every generated/loaded position resolves to a block state | `WorldState::block_at` distinguishes generated air from absent chunk truth | `PARITY` substrate | Extend this read surface into collision/raycast/render snapshots without creating another world authority. |
| Chunking | 16x16 horizontal chunk columns | Persisted sorted 16x16 `WorldChunk` columns | `PARITY` | Keep unless measurement later justifies a different derived render partition. |
| Vertical sections | 16x16x16 block sections | Sorted 16-cubed `WorldSection` records; absent sections in a generated chunk are canonical air | `PARITY` | Reuse section bounds for later lighting, meshing and activity summaries. |
| Palette compression | Section-local palette plus bit-packed indices | Local `BlockStateId` palette with minimum-width packed `u64` indices; one-state sections store no index words | `PARITY+` | Keep the compact common case and add only measured summaries/sidecars. |
| Empty-section fast path | Counters allow all-air/tickless sections to skip work | Empty sections are omitted; stored sections maintain validated exact `non_air_count` | `PARITY` foundation | Add ticking/fluid/light/activity summaries only when those owners exist. |
| Heightmaps | Several heightmaps accelerate surface queries | Seeded terrain exists, but no persisted/derived height summaries | `GAP-P1` | Derive height summaries for sky exposure, spawning, visibility, and repeated surface queries. |
| Block entities | Sparse per-position extended objects for chests, furnaces, etc. | Equipment/stores are independent owner records with optional voxel locations | `DIFFERENT+` | Keep domain owners for consequential machine/storage state. Add a sparse voxel attachment/index that points from world cells to owner identities rather than duplicating owner state inside a generic block entity blob. |
| Chunk lifecycle | Generated, loaded, ticking, visible states differ | Generated/persisted truth exists; residency/ticking/visibility tiers do not | `GAP-P0` | Add deterministic demand residency without making renderer distance authoritative. |
| Render distance | Client loads/renders a bounded chunk radius | None | `GAP-P0` | Adapter owns render residency. It must not define simulation authority. |
| Simulation distance | Server ticks a bounded region | Current state is small enough to tick globally | `GAP-P1` | Add explicit activity levels when world scale demands them. Far industry/ecology may use deterministic coarse continuation rather than Minecraft-style total freezing where a better model is justified. |
| Streaming | Chunks enter/leave memory around demand | None | `GAP-P0` | Add deterministic chunk load/generate/unload boundaries before claiming persistent world scale. |
| World border | Bounded gameplay border despite large address space | No border | `OUT-OF-SCOPE` | Not needed for parity. Coordinate and generation limits should still fail safely. |

### Better-than-Minecraft storage target

Deep Hearth should not answer richer physics by making every voxel enormous. The efficient target is two-tiered:

1. a compact palette-compressed cell stores the ordinary block state needed for geometry, interaction class,
   appearance, and default physical material; full-cube default mass is derived from material density and
   section summaries retain exact represented mass for conservation accounting;
2. sparse per-section sidecars store exceptional generated state such as temperature departure, moisture,
   damage, contamination, exact embodied trace, active process identity, or other values that actually differ
   from the block-state default.

This can store more meaningful information than Minecraft without paying for every possible property on every
cell. A stone voxel at ambient conditions should remain close to a small palette index. A heated, cracked,
water-saturated structural stone voxel can acquire sparse state only while those facts matter.

## 3. Blocks, block states, shapes, and update semantics

Minecraft's world is not merely cubes. A position holds a block state. The state determines collision, occlusion,
light behavior, fluid state, placement behavior, random ticking, neighbour reactions, shape updates and model
variants. Some positions also reference a block entity. Deep Hearth currently has `BlockAppearanceDefinition`,
but that is presentation metadata, not a runtime block model.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Block type registry | Stable block definitions | `WorldRegistry` owns `BlockDefinition` separately from material and appearance | `PARITY` foundation | Extend behavior fields only when a real world verb needs them. |
| Block state | Compact state from finite properties | Immutable `BlockStateDefinition` values are referenced by compact `BlockStateId` in sections | `PARITY` foundation | Add finite state properties/orientation as placement/update mechanics arrive. |
| Material vs block | Blocks often encode gameplay properties separately from item/material concepts | `BlockDefinition` references commodity identity and exact represented mass; full-cube mass is validated from material density | `DIFFERENT+` | Keep material/form authority separate from block identity and preserve exact matter through generation, breaking, and placement. |
| Full cube | Common 1x1x1 solid | Blocks currently classify collision as `Empty` or `FullCube` | `PARITY` foundation | Use full cube as the fast path for collision/meshing; richer shapes follow. |
| Partial block shapes | Slabs, stairs, fences, doors, snow layers, etc. | None | `GAP-P1` | Add immutable voxel-shape/collision-shape definitions with orientation/state variants. |
| Replaceability | Grass, fluids, snow etc. have placement replacement rules | No block placement model | `GAP-P1` | Define replacement/placement legality by block state and target face. |
| Orientation | Placement derives facing/axis/half from click and player direction | No generic placement | `GAP-P1` | Placement resolver derives the candidate state from hit face, hit point and player orientation. |
| Collision shape | State-specific | Definition distinguishes empty/full-cube collision, but player collision does not consume it yet | `GAP-P0` integration | Continuous movement must query these shapes; partial shape representation follows. |
| Visual shape | May differ from collision | Appearance only, no geometry | `GAP-P1` | Keep render shape separate from collision and interaction shape. |
| Occlusion | Faces can cull against neighbouring geometry | No mesh world | `GAP-P0` | State exposes face-occlusion masks for meshing. |
| Light emission/opacity | State-specific | Shader vocabulary exists, no world light source | `GAP-P1` | State supplies gameplay light emission and light obstruction independent of texture brightness. |
| Neighbour update | Nearby change can synchronously notify blocks | No generic block behavior | `GAP-P1` | Add bounded neighbour notifications owned by world mutation. |
| Shape update | State can adapt when neighbour changes | No generic block behavior | `GAP-P1` | Support deterministic state replacement derived from neighbour geometry. |
| Scheduled block tick | Deferred per-position block appointment | Production/labor schedules exist, no world block scheduler | `GAP-P1` | Add chunk-owned scheduled block/fluid appointments with deterministic ordering and persistence. |
| Random tick | Selected active blocks update stochastically | No runtime RNG/world blocks | `GAP-P2` | Add only when ecology/growth/fire needs it, with deterministic state-owned or position-derived random streams. |
| Block event | Short delayed action channel for pistons/notes/etc. | None | `GAP-P2` | Prefer typed owner events or scheduled state transitions. Do not add a generic event bus solely for parity. |
| Block entity | Sparse state beyond finite block-state palette | Separate domain records | `DIFFERENT+` | Voxel attachment points to canonical domain owner. Generic opaque NBT-like state is not the authority. |
| Breaking hardness | Block + tool determine break duration | Mining hardness and equipment capability already exist, but not general terrain blocks | `DIFFERENT+` | Use actual material/tool capability to derive break work. Preserve hold-to-break interaction. |
| Tool requirement | Tags/tiers determine drops and speed | Capability/hardness contracts | `PARITY+` conceptually | Integrate those contracts into block breaking instead of adding parallel tool-tier tags. |
| Drops | Block destruction resolves item drops | Mining/gathering produce exact material custody | `DIFFERENT+` | Convert destroyed block matter into conserved world/item material outputs, then auto-pickup when capacity permits. |

## 4. Rendering, textures, meshing, and visibility

Minecraft bakes block models into quads, uses texture atlases, builds meshes per chunk section, culls hidden faces,
rebuilds dirty visible sections, and submits terrain through a modern GPU path. Deep Hearth has unusually compact
texture and shader definitions but no world mesher or graphics backend.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Authored block textures | Resource-pack images | 32x32 indexed `TextureDefinition` | `DIFFERENT+` | Keep indexed authored textures. They are compact and support palette variation. |
| Texel storage | GPU-resident color textures/atlases | `PackedTexel` is exactly 1 byte: 4-bit palette slot + 4-bit shade | `PARITY+` at asset payload | Preserve. One 32x32 base pattern is 1,024 indexed bytes versus 4,096 raw RGBA8 texel bytes before shared palette overhead. |
| Mipmaps | Texture mip chain | Deterministic indexed mip chain | `PARITY+` | Keep deterministic mip generation and point-sampled index lookup. |
| Pattern deduplication | Atlas stitches resources; identical source images are not the central semantic model | Baked layout deduplicates identical indexed patterns | `PARITY+` | Keep. |
| Palette reuse | Tint systems and texture assets vary color by different mechanisms | Shared 16-color ramps and deduplicated palette rows | `PARITY+` | Use material/biome/environment bindings to exploit this without proliferating textures. |
| Alpha classes | Opaque/cutout/translucent render layers | `Opaque`, `Cutout`, `Blend` | `PARITY` | Integrate into block mesh render queues. |
| Order-independent transparency | Java 26.3 can use an optional OIT path; otherwise translucent geometry still uses performance-oriented sorting | Alpha mode definitions only | `GAP-P2` | Start with correct bounded translucent sorting. Add OIT only if actual overlapping water/glass/particles justify its cost. |
| Graphics backend | Java 26.3 supports modern OpenGL/Vulkan rendering infrastructure | No backend | `GAP-P0` | Choose adapter backend by platform support and measurement. Core shader definitions remain backend-neutral. |
| Window/input backend | Java 26.3 uses SDL3 | No platform adapter | `GAP-P0` | An SDL3/winit-class adapter is required, but the library choice does not belong in simulation authority. |
| Cube face mapping | Block models often resolve to quads | Six-face `BlockAppearanceDefinition` | `PARITY` for cubes | Use as fast path, not the full block model system. |
| Non-cube models | Baked model quads for stairs, plants, doors, etc. | Object appearances exist, no block geometry model | `GAP-P1` | Add renderer-neutral block model geometry and state-to-model selection. |
| Section mesh | Derived mesh for local chunk section | None | `GAP-P0` | Mesh loaded sections from immutable/snapshotted world data. Mesh is derived cache, never authority. |
| Meshing snapshot | Worker meshing reads copied section/block-entity data rather than mutating the live world | No world mesh lane | `GAP-P0` | Snapshot the bounded section neighbourhood needed for meshing so background work cannot race authoritative state. |
| Hidden-face culling | Faces against opaque neighbours omitted | None | `GAP-P0` | Use block occlusion masks and neighbour state. |
| Dirty rebuild | Changed section marked and rebuilt | None | `GAP-P0` | Track mesh generation/dirtiness per render partition. |
| Rebuild granularity | Minecraft can rebuild a 16-cubed section | None | `DIFFERENT+` opportunity | Start with section rebuilds for simplicity, then profile. If frequent rich-state changes dominate, smaller mesh pages/meshlets are justified; do not optimize speculatively. |
| Frustum/visibility | Section visibility traversal plus camera frustum | None | `GAP-P0` | Add camera frustum and section visibility. Optional occlusion refinement can follow. |
| Draw batching | Java 26.3 renders terrain with MultiDrawIndirect on supported devices | Shader definitions only | `GAP-P0` | Adapter should batch by pipeline/material and use indirect/multidraw where backend supports it. |
| Dynamic lights | Vanilla gameplay mostly block light, renderer has fixed world-light path | Shader work budget already allows dynamic lights | `GAP-P2` | Do not make dynamic point lights substitute for authoritative block/sky light. They may augment visuals later. |
| Renderer authority | Renderer consumes client copy/derived data | Core explicitly keeps renderer IO outside `AppState` | `PARITY+` architecture | Preserve this separation. |

### Texture efficiency claim

The current texture representation is one area where a narrow "better" claim is justified. For one unique 32x32
pattern, Deep Hearth stores one byte per indexed texel, and its complete 32, 16, 8, 4, 2, 1 mip chain contains
1,365 indexed texel bytes. The same texel count in raw RGBA8 is 5,460 bytes. Shared ramp tables and palette rows
add overhead, but that overhead is amortized and both identical patterns and identical palette rows are deduplicated.

This does **not** mean Deep Hearth's renderer is currently more efficient than Minecraft's. There is no world
renderer yet. It means the authored/baked texture payload has a strong compact representation worth preserving
when the actual mesher and GPU adapter are built.

## 5. Camera, input, targeting, movement, and collision

This remains a major P0 disparity, but ordinary horizontal walking now uses the authoritative body path. Minecraft
movement is continuous within the block grid. Deep Hearth persists a fixed-point bottom-center body point plus
horizontal and vertical velocity, samples normalized world-space horizontal control at 20 Hz, accelerates/brakes
deterministically, integrates gravity, and sweeps the authored standing body against generated full-cube terrain.
Richer stances, impulses, surface response, and partial collision shapes are still absent.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Mouse/controller look | Continuous yaw/pitch camera | No input/camera adapter | `GAP-P0` | Add camera orientation and input sampling in adapter/client state. |
| Player position | Continuous 3D position | Persisted fixed-point `WorldPoint`; voxel locality is derived | `PARITY` foundation | Keep one transform authority. |
| Velocity | 3D delta/velocity affected by input and environment | Persisted signed horizontal and vertical velocity; horizontal control is normalized fixed-point input | `PARITY` foundation | Extend only with live stance/environmental effects rather than speculative components. |
| Collision box | Player AABB | Authored 0.6 m wide x 1.8 m standing body envelope | `PARITY` foundation | Add stance-dependent height and richer collision shapes only when required. |
| Swept collision | Movement vector clipped against block shapes | Deterministic X-then-Z horizontal sweep plus vertical sweep against `FullCube` terrain; exact wall/floor/ceiling contact; missing chunks fail closed as walls | `PARITY` foundation | Add partial shapes, bounded step-up, and swept contact effects without changing world authority. |
| Walking | Analog-ish continuous horizontal motion | 20 Hz normalized world-space input drives authored walking speed/acceleration; legacy timed one-voxel step remains only outside generated-world body mode | `PARITY` foundation | Keep direct walking on body control time. Retain discrete route/haul cost only for logistics and AI planning. |
| Sprinting | Faster movement with resource cost | Direct walking already settles speed- and load-scaled locomotion exertion into survival; no sprint verb | `GAP-P0` | Add sprint input with distinct authored speed/effort and encumbrance limits. |
| Crouching/sneaking | Slower stance, reduced height, edge behavior, interaction modifier | None | `GAP-P0` | Add stance, collision-height change and contextual input modifier. |
| Jumping | Vertical impulse | None | `GAP-P0` | Add jump impulse constrained by footing, encumbrance and physiology. |
| Gravity/falling | Continuous vertical acceleration and collision | 20 Hz authored gravity, capped fall speed, exact full-cube landing, persisted mid-fall continuation | `PARITY` foundation | Add fall injury and environmental modifiers without weakening deterministic integration. |
| Fall damage | Distance/velocity-derived damage | No general damage system | `GAP-P1` | Derive impact injury from landing speed and protection rather than copying arbitrary block-count thresholds. |
| Step-up | Small obstacle auto-step | None | `GAP-P1` | Add bounded step height so ordinary terrain traversal feels familiar. |
| Friction/slipperiness | Surface-dependent movement | None | `GAP-P1` | Material/block state should provide traction; ice/mud equivalents emerge from it. |
| Fluids movement | Swimming, buoyancy/drag, current interaction | No world fluid field | `GAP-P1` | Add after spatial fluids. Use density/drag/current rather than special-case-only movement. |
| Climbing | Ladders/vines/scaffolding special movement | None | `GAP-P1` | Add climbable support surfaces/components. |
| Edge/contact effects | Movement path encounters blocks, fire, fluids, etc. | Voxel destination only | `GAP-P1` | Track swept contact so environmental effects cannot be bypassed by fast movement. |
| Encumbrance | Vanilla inventory has no weight | Exact carried mass plus equipment mass scales direct-walking energy/hydration cost; fractional 20 Hz cost persists and settles through the slow survival owner | `DIFFERENT+` | Keep. Extend it to achievable acceleration/sprint/jump/climb/swim where physically justified. |
| Multi-step routing | Mob/path systems route through blocks | Explicit traversal edges only | `GAP-P1` | Player needs no click-to-route for WASD, but AI/logistics need pathfinding over world collision/access data. |

`validate_start_player_step` is rejected once the player's footprint is backed by generated chunks. It remains a
bounded logistics/planning surface for the opening graph and older evidence, not a first-person locomotion
primitive. Direct horizontal walking already extends the persisted body path; future stance/impulse mechanics
must stay on that authority rather than reopening timed voxel movement.

## 6. Raycasting, breaking, placing, using, and direct manipulation

Minecraft's basic grammar is point at something, then primary or secondary action. Deep Hearth's design already
requires this grammar, but production currently exposes domain-specific validators rather than one direct world
interaction composition layer.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Camera raycast | Crosshair selects block/entity within reach | No camera/raycast | `GAP-P0` | Add deterministic fixed-point ray query against block interaction shapes and entities. |
| Reach | Player has bounded interaction distance | Locality is mostly same/adjacent voxel | `GAP-P0` | Replace coarse voxel-local checks at direct-action surface with ray/reach authorization; owners still validate exact targets. |
| Hold-to-break | Break progress accrues every tick from tool speed, block hardness and player modifiers; client predicts while server validates | Gathering/mining are admitted timed jobs | `GAP-P0` shell | Map held primary action to canonical break work with deterministic progress from material resistance, tool capability, condition, stance and environment. Progress is owner-derived work, not an extra confirmation action. |
| Crack/progress feedback | Visual breaking stages | Timed work can expose schedule but no renderer | `GAP-P1` | Expose normalized progress and target identity for adapter feedback. |
| Tool selection | Held hotbar item defines tool context | Equipment can be carried but no hotbar | `GAP-P0` | Add active-hand/hotbar projection that resolves to canonical equipment/material identity. |
| Placement preview | Click face and state placement rules determine result | Grounded structural plan is explicit API | `GAP-P0` | Secondary action should derive candidate placeable/block state from held stack and hit result. Heavy structural work may then continue as timed construction. |
| Instant simple placement | Most Minecraft blocks place immediately | Structural materialization takes authored work | `DIFFERENT+` | Simple loose/placeable camp blocks may commit immediately or near-immediately; materially significant structural members can require work. Do not make every block placement a construction form. |
| Contextual use | Doors, chests, crafting stations, buttons, food | Domain-specific operations exist | `GAP-P0` | Add a thin interaction resolver that selects the owning domain operation without duplicating legality. |
| Attack | Primary on entity attacks | Combat absent | `GAP-P1` | Same primary-action grammar should target entities when no block action takes precedence. |
| Pick block | Creative convenience | None | `OUT-OF-SCOPE` | Development/creative tooling can add later. |
| Auto pickup | Broken drops enter inventory if possible | Design requires it; some mining/gathering already auto-land outputs | `PARITY+` direction | Generalize to world item matter once dropped entities exist. Capacity failure leaves matter in world. |

## 7. Items, stacks, hotbar, inventory, and containers

Minecraft uses count-based item stacks in slots. Deep Hearth uses exact mass-based material lots and finite
stockpiles. The underlying Deep Hearth model is stronger, but the familiar manipulation shell is still absent.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Item definition | Registry item plus prototype components | Material/form/equipment definitions | `DIFFERENT+` | Keep typed domain definitions. Add a player-facing item presentation identity that can refer to material lots, equipment or placeables without becoming another owner. |
| Item stack | Item identity + count + component differences | Exact material lots may coalesce by compatible physical state | `DIFFERENT+` | Project compatible lots as familiar stacks. Split visible stacks when freshness, temperature, composition, provenance or physical state makes the difference meaningful. |
| Stack count | Discrete count up to stack max | Natural bulk is exact mass | `DIFFERENT+` | Use counts where the form has discrete unit identity; use mass/volume for bulk. This is already the design law. |
| Hotbar | Nine quick slots | Absent | `GAP-P0` | Add nine-slot or equivalently familiar quick-access projection with active slot. |
| Main inventory | Slot grid | Carried stockpile with mass capacity | `GAP-P0` shell | Add slot/container presentation over carried custody. Physical capacity remains independently enforced. |
| Weight | None | 25 kg unaided body-carry capacity plus equipment mass | `PARITY+` physical model | Keep as second constraint. Do not make the player micromanage lots to satisfy it. |
| Pickup | Item entity transfers into inventory if space | Surface gathering and job output can land in inventory | `GAP-P0` generic world item path | Add generic item/matter world custody and automatic pickup. |
| Drop | Spawn item stack entity | Material/equipment drop operations exist only in narrower forms | `GAP-P0` | Active stack/equipment can be dropped into world custody at player position with conserved identity. |
| Merge nearby drops | Compatible item entities can merge | Lot coalescing exists in inventory | `GAP-P1` | World item custody may coalesce compatible material to reduce entity count while preserving physical distinctions. |
| Despawn drops | Item entities eventually despawn | Matter conservation rejects silent deletion | `DIFFERENT+` | Do not copy magical matter deletion. If cleanup is needed, use explicit decay, scattering, garbage collection into an environmental sink, or no despawn for conserved matter. |
| Container slots | Chests/furnaces expose slot policies | Stockpiles/storage profiles exist | `GAP-P0` shell | Containers are UI/projection over storage owners plus typed input/output slots where machines require them. |
| Shift-click/quick move | Familiar transfer gesture | No UI | `GAP-P0` | Implement as request composition over canonical relocation/ingress rules. |
| Drag/split stack | Familiar manipulation | Exact lot selection APIs exist | `GAP-P1` | UI translates count/mass gesture into exact lot partition transaction. |
| Capacity rejection | Full slots block transfer | Mass, phase, temperature, enclosure and reservation constraints | `PARITY+` | Surface the physical blocker clearly while preserving expected transfer gestures. |
| Equipment durability | Stack component durability | Separate equipment condition/maintenance | `PARITY+` | Better owner model. Hotbar/item projection should show condition without flattening it to generic stack damage. |

## 8. Crafting, recipes, tools, machines, and progression work

Minecraft has recipe grids and station-specific menus. Deep Hearth already models much richer physical processes.
The missing part is not recipe depth. It is the simple shell that lets ordinary craft actions reach those owners.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Personal crafting | 2x2 recipe grid | Manual craft process definitions, no inventory UI | `GAP-P0` shell | Provide personal crafting from carried inventory through recipe/amount requests. |
| Crafting table | 3x3 recipe grid | No generic station UI | `GAP-P1` | A primitive work surface can widen recipes/capabilities without requiring Minecraft's exact 3x3 semantics. |
| Shaped recipe | Grid positions matter | Process input selection is commodity/mass based | `OUT-OF-SCOPE` as universal rule | Use shaped arrangement only when geometry is meaningful. Do not force physical processes into pattern puzzles. |
| Shapeless recipe | Ingredient set | Material transforms can already resolve exact inputs | `PARITY+` | Familiar recipe presentation can map to canonical process inputs. |
| Cooking/furnace | Fuel + timed recipe + output | Thermal processes and exact energy stores are much richer | `PARITY+` model | Present simple fire/cooking interactions without exposing energy bookkeeping unless it matters. |
| Tool efficiency | Tool class/tier modifies break speed | Capability and condition resolve actual work | `PARITY+` | Integrate with block hardness and direct breaking. |
| Tool wear | Durability decrements | Condition/wear curves and maintenance | `PARITY+` | Keep. |
| Repair | Anvil/grindstone/material repair | Explicit maintenance with replacement matter | `PARITY+` | Familiar repair action can compose maintenance owner. |
| Enchanting | Magic modifiers/XP | No magic progression | `OUT-OF-SCOPE` | Materials, maintenance, knowledge and technology intentionally replace enchantment as capability growth. |
| Experience levels | Generic XP currency | Information/capability/material progression | `OUT-OF-SCOPE` | Do not add abstract XP solely for parity. |
| Recipe book | Discoverable recipe browser | Commodity handbook/reverse indexes exist | `PARITY+` direction | Present them as handbook/recipe UI with authored possibility separated from current opportunity. |
| Automation | Hoppers/redstone/furnaces | Physical production, power and future logistics networks | `DIFFERENT+` | Do not copy redstone as universal automation. Provide physical transport/control networks while preserving familiar local switches, containers and machine states. |

## 9. Health, hunger, hydration, temperature, damage, death, and rest

Minecraft's survival model is intentionally abstract: health, food level, saturation, exhaustion and a set of
damage sources. Deep Hearth already has a more physical survival model, but it lacks the general damage/combat
and rest shell that makes survival feel complete.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Health/vitality | Health points | Normalized `Vitality` | `PARITY` | Keep vitality as canonical health-like state. |
| Hunger | Food + saturation + exhaustion | Metabolic energy reserve with exertion costs | `DIFFERENT+` | Better causal model. HUD should still give an immediately readable food/energy condition. |
| Hydration | Not modeled in vanilla | Explicit hydration reserve and work loss | `PARITY+` | Keep. |
| Nutrition breadth | Minimal vanilla food distinctions | Nutrition categories/reserves | `PARITY+` | Keep only where consequences remain legible. |
| Thermal comfort | Mostly biome/status effects, not a body warmth reserve | Explicit thermal-comfort reserve and local heat response | `PARITY+` | Keep and connect to weather, shelter, clothing and environmental heat. |
| Starvation damage | Food depletion can damage | Explicit starvation vitality loss | `PARITY+` | Keep. |
| Dehydration damage | Absent | Explicit | `PARITY+` | Keep. |
| Drowning | Air meter/damage | Absent | `GAP-P1` | Add when spatial fluids exist. |
| Fire/burning | Fire ticks and damage | Thermal energy exists but player burning absent | `GAP-P1` | Add heat exposure/burn damage as physical environmental consequence. |
| Fall damage | Landing damage | Absent | `GAP-P1` | Add impact injury from kinematics. |
| Suffocation | Collision with solid blocks damages | Absent | `GAP-P1` | Add body-space occupancy safety and escape/recovery. |
| Poison/status effects | Timed effects modify attributes/health | No general status-effect owner | `GAP-P2` | Add domain-specific conditions when ecology/toxicity needs them; avoid generic effect soup if typed physiology can own the cause. |
| Natural regeneration | Health regenerates under food conditions | No general injury recovery loop | `GAP-P1` | Add recoverable vitality/injury treatment with food/rest/medical causes rather than automatic arcade regen alone. |
| Death | Player dies and can respawn, inventory handling depends on rules | Death cancels/settles active work; no complete ordinary respawn loop | `GAP-P1` | Define death, corpse/dropped custody, respawn/recovery and world continuation. Matter must remain accounted for. |
| Sleep | Bed sets spawn and skips/advances night under rules | Absent | `GAP-P1` | Add rest as recovery and time-management after world clock/shelter exist. Do not require instant night skip if it breaks environmental/production causality. |

## 10. Entities, creatures, combat, projectiles, and AI

Minecraft's sandbox is populated by entities with continuous transforms and AABBs. Deep Hearth currently has
domain records for equipment/stores and a player logistics record, but no generic spatial entity runtime.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Entity identity | Runtime entity IDs/UUIDs | Typed IDs per domain, no general entity | `DIFFERENT+` | Keep domain identity. Add a spatial actor/entity owner only for things with shared transform/lifecycle semantics. |
| Continuous transform | Position/rotation/velocity | Absent except discrete player voxel | `GAP-P0` | Shared fixed-point transform primitive for player, creatures, item drops, projectiles and vehicles. |
| Entity collision | AABB/block and selected entity interactions | Absent | `GAP-P1` | Add broadphase spatial indexing and shape-aware collision. |
| Item entity | Dropped stack in world | Absent | `GAP-P0` | First generic entity-like custody case. |
| Passive animals | Spawn, wander, flee, breed | Absent | `GAP-P2` | Ecology should eventually own populations/reproduction. Basic wildlife is needed before the world feels like a survival sandbox. |
| Hostile mobs | Spawn/AI/attack | Absent | `GAP-P1` for familiar survival tension | One coherent threat family is enough initially. Do not copy the entire mob roster. |
| Melee attack | Reach, cooldown/damage/knockback | Absent | `GAP-P1` | Add primary-action entity attack, equipment capability, hit timing and physical knockback. |
| Armor | Damage mitigation equipment | Equipment exists, protection capability absent | `GAP-P2` | Model material/protection/coverage rather than armor points only. |
| Shield/block | Contextual defense | Absent | `GAP-P2` | Add when combat merits it. |
| Projectiles | Arrow-like entities with flight/collision | Absent | `GAP-P2` | Fixed-point ballistic projectile owner can reuse kinematics/collision. |
| Pathfinding | A* over loaded world navigation types | No general terrain/access graph | `GAP-P1` | Derive navigation from voxel collision and movement capabilities. Explicit traversal edges should not be hand-authored world truth. |
| Spawning | Local conditions + caps | No ecology/entities | `GAP-P2` | Prefer ecology/population causes over arbitrary caps where feasible, while still bounding active entity cost. |
| Despawn | Rules reduce far mob count | No entities | `DIFFERENT+` opportunity | Persistent ecology can aggregate far populations instead of deleting creatures solely for performance. |
| Riding/vehicles | Entity passengers, boats, carts, mounts | Absent | `GAP-P2` | Later logistics can reuse transform/attachment primitives. |

## 11. Fluids, fire, heat, light, weather, and environment

Minecraft spatially simulates water and lava as block-derived fluid states with scheduled flow. It is not volume
conserving in the way Deep Hearth intends. Deep Hearth has finite fluid stores and much stronger energy accounting,
but no world-space fluid or environment field.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Water occupies world cells | Fluid state at blocks | Finite fluid stores only | `GAP-P1` | Add voxel/world fluid occupancy distinct from container stores. |
| Flow | Scheduled local propagation | No world flow | `GAP-P1` | Flow finite volume through cells/porous media/channels with deterministic scheduling. |
| Source blocks | Water can regenerate under rules | Finite non-refilling water reserve | `DIFFERENT+` | Do not copy infinite source creation. Recharge must come from hydrology. |
| Fluid level | Local flow amount maps to visual height | Store volume only | `GAP-P1` | Spatial fluid cells need bounded fill/volume state and surface height. |
| Waterlogging | Blocks can coexist with fluid | No world blocks | `GAP-P1` | Model pore/contained fluid or explicit waterlogged state for compatible geometry. |
| Buoyancy/current | Entity-fluid interaction | Absent | `GAP-P1` | Couple fluid density/velocity to body movement. |
| Lava | Flowing hazardous fluid, block reactions | General molten materials exist in foundry, no world lava | `GAP-P2` | World molten material can reuse thermal/material physics. No need for Minecraft's special source semantics. |
| Combustion | Fire/campfires/furnaces consume fuels with simplified rules | Exact fuel matter to thermal energy and terminal mass accounting for campfire | `PARITY+` model | Preserve matter/energy accounting. Add world ignition, flame exposure and visible fire. |
| Fire spread | Random/scheduled ignition of flammables | Absent | `GAP-P1` | Add material ignition/combustibility when environment owner exists. |
| Smoke | Particles/campfires; little atmospheric simulation | Absent | `GAP-P2` | Ventilation/air quality can eventually make smoke causal rather than visual only. |
| Block light | 0-15 propagated light | No world light engine | `GAP-P0` for caves/night | Add authoritative gameplay light field with emission/occlusion. Exact numeric scale need not match Minecraft. |
| Sky light | Propagated sky exposure | No light engine | `GAP-P0` | Add sky/exposure light derived from world geometry and clock. |
| Dynamic visual light | Limited vanilla behavior | Shader budget supports dynamic lights but no backend | `GAP-P2` | Visual lights augment, not replace, gameplay visibility/light truth. |
| Day/night | 24,000-tick cycle | Calendar date exists, no sun/sky/daylight gameplay | `GAP-P1` | World clock drives sky, light, temperature and schedules. Cycle duration can differ with explicit pacing reason. |
| Rain/thunder | Dimension weather state affects world | Absent | `GAP-P1` | Weather should affect temperature, wetness, fire, soil, visibility, water and work, not just visuals. |
| Biome climate | Temperature/precipitation/environment parameters | Intended, not implemented | `GAP-P1` | Generate climate fields that actually feed survival/ecology/hydrology. |
| Seasons | Vanilla lacks ordinary seasonal cycle | Calendar already projects four seasons | `PARITY+` planned depth | Use seasons only once they change observable environment and decisions. |

## 12. World generation, biomes, geology, structures, and discovery

Minecraft deterministically derives terrain, biomes, caves, aquifers, ores, structures and decorations from the
seed plus world settings/data packs. Deep Hearth has an explicit seed and strong geological information models,
but its production world generation is currently a six-position opening layout and five traversal edges.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| World seed | Deterministic world input | Explicit `WorldGenerationSeed` | `PARITY` | Keep explicit seed. |
| Procedural terrain | Density field resolves solid/air terrain | None | `GAP-P0` | Generate real chunk terrain from deterministic regional fields. |
| Height variation | Mountains/valleys/continents etc. | None | `GAP-P0` | Terrain field needs elevation, erosion/roughness and landform scale. Exact Minecraft noise stack need not be copied. |
| Caves | Carvers/density create underground space | None | `GAP-P1` | Add caves when underground mining becomes world-space rather than abstract deposit work. |
| Biomes | Climate sampling maps positions to biome definitions | None | `GAP-P1` | Biome/ecoregion should be a readable projection of climate, soil/geology and ecology rather than arbitrary paint where possible. |
| Surface rules | Terrain material varies by context | None | `GAP-P0` | Surface material derives from geology, weathering, soil and climate. |
| Aquifers | Worldgen water tables/flooded caves | Finite opening water store only | `GAP-P1` | Hydrology should own water table/recharge/flow. |
| Ore placement | Seeded geological/worldgen placement | Hidden geology/deposits can be generated in controlled paths | `DIFFERENT+` | Keep geology as causal owner and extend it regionally into terrain chunks. Avoid independent scatter-noise ore blobs where geological structure can explain them. |
| Structures | Villages, camps, dungeons, etc. | Player-built structures only; no generated built sites | `GAP-P2` | Generated ruins/camps/settlements can come after terrain, with material-backed world state where practical. |
| Vegetation features | Trees/patches placed by feature passes | Opening timber/forage resources are abstract finite records | `GAP-P1` | World-space trees/plants must become block/entity/ecology state. |
| Feature placement pipeline | What, where, biome ownership separated | No general generator | `PARITY+` opportunity | Preserve separation among generator definition, placement rule and regional/ecological eligibility. |
| Chunk generation stages | Progressive chunk statuses allow partial work | None | `GAP-P0` | Use staged generation so terrain can stream before every expensive ecology/geology detail is complete. |
| Discovery | Player sees/loads world by travel | Exact observation only on bounded graph edges | `GAP-P0` | Loaded/rendered local terrain itself becomes primary discovery. Information-gated geology remains deeper evidence, not permission to see ordinary ground. |
| Spawn selection | Safe-ish valid world spawn | Caller supplies opening spawn | `GAP-P1` | Generate/select a valid spawn from terrain, water/food opportunity and collision safety without guaranteeing a scripted resource checklist. |

### Better-than-Minecraft generation target

Deep Hearth has a reason to differ from Minecraft most strongly in generation semantics. Minecraft's terrain,
biomes, ore features, aquifers and ecology are related enough to make a compelling world, but many systems are
still independent generators. Deep Hearth's design can derive more of the world from shared regional causes:

`tectonic/geological province -> lithology + relief -> drainage + soil -> climate exposure -> ecology -> local resources`

That gives prospecting, water, agriculture, erosion, structures and industry a common explanation. The requirement
is not to simulate Earth for its own sake. Each added field must change a player decision and have a bounded
generation/runtime cost.

## 13. Structures, gravity, terrain support, redstone-like reactions, and automation

Minecraft is a construction sandbox but most blocks ignore structural load. Deep Hearth already has a much
stronger structural owner. The parity challenge is to make that depth compatible with rapid block building.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Stack blocks freely | Most solid blocks can be placed without support analysis | Grounded adjacent member construction only | `GAP-P0` shell | Basic block placement must be straightforward. Structural simulation should reject/settle physically impossible builds, not turn every placement into engineering UI. |
| Gravity blocks | Sand/gravel-like blocks fall | Structural owner models load but no terrain blocks | `DIFFERENT+` | Loose/unsupported materials should fall or fail based on material/cohesion/support, not a hardcoded tiny list. |
| Structural failure | Most Minecraft blocks never fail by load | Support/load/failure physics | `PARITY+` | This is a major differentiator. Keep failure readable and recoverable. |
| Terrain support | Terrain implicitly supports blocks | Terrain support absent | `GAP-P0` | World terrain must contribute structural support before structures can integrate with actual ground. |
| Joints/panels | Block adjacency usually implies connection | Joints/panels absent | `GAP-P1` | Add explicit connection semantics where material/geometry needs them; ordinary masonry/block contact can use simple defaults. |
| Deconstruction | Break block and get drops | General structural deconstruction absent | `GAP-P1` | Primary break action must dismantle placed structures with salvage/yield consequences. |
| Redstone signal | 0-15 neighbour-propagated signal network | No generic control network | `OUT-OF-SCOPE` exact mechanic | Physical electrical/mechanical/control networks replace redstone. |
| Switches/buttons | Immediate local player control | No direct block controls | `GAP-P1` | Basic switches/valves/levers are still familiar world interactions and should exist even though network physics differ. |
| Pistons | Blocks move block states | No terrain mutation machinery | `GAP-P2` | Later actuators can move supported world blocks/assemblies through canonical world transactions. |
| Hoppers | Item transfer automation | No general logistics network | `GAP-P2` | Conveyors/chutes/workers/containers can provide the same automation category with explicit transport cost. |

## 14. Persistence, saves, chunk storage, networking, and data-driven content

Deep Hearth's trusted-state semantics are stronger than Minecraft's general save admission, but there is no
actual save-file/chunk storage adapter or streaming persistence yet.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Save world state | Chunk/level data persisted to disk | `SaveEnvelope` serializes current `AppState`; bytes/storage adapter absent | `GAP-P0` product shell | Add durable file adapter with atomic replace/recovery policy. |
| Chunk-local persistence | Region/chunk storage supports world streaming | Whole small state model | `GAP-P0` | World owner needs chunk-granular persistence and dirty tracking. Do not serialize an enormous world as one monolith. |
| Trusted load | Minecraft validates through codecs and runtime assumptions | Exact schema, derived-index rebuild, whole-state validation before promotion | `PARITY+` | Preserve strict admission for every chunk/owner attachment. |
| Compatibility | Minecraft migrates old world data | Current-schema only by policy | `DIFFERENT+` | Solo development explicitly does not need backward compatibility now. Keep migrations absent until there is an active supported save contract. |
| Autosave | Periodic/background saving | Byte-storage adapter absent | `GAP-P1` | Add snapshot/copy boundary so IO never observes half-mutated owner state. |
| Crash recovery | Region/save semantics limit damage | Absent adapter | `GAP-P1` | Durable writer should preserve last known-good world/chunk data. |
| Multiplayer protocol | Client/server packets | Absent | `GAP-P2` | Singleplayer parity does not require networking. Keep command/read boundaries serializable enough to add it later. |
| Data packs | Many definitions/data-driven registries | Rust-authored validated registries | `DIFFERENT+` | Internal typed authoring is acceptable now. External data/mod loading can follow when content iteration needs it. |
| Resource packs | External assets/models/textures | Authored Rust/assets shader setup | `GAP-P2` ecosystem | Not required for core gameplay parity, but renderer assets should remain decoupled from simulation identity. |
| Commands/creative tooling | Rich command system and creative mode | Test fixtures and direct APIs | `GAP-P2` | Add explicit developer/world-edit tooling when runtime world exists; never expose test-only mutation as player authority. |

## 15. HUD, menus, feedback, audio, particles, and accessibility

The core can be physically correct and still fail parity if the player cannot read or manipulate it. Minecraft's
immediacy comes from constant low-latency feedback: crosshair, hotbar, hand animation, block cracks, sounds,
particles, damage flashes, hunger/health, item tooltips and menus.

| Granular mechanic | Minecraft | Deep Hearth now | Verdict | Required Deep Hearth position |
| --- | --- | --- | --- | --- |
| Crosshair | Persistent target reference | Absent | `GAP-P0` | First-person adapter. |
| Held item view | Active hotbar item rendered/animated | Absent | `GAP-P0` | Show active tool/item and action feedback. |
| Hotbar HUD | Quick selection + counts/durability | Absent | `GAP-P0` | Projection over carried custody/equipment. |
| Health/food HUD | Immediate reserve status | Read-side survival assessments only | `GAP-P1` | Present vitality, hydration, energy and thermal warnings without exposing raw bookkeeping by default. |
| Block info | Player learns target via visuals/tooltips/debug | Definitions exist but no world target UI | `GAP-P1` | Target tooltip/handbook link should identify material/form/block and meaningful state. |
| Inventory screen | Familiar grid/menu | Absent | `GAP-P0` | Required by familiar interaction shell. |
| Crafting UI | Grid/result/recipe book | Absent | `GAP-P0` first craft path | Compose canonical crafting. |
| Container UI | Chest/station screens | Absent | `GAP-P0` | Compose storage/machine owner operations. |
| Action progress | Breaking/use animations | Timed work records | `GAP-P1` | Project progress from canonical schedule. |
| Sounds | Footsteps, block break/place, tools, mobs, UI | Engine audio absent | `GAP-P1` | Add material/state-driven sound events at adapter boundary. |
| Particles | Break debris, smoke, hit, weather, etc. | Absent | `GAP-P1` | Derived visual feedback, never simulation authority. |
| Damage feedback | Screen/animation/sound | No general damage UX | `GAP-P1` | Required when damage exists. |
| Settings/keybinds | Rebindable controls, sensitivity, video/audio settings | Absent | `GAP-P1` product shell | Adapter owns controls/settings. |
| Accessibility | Subtitles, UI scales and other options | Absent | `GAP-P2` | Plan early enough that feedback is semantic, not hardwired only to color/sound. |

## 16. First-day loop and content-category parity

Mechanical parity does not require Deep Hearth to reproduce Minecraft's exact blocks, mobs or dimensions. It does
require enough content to exercise the substrate through the same recognizable first-day loop.

| Player need | Minecraft example | Deep Hearth target | Verdict now |
| --- | --- | --- | --- |
| Orient | Look around generated terrain | Camera + visible terrain + local resource clues | `GAP-P0` |
| Move | Walk/sprint/jump across blocks | Continuous movement with encumbrance/exertion | `GAP-P0` |
| Acquire wood/stone | Break logs/stone, collect drops | Break/gather world material through capability physics | `GAP-P0` shell, deeper owner pieces exist |
| Make basic tools | Inventory/crafting table recipes | Familiar recipe action over exact material transformations | `GAP-P0` shell, crafting owner exists |
| Carry items | Hotbar/inventory stacks | Hotbar/stacks projected over exact mass custody | `GAP-P0` shell |
| Store items | Chest | Material-backed container with slot-like UI and mass/storage constraints | `GAP-P0` shell, storage owner exists |
| Eat | Hold/use food | Contextual use with human-scale serving | `PARITY+` owner, input shell absent |
| Drink | Vanilla has no hydration requirement | Local source/container use | `PARITY+` depth, input shell absent |
| Survive night/environment | Light, shelter-ish construction, hostile mobs | Darkness, weather/thermal exposure, shelter, fire, basic threat | `GAP-P1` |
| Place shelter blocks | Right-click blocks into world | Contextual placement plus structural consequences | `GAP-P0` |
| Use fire | Furnace/campfire/torches | Exact fuel/heat plus cooking/light/warmth | `PARITY+` heat core, `GAP-P1` world effects |
| Save and return | World persists | Validated persisted world chunks and player | `GAP-P0` file/world adapter |
| Explore farther | Stream new chunks/biomes/resources | Deterministic terrain/climate/geology/ecology streaming | `GAP-P0` |

The initial parity milestone is complete only when a fresh player can spawn into generated terrain with empty
carried custody, look around, walk and jump freely, break a visible material, see/pick up the resulting matter,
select it or a tool in a hotbar, craft through a familiar inventory surface, place a basic block or structure,
use food/water/fire/storage, survive ordinary darkness/environmental pressure, save, reload, and continue. Every
action must flow through the same production owners used by deeper simulation rather than through a temporary
Minecraft-like facade.

## 17. Deliberate non-parity

Some Minecraft mechanics should not be copied because Deep Hearth already has a stronger causal replacement.
These differences are acceptable only after the familiar action category remains available.

| Minecraft mechanic | Deep Hearth replacement | Reason |
| --- | --- | --- |
| Count-only inventory with no weight | Exact mass/volume-compatible custody plus familiar stack/slot projection | Transport and storage become physical progression rather than arbitrary slot pressure. |
| Universal infinite water-source behavior | Finite hydrology, storage, recharge and transport | Water infrastructure and geography remain meaningful. |
| Most blocks ignore structural support | Material/support/load/failure model | Construction creates engineering decisions and recoverable failure. |
| Tool tiers and per-block hardcoded harvest rules | Material hardness plus equipment capability/condition | One physical vocabulary scales from stone tools to industrial machines. |
| Furnace recipe abstraction | Explicit heat, phase, energy, equipment and process owners | Thermal progression has real scale, loss and material consequences. |
| Generic XP/enchantment capability ladder | Knowledge, materials, maintenance, instruments, infrastructure and technology | Capability comes from physical causes rather than a generic currency/magic modifier layer. |
| Redstone as the universal automation substrate | Mechanical, electrical, fluid, logistics and control networks | Automation should inherit physical transport, power, maintenance and failure costs. |
| Item despawn as cleanup | Conserved matter or explicit environmental sink/decay | Performance cleanup must not silently violate the material model. |
| Remote chunks largely stop simulating | Deterministic activity levels and coarse continuation where beneficial | Persistent settlements/industry should not depend on camera proximity, while detailed local simulation still needs bounded cost. |

## 18. Implementation order implied by the audit

This is a dependency order, not a promise that every row becomes a separate subsystem.

### P0: block-world substrate

1. Refactor time so one authoritative fixed game tick is suitable for responsive movement and calendar scale is
   independent.
2. Add compact block definitions/states, chunk columns, 16-cubed sections, palette-compressed cells, sparse
   physical sidecars, dirty/activity summaries, and chunk-granular persistence boundaries.
3. Add deterministic seeded terrain generation and streaming sufficient for ordinary walking-world continuity.
4. Add continuous fixed-point player transform, gravity, swept collision, jump, crouch, sprint, step-up and
   terrain contact.
5. Add camera raycast, reach, primary break, secondary place/use, generic world item drops and automatic pickup.
6. Add hotbar, familiar stack projection, inventory/container interaction composition and personal crafting shell.
7. Add section meshing, camera visibility, block model/shape rendering, indexed texture integration, block/sky light
   and a graphics adapter sufficient to inspect the actual world through a day/night cycle.
8. Add save-file/chunk storage adapter and reload proof for the generated world/player state.

### P1: recognizable survival sandbox

1. Basic weather, shelter/exposure coupling and environmental thermal effects.
2. Spatial finite water and player swimming/drowning.
3. General damage, fall/fire/suffocation, death/corpse/respawn/recovery.
4. One coherent wildlife/threat slice with entity transforms, pathfinding, melee and drops.
5. Fire as world light/heat/ignition plus existing exact fuel accounting.
6. Terrain support, basic deconstruction/salvage, partial block shapes and common world controls.
7. Audio, particles, action progress and readable HUD feedback.

### P2: sandbox breadth after the first-day shell

Agriculture/ecology, richer mobs, ranged combat, vehicles, generated structures, advanced block mechanisms,
transport automation, multiplayer/networking, external data/resource packs and broader accessibility/tooling can
then expand the sandbox without delaying the foundation again.

## 19. Acceptance questions for every future foundational mechanic

Before declaring a Minecraft-adjacent mechanic complete, answer all of these:

1. What familiar player verb does Minecraft expose here?
2. Can a new Deep Hearth player perform that verb without learning internal owner terminology?
3. What state is authoritative in Deep Hearth, and is there exactly one owner?
4. If Deep Hearth differs, what concrete player or engineering benefit justifies the difference?
5. Does the difference preserve ordinary responsiveness and recognizability?
6. Can the player observe the state needed to make the decision?
7. Can the player recover from an ordinary mistake?
8. Does persistence preserve every future-affecting fact?
9. Is derived rendering/UI/cache state rebuildable rather than duplicated authority?
10. Is the common case compact enough for millions of world cells or many entities?
11. Does the mechanic reuse material, energy, fluid, labor, information, support and capacity owners rather than
    adding a parallel simplified simulation?
12. Has real gameplay, not only an owner unit test, demonstrated the familiar loop?

If the answer to question 2 is no, deeper simulation is not a substitute. If the answer to question 4 is no,
prefer the familiar convention until a measured or experiential reason to diverge exists.

## 20. Vanilla system-family breadth decisions

The earlier sections cover the substrate. This table closes the remaining obvious Minecraft system families so
that omission is an explicit decision rather than an accidental one. `GAP-P2` here means "later recognizable
sandbox breadth," not "copy Minecraft's exact content."

| Minecraft system family | Deep Hearth position | Priority / decision |
| --- | --- | --- |
| Trees as block structures | Trees should be world-space biological/material structures that can be cut and yield conserved wood | `GAP-P1` |
| Leaf decay | Foliage persistence should follow plant structure/health rather than a special leaf-distance counter if ecology can own it | `GAP-P2` |
| Sapling growth | Plant growth should use climate, soil, water and time once ecology exists | `GAP-P2` |
| Crop farming | Required eventually because agriculture is a core settlement economy | `GAP-P2` |
| Farmland hydration | Replace binary nearby-water rule with soil moisture/hydrology when available, while retaining readable irrigation behavior | `DIFFERENT+` |
| Bonemeal-style growth boost | No need for magical instant growth; fertilizers should act through nutrients/growth rate | `OUT-OF-SCOPE` exact mechanic |
| Animal breeding | Required eventually through ecology/domestication rather than item-triggered heart mode alone | `GAP-P2` |
| Hunting | Required as a wilderness food/material option once wildlife/combat exists | `GAP-P2` |
| Fishing | Useful wilderness food/resource verb; implement with actual water/ecology when justified | `GAP-P2` |
| Villages | Settlements/workers are central to later progression but should use the same economy/logistics owners as the player | `GAP-P2` |
| Villager trading | Trade is intended later; prices/availability should arise from settlement economy rather than a disconnected trade table | `GAP-P2` |
| Wandering trader | Not mechanically required | `OUT-OF-SCOPE` |
| Beds | Rest, spawn/recovery and shelter interaction are important | `GAP-P1` |
| Spawn point | Need a persistent safe player return/respawn location once death loop exists | `GAP-P1` |
| Sleeping through night | Rest may advance time, but skipped intervals must preserve production/environment semantics | `DIFFERENT+` |
| Torches | A basic portable/placeable light source is required | `GAP-P1` |
| Lanterns | Later durable lighting content, not a distinct substrate | `GAP-P2` |
| Doors/trapdoors | Common partial blocks and contextual open/close state are useful parity tests | `GAP-P1` |
| Fences/walls | Common connected collision/model state is a useful block-shape parity test | `GAP-P2` |
| Slabs/stairs | Common partial geometry/orientation is a required block-shape capability even if content arrives after cubes | `GAP-P1` capability |
| Signs/books | Useful communication/UI objects but not foundational for solo survival | `GAP-P2` |
| Chests/barrels | Basic storage UI is required; physical storage definitions already provide deeper constraints | `GAP-P0` shell |
| Furnaces/smokers/blast furnaces | Simple player-facing thermal stations should compose the existing thermal/energy owners | `PARITY+` owner, shell incomplete |
| Brewing/potions | Chemistry/medicine can provide later equivalents; no need to clone potion recipes | `OUT-OF-SCOPE` exact system |
| Enchanting | Replaced by material/tool/knowledge/maintenance progression | `OUT-OF-SCOPE` |
| Experience orbs/levels | No generic XP currency required | `OUT-OF-SCOPE` |
| Advancements/achievements | Useful guidance/recording layer, not simulation authority | `GAP-P2` product feature |
| Recipe unlocks | Handbook discovery may reveal authored processes, but physical capability must not depend on an abstract unlock | `DIFFERENT+` |
| Boats | Useful once world water transport exists | `GAP-P2` |
| Minecarts/rails | Later transport/logistics technology can provide equivalent routed vehicles | `GAP-P2` |
| Horses/mounts | Later domestication/transport | `GAP-P2` |
| Elytra flight | Not needed for baseline or intended progression | `OUT-OF-SCOPE` |
| Portals | Not required for a single persistent-world progression | `OUT-OF-SCOPE` unless future world design adds a physical reason |
| Nether/End dimensions | Not required for mechanical parity; Deep Hearth can remain one physically coherent world | `OUT-OF-SCOPE` |
| Explosions/TNT | General impulse, fragmentation, terrain damage and structural failure are useful later | `GAP-P2` |
| Falling sand/gravel | General loose-material/support behavior should supersede hardcoded falling-block classes | `DIFFERENT+`, capability `GAP-P1` |
| Snow layers/freezing | Climate-driven phase/deposition can eventually produce analogous terrain changes | `GAP-P2` |
| Fire blocks | World ignition/spread required with material flammability | `GAP-P1` |
| Lava-water reactions | Material/thermal phase interactions can provide later physical equivalents | `GAP-P2` |
| Redstone dust | Do not clone as universal logic wire | `OUT-OF-SCOPE` exact system |
| Levers/buttons/pressure plates | Familiar local controls and sensors remain useful even with physical networks | `GAP-P1` |
| Pistons | Later actuator capability over world blocks/assemblies | `GAP-P2` |
| Hoppers/droppers/dispensers | Later transport/control equipment should provide equivalent automation categories | `GAP-P2` |
| Note blocks/jukeboxes | Decorative/audio content, not foundational | `OUT-OF-SCOPE` for parity gate |
| Maps/compasses | Navigation tools become valuable once world exploration exists | `GAP-P2` |
| Coordinates/debug screen | Developer/debug projection is useful but should not be the ordinary navigation mechanic | `GAP-P2` tooling |
| Creative mode | Valuable world-building/debug adapter, not survival authority | `GAP-P2` tooling |
| Spectator mode | Useful debug/inspection mode later | `GAP-P2` tooling |
| Difficulty modes | Challenge presets may tune authored pressure, but owner physics should remain coherent | `GAP-P2` product feature |
| Gamerules | Add only when a real customization need appears; avoid a bag of invariant-breaking toggles | `OUT-OF-SCOPE` as a blanket system |
| Commands | Developer/admin command surface can be layered over canonical operations | `GAP-P2` tooling |
| Data/resource packs | External content extensibility is later ecosystem work | `GAP-P2` |
| Multiplayer | Not required for first singleplayer parity; architecture should not preclude it | `GAP-P2` |
| Realms/server hosting | Product/service scope, not simulation parity | `OUT-OF-SCOPE` |
| Scoreboards/teams | Multiplayer/minigame infrastructure, not core survival | `OUT-OF-SCOPE` |
| Boss fights | Content breadth after combat/entity substrate | `OUT-OF-SCOPE` for parity gate |
| Raids | Settlement/combat content, not substrate | `OUT-OF-SCOPE` for parity gate |
| Beacons | Abstract area buffs do not map cleanly to the physical design | `OUT-OF-SCOPE` |
| Ender storage | Nonlocal magical storage conflicts with physical logistics unless future fiction explicitly justifies it | `OUT-OF-SCOPE` |
| Shulker-like portable nested storage | Portable containers can exist, but mass/volume must remain conserved and nested custody explicit | `DIFFERENT+`, later |
| Keep-inventory rule | Death recovery policy may be configurable, but default custody should remain physically explainable | `GAP-P2` policy |
| Mob griefing toggle | Environmental/entity damage should use coherent ownership; debug/custom policy can follow later | `GAP-P2` policy |

## Reference implementation sources

The external comparison was checked against these current Java Edition references:

- [Minecraft Java Edition 26.3 release notes](https://feedback.minecraft.net/hc/en-us/articles/48913133328013-Minecraft-Java-Edition-26-3)
- [How Java Minecraft Works](https://minecraftdocs.dev/), verified by that reference against Java Edition 26.3
- [Server tick](https://minecraftdocs.dev/systems/server/server-tick)
- [Chunk anatomy](https://minecraftdocs.dev/systems/world/chunk-anatomy)
- [Blocks](https://minecraftdocs.dev/systems/blocks/)
- [Scheduled ticks](https://minecraftdocs.dev/systems/world/scheduled-ticks)
- [Fluids](https://minecraftdocs.dev/systems/world/fluids)
- [Movement and collision](https://minecraftdocs.dev/systems/entities/movement-and-collision)
- [Items and inventories](https://minecraftdocs.dev/systems/items/)
- [Items and stacks](https://minecraftdocs.dev/systems/items/items-and-stacks)
- [Containers and menus](https://minecraftdocs.dev/systems/items/containers-and-menus)
- [Section meshing](https://minecraftdocs.dev/systems/rendering/section-meshing)
- [Lighting](https://minecraftdocs.dev/systems/world/lighting)
- [World generation](https://minecraftdocs.dev/systems/worldgen/)
- [Density functions](https://minecraftdocs.dev/systems/worldgen/density-functions)
- [Features and placement](https://minecraftdocs.dev/systems/worldgen/features-and-placement)

These references describe Minecraft; they do not own Deep Hearth behavior. Refresh external details when a future
decision depends on them.
