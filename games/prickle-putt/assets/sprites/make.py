"""Prickle Putt art: two rolled-up hedgehogs (16x16 maps, exported at 4x) and the game icon.
Run from the engine checkout: python3 games/prickle-putt/assets/sprites/make.py  (uses tools/pixelart.py)"""
import sys, pathlib
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / 'tools'))
import pixelart as px

HEDGEHOG = [
    "....DD.DD.DD....",
    "..DDDBDDBDDBDD..",
    ".DDBBBBBBBBBBD..",
    ".DBBBBBBBBBBBDD.",
    "DDBBBBBBBBBBCCCD",
    "DBBBBBBBBBBCCCCC",
    "DBBBBBBBBBCCECCC",
    "DBBBBBBBBBCCCCCN",
    "DBBBBBBBBBCCCCCN",
    "DBBBBBBBBBCCCCCC",
    "DBBBBBBBBBBCCCCD",
    "DDBBBBBBBBBBCCD.",
    ".DBBBBBBBBBBBDD.",
    ".DDBBBBBBBBBBD..",
    "..DDDBDDBDDBDD..",
    "....DD.DD.DD....",
]
def palette(body, dark, band):
    return {'D': dark, 'B': body, 'C': (247, 228, 190, 255), 'E': (30, 20, 20, 255), 'N': (40, 24, 20, 255), 'R': band}

PRICKLE = palette((150, 96, 52, 255), (82, 48, 26, 255), (70, 140, 230, 255))
BRAMBLE = palette((110, 62, 40, 255), (52, 28, 18, 255), (235, 120, 50, 255))

def with_band(lines, band_rows=(3, 4)):
    out = list(lines)
    for r in band_rows:
        out[r] = ''.join('R' if c == 'B' and 3 <= i <= 9 else c for i, c in enumerate(out[r]))
    return out

if __name__ == '__main__':
    for name, pal in (('prickle', PRICKLE), ('bramble', BRAMBLE)):
        rows = px.render(with_band(HEDGEHOG), pal, 4)
        (HERE / f'{name}.png').write_bytes(px.png_bytes(rows))
    # Icon: Prickle on a putting-green tile with a cup and flag.
    tile_art = px.render(with_band(HEDGEHOG), PRICKLE, 10)
    px.write_icon_set(HERE.parent, tile_art, (62, 150, 78, 255), (34, 96, 48, 255), 256, scale=1)
    print('wrote prickle.png bramble.png and icon set')
