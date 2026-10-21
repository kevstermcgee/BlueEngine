"""Render the game's geometric tile icon, including native Windows shell sizes.

No external dependencies. Small ICO frames use BGRA DIBs plus AND masks;
large frames use RGBA PNGs. All window blobs share their exact ICO pixels.
"""
from pathlib import Path
import math
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1] / 'assets'
SIZES = (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
PALETTE = ((46, 212, 242), (171, 112, 247), (250, 204, 64), (91, 224, 158))
TILES = ((1,1,0),(2,1,0),(3,1,0),(4,1,0),(2,3,1),(1,4,1),(2,4,1),(3,4,1),
         (5,3,2),(6,3,2),(5,4,2),(6,4,2),(4,5,3),(5,5,3),(3,6,3),(4,6,3))


def png(size, rgba):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind+data))
    rows = b''.join(b'\0'+rgba[y*size*4:(y+1)*size*4] for y in range(size))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size,size,8,6,0,0,0))
            + chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b''))


def render(size):
    pixels = bytearray()
    for py in range(size):
        for px in range(size):
            samples=[]
            for oy,ox in ((.25,.25),(.25,.75),(.75,.25),(.75,.75)):
                x,y=(px+ox)/size,(py+oy)/size
                # Rounded tile with genuinely transparent outer corners.
                dx,dy=max(abs(x-.5)-.29,0),max(abs(y-.5)-.29,0)
                if math.hypot(dx,dy)>.155:
                    samples.append((0,0,0,0));continue
                light=1-y*.35+x*.12
                color=(int(17*light),int(28*light),int(48*light),255)
                for tx,ty,k in TILES:
                    u,v=x*8-tx,y*8-ty
                    if .035<u<.965 and .035<v<.965:
                        tint=1.12-v*.32
                        if v<.16:tint+=.16
                        if u>.83:tint*=.66
                        color=tuple(min(255,int(c*tint)) for c in PALETTE[k])+(255,)
                        break
                samples.append(color)
            alpha=sum(s[3] for s in samples)//4
            count=sum(s[3]>0 for s in samples)
            rgb=[sum(s[k] for s in samples)//count if count else 0 for k in range(3)]
            pixels.extend((*rgb,alpha))
    return bytes(pixels)


def dib(size, rgba):
    stride=((size+31)//32)*4
    mask=bytearray(stride*size)
    bgra=bytearray()
    for row,y in enumerate(range(size-1,-1,-1)):
        for x in range(size):
            r,g,b,a=rgba[(y*size+x)*4:(y*size+x+1)*4]
            bgra.extend((b,g,r,a))
            if a<128:mask[row*stride+x//8] |= 1 << (7-x%8)
    return struct.pack('<IiiHHIIiiII',40,size,size*2,1,32,0,len(bgra)+len(mask),0,0,0,0)+bgra+mask


def main():
    images=[(size,render(size)) for size in SIZES]
    payloads=[(size,png(size,pixels) if size>=64 else dib(size,pixels)) for size,pixels in images]
    offset=6+16*len(payloads)
    directory=bytearray();data=bytearray()
    for size,payload in payloads:
        directory.extend(struct.pack('<BBBBHHII',size%256,size%256,0,0,1,32,len(payload),offset))
        data.extend(payload);offset+=len(payload)
    (ROOT/'icon.ico').write_bytes(struct.pack('<HHH',0,1,len(payloads))+directory+data)
    for size,pixels in images:
        if size in (16,32,64):(ROOT/f'icon_{size}.rgba').write_bytes(pixels)
        if size==256:(ROOT/'icon.png').write_bytes(png(size,pixels))


if __name__=='__main__':
    main()
