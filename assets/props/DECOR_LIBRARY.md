# Decor library additions

Eight original, static, matte props for future maps. Origins are bottom center, metres, Y-up. The clock faces +Z. Lamps and candles are unlit; the clock has fixed hands. No changes to the default house placements.

| Asset ID | Native kind | Width × height × depth (m) |
|---|---|---|
| table_lamp_1 | table-lamp | 0.46 × 0.7 × 0.46 |
| book_stack_1 | book-stack | 0.5 × 0.21 × 0.36 |
| candle_trio_1 | candle-trio | 0.5 × 0.36 × 0.36 |
| potted_cactus_1 | potted-cactus | 0.46 × 0.8 × 0.36 |
| flower_vase_1 | flower-vase | 0.6 × 0.8 × 0.5 |
| tall_vase_1 | tall-vase | 0.4 × 0.68 × 0.4 |
| mantel_clock_1 | mantel-clock | 0.58 × 0.44 × 0.24 |
| woven_basket_1 | woven-basket | 0.6 × 0.38 × 0.46 |

Discover with `python tools/author.py query "decor" --limit 20`. Place using, for example:

```sh
python tools/author.py add house.json table_lamp_1 lamp-room.json --id desk-lamp --at=-2,0.8,-3.4
```

Inspect placement and validate routes before adopting a map. Exported asset scenes are visual-only; native placement adds inspection and conservative collision bounds.
