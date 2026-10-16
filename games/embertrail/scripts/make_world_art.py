#!/usr/bin/env python3
"""Embertrail's original pixel silhouettes, authored at their display resolution.

Pure standard library; deliberately independent of music authoring and the engine
checkout so the game's source art remains reproducible after an engine upgrade.
"""
from pathlib import Path
import struct
import sys
import zlib

OUT = Path(__file__).resolve().parents[1] / 'assets' / 'sprites'
OUT.mkdir(parents=True, exist_ok=True)
INK = '#152b36'
PAPER = '#fff3cb'
GOLD = '#f5bc65'
MINT = '#a4f1ce'


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.pixels = [[(0, 0, 0, 0) for _ in range(w)] for _ in range(h)]

    def dot(self, x, y, color):
        if 0 <= x < self.w and 0 <= y < self.h:
            if isinstance(color, str):
                color = tuple(bytes.fromhex(color[1:])) + (255,)
            self.pixels[y][x] = color

    def rect(self, x, y, w, h, color):
        for j in range(y, y + h):
            for i in range(x, x + w):
                self.dot(i, j, color)

    def ellipse(self, x, y, w, h, color):
        for j in range(y, y + h):
            for i in range(x, x + w):
                if ((i + .5 - x - w / 2) / (w / 2)) ** 2 + ((j + .5 - y - h / 2) / (h / 2)) ** 2 <= 1:
                    self.dot(i, j, color)

    def poly(self, points, color):
        for y in range(min(p[1] for p in points), max(p[1] for p in points) + 1):
            crosses = []
            for a, b in zip(points, points[1:] + points[:1]):
                if (a[1] <= y < b[1]) or (b[1] <= y < a[1]):
                    crosses.append(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]))
            crosses.sort()
            for left, right in zip(crosses[::2], crosses[1::2]):
                self.rect(int(left), y, max(1, int(right) - int(left) + 1), 1, color)

    def line(self, a, b, color, width=1):
        dx, dy = b[0] - a[0], b[1] - a[1]
        steps = max(abs(dx), abs(dy), 1)
        for t in range(steps + 1):
            self.rect(round(a[0] + dx * t / steps), round(a[1] + dy * t / steps), width, width, color)

    def save(self, name, mirror=False):
        raw = b''.join(b'\0' + bytes(c for px in (row[::-1] if mirror else row) for c in px) for row in self.pixels)
        def chunk(kind, data):
            return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
        png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', self.w, self.h, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')
        (OUT / (name + '.png')).write_bytes(png)


