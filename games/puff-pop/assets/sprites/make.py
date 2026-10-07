"""Puff Pop art: Pip the pufferfish, a pearl, and the icon.
Run from the engine checkout: python3 games/puff-pop/assets/sprites/make.py (uses tools/pixelart.py)"""
import sys, pathlib
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / 'tools'))
import pixelart as px

FISH = [
    "................",
    ".....DDDDDD.....",
    "...DDBBBBBBDD...",
    "..DBBBBBBBBBBD..",
    ".DBBBBBBBBBBBBD.",
    ".DBBBBBBBBBWEBBD",
    "DDBBBBBBBBBEEBBD",
    "FDBBBBBBBBBBBBBD",
    "FFDBBBBBBBBBBMMD",
    "FDCCCCCCCCCCCCDD",
    ".DCCCCCCCCCCCCD.",
    ".DDCCCCCCCCCCDD.",
    "..DDCCCCCCCCDD..",
    "...DDDCCCCDDD...",
    "....FF.DDDD.FF..",
    "................",
]
def pal(body, dark, fin, belly):
    return {'B': body, 'D': dark, 'C': belly, 'F': fin, 'W': (255, 255, 255, 255), 'E': (20, 20, 30, 255), 'M': (230, 120, 110, 255)}
PIP = pal((60, 190, 175, 255), (18, 78, 92, 255), (255, 170, 60, 255), (250, 235, 190, 255))
PEARL = ["..WWWW..", ".WWWWWW.", "WWPWWWWW", "WPWWWWWW", "WWWWWWPP", "WWWWWPPP", ".WWPPPP.", "..PPPP.."]
PEARL_PAL = {'W': (250, 248, 240, 255), 'P': (200, 190, 215, 255)}

if __name__ == '__main__':
    (HERE / 'pip.png').write_bytes(px.png_bytes(px.render(FISH, PIP, 4)))
    (HERE / 'pearl.png').write_bytes(px.png_bytes(px.render(PEARL, PEARL_PAL, 4)))
    art = px.render(FISH, PIP, 11)
    px.write_icon_set(HERE.parent, art, (36, 120, 190, 255), (20, 70, 130, 255), 256, scale=1)
    print('ok')
