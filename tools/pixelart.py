"""Pure-Python pixel art export: ASCII maps -> PNG (scaled), raw RGBA icon blobs and a Windows ICO.

render(map_lines, palette, scale) -> list of RGBA rows; png_bytes(rows); ico_bytes(frames); rgba_bytes(rows)
A map is a list of equal-length strings; palette maps a character to (r,g,b,a). '.' is transparent by default.
"""
import struct, zlib

def render(lines, palette, scale=1, background=None):
    h, w = len(lines), len(lines[0])
    rows = []
    for y in range(h):
        assert len(lines[y]) == w, f"row {y} has width {len(lines[y])}, expected {w}"
        row = []
        for x in range(w):
            c = lines[y][x]
            px = palette.get(c, (0, 0, 0, 0)) if c != '.' else palette.get('.', (0, 0, 0, 0))
            if px[3] == 0 and background is not None:
                px = background
            row.extend([px] * scale)
        for _ in range(scale):
            rows.append(list(row))
    return rows

def composite(base, overlay, ox, oy):
    """Alpha-blend overlay rows onto base rows at (ox, oy); returns base."""
    for y, row in enumerate(overlay):
        for x, px in enumerate(row):
            if px[3] == 0:
                continue
            by, bx = oy + y, ox + x
            if 0 <= by < len(base) and 0 <= bx < len(base[0]):
                if px[3] == 255:
                    base[by][bx] = px
                else:
                    a = px[3] / 255
                    b = base[by][bx]
                    base[by][bx] = tuple(int(px[i] * a + b[i] * (1 - a)) for i in range(3)) + (max(b[3], px[3]),)
    return base

def solid(w, h, color):
    return [[color] * w for _ in range(h)]

def rounded_tile(size, color, radius, edge=None):
    rows = solid(size, size, (0, 0, 0, 0))
    for y in range(size):
        for x in range(size):
            cx = min(max(x, radius), size - 1 - radius)
            cy = min(max(y, radius), size - 1 - radius)
            d2 = (x - cx) ** 2 + (y - cy) ** 2
            if d2 <= radius * radius:
                rows[y][x] = color
                if edge and d2 > (radius - size // 32 - 1) ** 2 and (x < radius or x >= size - radius or y < radius or y >= size - radius):
                    rows[y][x] = edge
                elif edge and (x < size // 32 or x >= size - size // 32 or y < size // 32 or y >= size - size // 32):
                    rows[y][x] = edge
    return rows

def resample(rows, size):
    """Nearest-neighbour resample of square-ish rows to size x size (for 16/32/64 icon blobs)."""
    h, w = len(rows), len(rows[0])
    return [[rows[y * h // size][x * w // size] for x in range(size)] for y in range(size)]

def png_bytes(rows):
    h, w = len(rows), len(rows[0])
    raw = b''.join(b'\x00' + b''.join(struct.pack('BBBB', *px) for px in row) for row in rows)
    def chunk(tag, data):
        c = struct.pack('>I', len(data)) + tag + data
        return c + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))

def rgba_bytes(rows):
    return b''.join(struct.pack('BBBB', *px) for row in rows for px in row)

def ico_bytes(frames):
    """frames: list of rows (square). Each stored as a 32-bit BGRA DIB with an AND mask."""
    entries, data = [], b''
    offset = 6 + 16 * len(frames)
    for rows in frames:
        s = len(rows)
        dib = struct.pack('<IiiHHIIiiII', 40, s, s * 2, 1, 32, 0, s * s * 4 + ((s + 31) // 32 * 4) * s, 0, 0, 0, 0)
        pixels = b''.join(struct.pack('BBBB', px[2], px[1], px[0], px[3]) for row in reversed(rows) for px in row)
        stride = (s + 31) // 32 * 4
        mask = b''
        for row in reversed(rows):
            bits = 0
            for x, px in enumerate(row):
                if px[3] == 0:
                    bits |= 1 << (stride * 8 - 1 - x)
            mask += bits.to_bytes(stride, 'big')
        image = dib + pixels + mask
        entries.append(struct.pack('<BBBBHHII', s % 256, s % 256, 0, 0, 1, 32, len(image), offset))
        offset += len(image)
        data += image
    return struct.pack('<HHH', 0, 1, len(frames)) + b''.join(entries) + data

def write_icon_set(assets_dir, art_rows, tile_color, edge_color, size=256, scale=None):
    """icon.png (256), icon_16/32/64.rgba and icon.ico from square art rows centred on a rounded tile."""
    import pathlib
    assets = pathlib.Path(assets_dir)
    tile = rounded_tile(size, tile_color, size // 5, edge_color)
    art = art_rows
    if scale is None:
        scale = max(1, (size * 3 // 4) // len(art))
    big = render([''.join('#' if px[3] else '.' for px in row) for row in art], {'#': (0, 0, 0, 255)}, 1)  # placeholder sizing
    scaled = [[px for px in row for _ in range(scale)] for row in art for _ in range(scale)]
    ox = (size - len(scaled[0])) // 2
    oy = (size - len(scaled)) // 2
    composite(tile, scaled, ox, oy)
    (assets / 'icon.png').write_bytes(png_bytes(tile))
    for s in (16, 32, 64):
        (assets / f'icon_{s}.rgba').write_bytes(rgba_bytes(resample(tile, s)))
    (assets / 'icon.ico').write_bytes(ico_bytes([resample(tile, s) for s in (16, 32, 48, 64)] + [resample(tile, 128)]))
    return tile