def mote(frame):
    c = Canvas(32, 40)
    hop = -1 if frame == 1 else 0
    # A trailing saffron scarf and nose/eye at the leading edge make facing literal.
    scarf_y = 21 + (frame % 2)
    c.poly([(15, 20), (8, scarf_y), (2, scarf_y - 3), (0, scarf_y + 1), (7, scarf_y + 5), (16, 24)], INK)
    c.poly([(15, 21), (8, scarf_y + 1), (2, scarf_y - 1), (2, scarf_y + 1), (8, scarf_y + 3), (16, 23)], GOLD)
    c.rect(3, scarf_y, 3, 1, PAPER)
    # The flame is a living sprout: quiet, recognizable, and pointed forward.
    c.poly([(13, 9 + hop), (11, 6 + hop), (15, 2 + hop), (18, 0), (18, 5 + hop), (21, 4 + hop), (19, 10 + hop)], '#448b7a')
    c.poly([(14, 7 + hop), (17, 3 + hop), (17, 7 + hop), (19, 6 + hop), (18, 9 + hop)], MINT)
    # Lantern head in a dark teal hood, with the face set toward the right.
    c.poly([(12, 8 + hop), (22, 8 + hop), (27, 12 + hop), (27, 22 + hop), (23, 25 + hop), (12, 24 + hop), (8, 19 + hop), (8, 13 + hop)], INK)
    c.poly([(12, 10 + hop), (21, 10 + hop), (25, 13 + hop), (25, 21 + hop), (21, 23 + hop), (12, 22 + hop), (10, 18 + hop), (10, 14 + hop)], '#397a6e')
    c.poly([(18, 11 + hop), (24, 13 + hop), (25, 16 + hop), (29, 17 + hop), (28, 20 + hop), (24, 20 + hop), (22, 23 + hop), (17, 22 + hop), (15, 18 + hop), (15, 14 + hop)], PAPER)
    c.rect(17, 13 + hop, 2, 7, '#e7c995')
    # A small inset warm pupil stays separate from the hood's dark outline.
    c.rect(22, 15 + hop, 2, 3, '#514b43')
    c.rect(22, 15 + hop, 1, 1, '#a69b7b')
    c.rect(25, 20 + hop, 2, 1, '#b77558')
    c.rect(11, 13 + hop, 2, 6, '#63bca2')
    # Cloak, lantern clasp and small boots stay readable against every biome.
    c.poly([(13, 23 + hop), (22, 23 + hop), (24, 32), (20, 35), (11, 34), (8, 31)], INK)
    c.poly([(13, 24 + hop), (21, 24 + hop), (22, 31), (19, 33), (11, 32)], '#4da990')
    c.rect(13, 25 + hop, 3, 6, '#8edbb9')
    c.rect(17, 24 + hop, 5, 3, GOLD)
    c.rect(18, 25 + hop, 2, 1, PAPER)
    c.rect(22, 27 + hop, 4, 4, INK)
    c.rect(23, 27 + hop, 3, 2, PAPER)
    legs = [(12, 34, 5, 4), (20, 34, 5, 4)]
    if frame == 1:
        legs = [(9, 33, 5, 4), (21, 35, 7, 3)]
    elif frame == 2:
        legs = [(13, 35, 7, 3), (21, 32, 5, 4)]
    elif frame == 3:
        legs = [(9, 32, 6, 4), (22, 31, 6, 4)]
    for x, y, w, h in legs:
        c.rect(x, y, w, h, INK)
        c.rect(x + 1, y, w - 2, 1, '#a17b57')
    c.save('mote' + str(frame))
    c.save('mote_left' + str(frame), True)


