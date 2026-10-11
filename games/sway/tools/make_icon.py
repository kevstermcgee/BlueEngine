"""Generate Sway's own waveform mark as PNG, ICO and native window RGBA sizes."""
import math
from pathlib import Path
import struct
import zlib
ROOT = Path(__file__).resolve().parents[1] / 'assets'
def rgba(n):
    result = bytearray()
    for y in range(n):
        for x in range(n):
            u, v = (x+.5)/n, (y+.5)/n
            edge = min(u, v, 1-u, 1-v)
            r, g, b = 0.024, 0.036, 0.075
            for line in range(5):
                curve = .5 + .19*math.sin((u-.17)*math.tau*1.05) + (line-2)*.027
                dist = abs(v-curve)
                mask = min(1.,max(0.,(u-.12)*24))*min(1.,max(0.,(.88-u)*24))
                light = (math.exp(-dist*dist/.00006) + .13*math.exp(-dist*dist/.0016))*mask
                r += light*(.36+.34*u); g += light*(.84-.49*u); b += light*.94
            alpha = min(1.,max(0.,(edge-.016)*n))
            result.extend((min(255,int(r*255)),min(255,int(g*255)),min(255,int(b*255)),int(alpha*255)))
    return bytes(result)
def png(n, pixels):
    def chunk(k, d):
        return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d))
    scan = b''.join(b'\0'+pixels[y*n*4:(y+1)*n*4] for y in range(n))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',n,n,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(scan,9))+chunk(b'IEND',b'')
def bmp(n, pixels):
    rows = bytearray()
    mask = bytearray()
    stride = ((n+31)//32)*4
    for y in range(n-1,-1,-1):
        bits = bytearray(stride)
        for x in range(n):
            r,g,b,a = pixels[(y*n+x)*4:(y*n+x+1)*4]
            rows.extend((b,g,r,a))
            if a == 0: bits[x//8] |= 1 << (7-x%8)
        mask.extend(bits)
    return struct.pack('<IiiHHIIiiII',40,n,n*2,1,32,0,len(rows)+len(mask),0,0,0,0)+rows+mask
if __name__ == '__main__':
    icons=[]
    for n in (16,20,24,32,40,48,64,96,128,256):
        pixels=rgba(n); data=png(n,pixels) if n>=64 else bmp(n,pixels); icons.append((n,data))
        if n in (16,32,64): (ROOT/f'icon_{n}.rgba').write_bytes(pixels)
        if n==256: (ROOT/'icon.png').write_bytes(png(n,pixels))
    offset=6+16*len(icons); records=[]
    for n,data in icons:
        records.append(struct.pack('<BBBBHHII',n%256,n%256,0,0,1,32,len(data),offset));offset+=len(data)
    (ROOT/'icon.ico').write_bytes(struct.pack('<HHH',0,1,len(icons))+b''.join(records)+b''.join(d for _,d in icons))
