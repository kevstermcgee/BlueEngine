"""Topsy-Turvy art: Dusk the bat, a gem, and the icon.
Run from the engine checkout: python3 games/topsy-turvy/assets/sprites/make.py (uses tools/pixelart.py)"""
import sys, pathlib
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / 'tools'))
import pixelart as px

BAT = [
    "..E..........E..",
    ".EEE........EEE.",
    ".EBEE......EEBE.",
    ".EBBBEEEEEEBBBE.",
    "WEBBBBBBBBBBBBEW",
    "WWEBBYYBBYYBBEWW",
    "WWWEBBKYBKYBEWWW",
    "WWWWEBBBBBBEWWWW",
    ".WWWWEBFFBEWWWW.",
    "..WWWWEBBBEWWW..",
    "...WWWWEBEWWW...",
    "....WW.EEE.WW...",
    ".....W.T.T.W....",
    "........T.T.....",
    "................",
    "................",
]
def pal(body, ear, wing, eye):
    return {'B': body, 'E': ear, 'W': wing, 'F': (255, 255, 255, 255), 'Y': (255, 255, 255, 255), 'K': (20, 10, 30, 255), 'T': (240, 190, 90, 255)}
DUSK = pal((120, 80, 190, 255), (60, 36, 110, 255), (86, 56, 150, 255), None)
GEM = ["...CC...", "..CDDC..", ".CDDDDC.", "CDDWDDDC", ".CDDDDC.", "..CDDC..", "...CC...", "........"]
GEM_PAL = {'C': (40, 150, 200, 255), 'D': (110, 220, 250, 255), 'W': (255, 255, 255, 255)}

if __name__ == '__main__':
    (HERE / 'dusk.png').write_bytes(px.png_bytes(px.render(BAT, DUSK, 4)))
    (HERE / 'gem.png').write_bytes(px.png_bytes(px.render(GEM, GEM_PAL, 4)))
    art = px.render(BAT, DUSK, 11)
    px.write_icon_set(HERE.parent, art, (40, 30, 80, 255), (90, 60, 160, 255), 256, scale=1)
    print('ok')