def small_enemies():
    c = Canvas(32, 26)
    c.line((5, 20), (2, 24), INK, 2); c.line((23, 20), (28, 24), INK, 2)
    c.ellipse(3, 6, 26, 18, INK); c.ellipse(5, 7, 22, 14, '#85465e')
    c.ellipse(7, 8, 17, 8, '#b2687a')
    c.poly([(7, 9), (5, 2), (11, 6), (15, 1), (17, 7), (25, 3), (23, 11)], '#718370')
    c.rect(9, 14, 5, 5, '#fff0b9'); c.rect(20, 14, 5, 5, '#fff0b9')
    c.rect(10, 15, 2, 3, INK); c.rect(21, 15, 2, 3, INK)
    c.rect(15, 20, 5, 1, '#e2a187'); c.save('mite')
    c = Canvas(32, 26)
    for x in (4, 13, 23): c.line((x + 3, 18), (x, 24), INK, 2)
    c.ellipse(2, 3, 28, 19, INK); c.ellipse(4, 4, 24, 16, '#ac704d')
    c.ellipse(6, 5, 20, 10, '#d8a45c'); c.line((16, 5), (16, 18), '#6b5149', 2)
    c.rect(8, 8, 4, 2, '#ffe3a0'); c.rect(21, 9, 3, 2, '#ffe3a0')
    c.rect(11, 19, 5, 4, PAPER); c.rect(22, 18, 5, 4, PAPER)
    c.rect(14, 20, 2, 3, INK); c.rect(25, 19, 2, 3, INK); c.save('beetle')
    c = Canvas(32, 26)
    c.ellipse(2, 12, 28, 12, INK); c.ellipse(4, 13, 24, 9, '#ac704d')
    c.ellipse(7, 11, 18, 10, '#d8a45c'); c.rect(7, 20, 20, 3, '#684e47')
    c.rect(10, 13, 4, 2, '#ffe3a0'); c.line((17, 12), (17, 20), '#7b5849'); c.save('shell')
    c = Canvas(32, 26)
    c.poly([(5, 17), (0, 11), (2, 5), (10, 12), (15, 2), (20, 12), (28, 5), (31, 11), (26, 20)], '#827a9e')
    c.poly([(5, 14), (2, 8), (11, 15), (15, 6), (19, 15), (28, 8), (26, 16)], '#c6b7d4')
    c.ellipse(8, 12, 17, 13, INK); c.ellipse(10, 13, 13, 10, '#7b719a')
    c.rect(12, 16, 3, 4, GOLD); c.rect(19, 16, 3, 4, GOLD)
    c.rect(12, 16, 1, 3, PAPER); c.rect(19, 16, 1, 3, PAPER); c.save('wisp')
    c = Canvas(32, 56)
    c.line((16, 53), (16, 20), INK, 5); c.line((17, 52), (17, 21), '#456c5b', 2)
    for y in (28, 41):
        c.poly([(17, y + 5), (6, y), (3, y - 6), (14, y - 2)], '#6c9270')
        c.poly([(18, y + 6), (28, y), (30, y - 5), (20, y)], '#517860')
    c.poly([(3, 12), (5, 4), (12, 7), (16, 0), (21, 7), (28, 3), (30, 14), (26, 24), (6, 24)], INK)
    c.poly([(5, 13), (7, 7), (13, 10), (16, 4), (20, 10), (26, 7), (28, 14), (24, 21), (8, 21)], '#b06679')
    c.rect(9, 12, 5, 4, PAPER); c.rect(20, 12, 5, 4, PAPER)
    c.rect(12, 13, 2, 3, INK); c.rect(23, 13, 2, 3, INK)
    c.rect(12, 20, 10, 2, '#5e3e51'); c.save('thorn')


