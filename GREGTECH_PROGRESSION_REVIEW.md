# GregTech progression comparison

**Role:** Comparative content-gap inventory, not implementation authority or a mandate to copy mod-specific recipes.

GregTech is a family of progression systems rather than one identical tree. The comparison
uses [GregTech: New Horizons progression](https://www.gtnewhorizons.com/),
[GregTech 5 ore processing](https://ftb.fandom.com/wiki/GregTech_5/Mining_and_Processing/en),
and [GTNH MV development notes](https://github.com/GTNewHorizons/GTNH-Dev-Doc/blob/master/vision%20of%20the%20modpack/breakdown/tech%20tree/02-MV%20Tier.md).
Deep Hearth follows its own physical simulation and should not add GT power tiers or items
without the resource, manufacturing, distribution, and waste transitions that make them meaningful.
For current reachability, [STATUS.md](STATUS.md) is authoritative.

| GregTech progression | Deep Hearth's current equivalent | Material missing edge |
| --- | --- | --- |
| Primitive/forge age: mining, hammering, manual sorting | Prospecting, physical excavation, hand crushing/grinding, native-copper sorting, timed metalwork | Bulk metallurgy beyond native copper and recycled metal |
| Steam age: boiler, steam macerator, forge hammer, early alloying | Treadle and walking-wheel mechanical work; assembled crusher, grinder, screen and separator | Steam pressure/condensation, steam engines, bronze/copper-tin alloy process |
| Ore beneficiation: maceration, washing, sifting | Conservation-aware crushing, screening, fine grinding, gravity copper/tin concentration and magnetic iron-mineral concentration | Water-consuming ore washer, filtration, wet tailings and liquor custody |
| LV electrical age: machines, wires, EBF, batteries | Wound treadle dynamo, finite electrical buffer, first arc crucible, electrical mineral separator, validated support and wear | Electrical distribution and industrial infrastructure acquisition |
| MV chemistry/thermal: electrolyzer, improved smelter, aluminum and silicon | Temperature/phase change, lead/iron sulfide precipitation, gas-reaction reference equations | Reduction with gas outputs, electrolysis, aluminum extraction, silicon purification, upgraded refractory furnace |
| HV/EV: chemical lines, cleanroom, plastics, advanced circuits | No corresponding ordinary production lines | Pressure chemistry, multistage fluid plants, sealed gases, polymers, semiconductors and cleanrooms |
| IV and beyond: assembly line, high-grade metals, complex power | Limited finite equipment manufacturing and manual/powered labor delegation | Multiblock assembly, transport/conversion networks, advanced metallurgy and factory-scale automation |

## Connected mineral-dressing content

Three GregTech-inspired branches use existing authoritative process owners:

1. Cassiterite (SnO2) is ground to a liberated particle size and enriched by mechanical gravity
   separation. Its target remains cassiterite concentrate rather than tin; tailings retain the
   uncollected mineral and gangue.
2. Magnetite (Fe3O4) is ground and separated by an assembled copper-wound machine consuming finite
   electricity. Cast iron poles, formed copper winding, timber framing and replaceable conductor
   stock are required. An unpowered stone separator cannot substitute; the product remains oxide.
3. Galena (PbS) is mined from rich or 80%-sulfide stone lenses and hand-sorted as coarse
   crystalline fragments at 65% sulfide recovery, leaving real stone and unrecovered PbS
   tailings. The selected output remains pure crushed PbS. One 100 g batch of 80%-grade
   ore yields 52 g selected sulfide, enough for the existing 47.852 g balanced charge
   with 11.169 g reclaimed iron to produce 41.440 g molten lead and 17.581 g FeS matte.
   The actual furnace step requires physical supporting structure, locally accessible
   stock, separate heat-rated lead custody and finite electricity, not a magical
   ore-to-metal conversion.
   Lead ingots and pure reclaimed lead scrap can be electrically remelted
   at 600.61 K and cast again with finite cooling-sink capacity. The early
   stone arc crucible handles at most 20 g per batch; the whole 41.44 g
   reaction product requires multiple charges or a larger foundry. Mixed
   metal and unreduced PbS remain invalid melting feed, so this closes the
   recovered-lead reuse loop without introducing false ore reduction.
   Retained coarse tailings can be reground in an assembled quern, mechanically
   gravity-scavenged at 80% remaining PbS / 10% stone recovery, then cleaned at
   90% PbS recovery into pure crushed sulfide. For the same initial 100 g feed,
   the second route yields another 20.16 g of pure PbS after 24.4 g mixed
   concentrate, leaving 27.84 g of finite exhausted solids across both passes.
   Skipping size reduction or the final cleanup is rejected by the canonical
   processing/form constraints. Combined pure PbS recovered is 72.16 g, not
   fabricated lead metal.

Cassiterite now also has a direct-labor coarse-crystal selection route:
timed player excavation -> hand break -> hand pick visible 2–10 mm mineral
fragments -> separately stored SnO2 and mineral-bearing gangue tailings.
The hand picking stage recovers 55% of the feed's SnO2 and no stone,
while mechanical fine-feed concentration recovers 85% of the oxide and 15%
of the gangue. A constructed stone rotary quern can reduce coarse picking
tailings to 0.5–2 mm; a further gravity pass recovers 70% of the remaining
SnO2 with 10% accompanying stone, leaving finite exhausted tailings.
This is **grade recovery**, not GregTech-style item or ore duplication.

These steps reuse canonical selection, processing time, condition wear, energy consumption,
two-stream output reservations, matter accounting and deterministic save continuation.
The dry mechanical operations preserve mineral identity: only the existing
balanced galena-plus-iron furnace changes PbS into lead, while tin and iron
oxide reduction are still absent. Their practical benefit is selective
recovery and explicit gangue custody, not invented steel or tin production.
**World acquisition:** seeded
four-metre cassiterite and magnetite lenses include fixed 40%-mineral, 60%-stone
variants, while galena has 80%-sulfide, 20%-stone variants, alongside richer
pure-mineral seams. Their full-cell masses obey physical
mixture density, and partial timed player excavation carries the same grade into
inventory or finite surface drops. Crushing, fine grinding and the distinct
separators can then act on the same represented constituents. Locating and
physically reaching deep seams, obtaining the machines and electrical supply,
and eventually reducing concentrate into metal remain real progression work.

## Highest-impact unresolved dependencies

A bronze era needs copper/tin blending, composition-preserving alloy melt/cast semantics and
reduced tin. Ore washing needs measured water input to an actual ore process, wet residues and
mixed solid/fluid custody. Finite water stores now support conserved, player-local pouring
between co-located vessels with exact temperature matching and structural-load validation;
this is preparation for, not an implementation of, washing or remote distribution.
Iron and tin reduction need reductant and oxygen/sulfur destinations, energy budgets and gas
handling rather than deletion of reactant mass. Scaled electrical industry requires power/shaft
and fluid transport, then sealed chemical processing and precision manufacturing.
Block-world interaction and adapter parity remain priorities defined by [DIRECTION.md](DIRECTION.md).