def bell(c, x, y, w, h, metal, light):
    c.ellipse(x + w // 2 - 4, y - 5, 8, 8, INK)
    c.ellipse(x + w // 2 - 2, y - 3, 4, 4, metal)
    c.poly([(x + 7, y), (x + w - 8, y), (x + w - 4, y + h - 8), (x + w, y + h - 5), (x + w, y + h), (x, y + h), (x, y + h - 5), (x + 4, y + h - 8)], INK)
    c.poly([(x + 9, y + 2), (x + w - 10, y + 2), (x + w - 6, y + h - 7), (x + w - 2, y + h - 4), (x + w - 2, y + h - 2), (x + 2, y + h - 2), (x + 2, y + h - 4), (x + 6, y + h - 7)], metal)
    c.line((x + 10, y + 4), (x + 8, y + h - 9), light, 2)
    c.rect(x + 4, y + h - 5, w - 8, 2, light)
    c.rect(x + w // 2 - 3, y + h, 6, 3, GOLD)


def bosses():
    for world in range(8):
        c = Canvas(80, 84)
        body = ['#547967', '#756b91', '#92715b', '#8ab2b7', '#b67a57', '#829bab', '#88677b', '#695c81'][world]
        bright = ['#a5c48b', '#b9afd3', '#ddb274', '#d6f5e8', '#ffcf7f', '#c8e5df', '#d59ca4', '#baa3c6'][world]
        # All wardens carry a stolen dawn bell, but each silhouette belongs to its biome.
        if world == 0:
            for a, b in [((17, 35), (7, 10)), ((61, 35), (72, 10)), ((10, 21), (1, 17)), ((68, 21), (78, 16))]:
                c.line(a, b, INK, 5); c.line(a, b, '#796e4f', 3)
            for x, y in [(2, 10), (11, 6), (65, 5), (71, 13)]:
                c.poly([(x, y + 7), (x + 2, y), (x + 9, y + 1), (x + 7, y + 8)], '#a5c48b')
            c.ellipse(11, 32, 57, 42, INK); c.ellipse(14, 33, 51, 36, body)
            c.ellipse(21, 26, 38, 38, INK); c.ellipse(24, 28, 32, 31, bright)
            c.ellipse(25, 45, 32, 15, '#6f8c6b')
        elif world == 1:
            c.ellipse(6, 21, 67, 51, INK); c.ellipse(9, 23, 61, 43, body)
            for x in (14, 28, 45, 58):
                c.poly([(x, 29), (x + 3, 8 + x % 13), (x + 8, 24)], '#b9afd3')
            for x in (3, 65):
                c.line((x + 8, 57), (x, 70), INK, 5)
            c.ellipse(20, 40, 40, 27, '#4b4b66')
        elif world == 2:
            c.ellipse(3, 46, 27, 27, INK); c.ellipse(51, 46, 27, 27, INK)
            c.ellipse(6, 47, 21, 22, body); c.ellipse(54, 47, 21, 22, body)
            c.ellipse(11, 22, 58, 49, INK); c.ellipse(14, 24, 52, 43, body)
            c.ellipse(17, 17, 19, 23, INK); c.ellipse(45, 17, 19, 23, INK)
            c.ellipse(20, 20, 13, 17, bright); c.ellipse(48, 20, 13, 17, bright)
            for x in (7, 64): c.rect(x, 68, 11, 5, '#b8905f')
        elif world == 3:
            for side in (-1, 1):
                for a, b in [((40 + side * 17, 36), (40 + side * 26, 8)), ((40 + side * 21, 23), (40 + side * 36, 17)), ((40 + side * 25, 12), (40 + side * 30, 2))]:
                    c.line(a, b, INK, 4); c.line(a, b, '#d6f5e8', 2)
            c.ellipse(14, 36, 52, 34, INK); c.ellipse(17, 37, 46, 29, body)
            c.poly([(24, 27), (40, 19), (56, 28), (52, 53), (40, 62), (29, 51)], INK)
            c.poly([(27, 28), (40, 22), (53, 29), (49, 51), (40, 58), (32, 50)], bright)
        elif world == 4:
            c.ellipse(12, 37, 59, 33, INK); c.ellipse(15, 39, 53, 27, body)
            c.line((66, 50), (75, 40), INK, 4); c.line((74, 42), (73, 30), GOLD, 3)
            c.poly([(40, 9), (47, 19), (61, 15), (61, 29), (72, 36), (64, 47), (65, 60), (51, 59), (42, 68), (32, 59), (18, 61), (19, 47), (10, 38), (22, 29), (22, 16), (34, 20)], INK)
            c.poly([(40, 13), (47, 23), (58, 19), (58, 31), (67, 36), (60, 45), (61, 56), (49, 56), (42, 63), (33, 55), (22, 57), (23, 44), (15, 38), (26, 31), (26, 21), (35, 25)], GOLD)
            c.ellipse(24, 27, 35, 30, body); c.ellipse(29, 43, 27, 14, bright)
            c.poly([(35, 45), (45, 45), (40, 51)], INK)
            for a, b in [((40, 16), (40, 26)), ((19, 37), (27, 37)), ((60, 36), (64, 36)), ((30, 54), (27, 59))]:
                c.line(a, b, PAPER, 2)
        elif world == 5:
            # Broad articulated wings make the storm heron's low sweep readable.
            for side in (-1, 1):
                points = [(40 + side * 11, 43), (40 + side * 31, 18), (40 + side * 39, 14), (40 + side * 33, 38), (40 + side * 23, 58), (40 + side * 10, 60)]
                c.poly(points, INK)
                c.poly([(x - side * 2, y + 2) for x, y in points], body)
                for n in range(3): c.line((40 + side * (14 + n * 6), 45 - n * 6), (40 + side * (18 + n * 5), 56 - n * 8), bright, 2)
            c.ellipse(24, 28, 32, 43, INK); c.ellipse(27, 29, 26, 36, body)
            c.poly([(26, 30), (20, 18), (33, 25), (41, 15), (48, 27), (61, 24), (55, 36)], bright)
            c.poly([(34, 39), (40, 34), (47, 40), (40, 48)], GOLD)
        elif world == 6:
            for side in (-1, 1):
                for a, b in [((40 + side * 18, 44), (40 + side * 34, 25)), ((40 + side * 30, 31), (40 + side * 36, 8)), ((40 + side * 27, 34), (40 + side * 14, 15))]:
                    c.line(a, b, INK, 6); c.line(a, b, '#61715e', 3)
            c.poly([(23, 31), (32, 20), (49, 20), (58, 32), (65, 73), (52, 69), (41, 77), (27, 70), (13, 76)], INK)
            c.poly([(26, 33), (34, 22), (47, 23), (55, 34), (59, 69), (42, 73), (20, 70)], body)
            for x, y in [(21, 25), (49, 18), (12, 12), (60, 7)]:
                c.ellipse(x, y, 13, 12, '#b26c82'); c.rect(x + 5, y + 4, 4, 4, GOLD)
        else:
            c.poly([(22, 36), (59, 36), (75, 76), (57, 72), (45, 81), (27, 74), (5, 79)], INK)
            c.poly([(24, 39), (56, 39), (69, 73), (55, 69), (43, 76), (27, 70), (11, 75)], '#65445e')
            for x in (20, 31, 52): c.line((x + 6, 46), (x, 72), '#a27086', 2)
            c.line((67, 19), (67, 75), INK, 4); c.line((68, 21), (68, 74), GOLD, 2)
            bell(c, 59, 13, 19, 17, GOLD, PAPER)
        if world not in (4, 5):
            for x in (21, 49):
                c.rect(x, 64, 9, 13, INK); c.rect(x + 1, 66, 6, 8, body)
        # Bell masks are the Regent's curse; amber eye slits make the family clear.
        if world == 7:
            bell(c, 22, 22, 36, 28, '#b28b65', '#f3d599')
            c.poly([(24, 21), (21, 7), (31, 14), (40, 3), (49, 14), (59, 7), (56, 21)], INK)
            c.poly([(27, 19), (25, 11), (33, 17), (40, 8), (47, 17), (55, 11), (53, 19)], GOLD)
            # The stolen Dawnheart is a visible object in his crown, matching home's socket.
            c.poly([(40, 10), (44, 15), (40, 20), (36, 15)], MINT)
            c.rect(39, 12, 2, 5, PAPER)
            c.rect(29, 33, 10, 3, INK); c.rect(44, 33, 9, 3, INK)
            c.rect(30, 34, 7, 1, '#ffad72'); c.rect(45, 34, 6, 1, '#ffad72')
            c.rect(36, 42, 11, 2, '#765345')
        else:
            for x in (25, 47):
                c.rect(x, 38, 9, 7, INK); c.rect(x + 2, 40, 6, 3, GOLD)
            bell(c, 28, 52, 26, 19, '#ac8055', '#f5d28e')
        c.save('boss' + str(world))


def villager():
    c = Canvas(32, 42)
    c.poly([(4, 35), (9, 19), (22, 19), (28, 35)], INK)
    c.poly([(7, 33), (11, 21), (20, 21), (25, 33)], '#a27b58')
    c.ellipse(6, 7, 23, 18, INK); c.ellipse(8, 9, 19, 14, PAPER)
    c.ellipse(3, 4, 25, 11, '#9c695a'); c.ellipse(6, 3, 19, 7, '#cb9672')
    c.rect(13, 15, 2, 3, INK); c.rect(22, 15, 2, 3, INK)
    c.rect(16, 22, 5, 3, GOLD); c.rect(11, 35, 4, 4, INK); c.rect(20, 35, 4, 4, INK)
    c.line((28, 21), (28, 39), '#b8a17d', 2); c.save('villager')


for i in range(4): mote(i)
if '--mote-only' in sys.argv:
    print('Authored Mote profiles only; other world assets were left untouched.')
else:
    small_enemies()
    bosses()
    villager()
    print('Authored Mote, five cursed woodland creatures, eight bell wardens and a villager.')
