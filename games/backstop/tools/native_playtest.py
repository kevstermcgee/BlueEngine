#!/usr/bin/env python3
"""Linux software-display control inspection using real X11 key events.

Runs the unmodified native client; reads engine-written saves only to assert observations.
Never writes gameplay state. Captures are PNGs made from the actual X framebuffer.
"""
import argparse
import ctypes as C
import ctypes.util
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import time
import zlib

ROOT = Path(__file__).resolve().parents[1]

def save_payload(path):
    data = path.read_bytes()
    assert data[:8] == b'BE2SAVE\x1a'
    assert hashlib.sha256(data[:-32]).digest() == data[-32:]
    h, n = struct.unpack_from('<IQ', data, 12)
    payload = json.loads(data[24+h:24+h+n])
    # Runtime snapshots wrap the game-owned State with the authoritative hash.
    return payload.get('state', payload)

class XImage(C.Structure):
    _fields_ = [('width', C.c_int), ('height', C.c_int), ('xoffset', C.c_int), ('format', C.c_int),
                ('data', C.c_void_p), ('byte_order', C.c_int), ('bitmap_unit', C.c_int),
                ('bitmap_bit_order', C.c_int), ('bitmap_pad', C.c_int), ('depth', C.c_int),
                ('bytes_per_line', C.c_int), ('bits_per_pixel', C.c_int),
                ('red_mask', C.c_ulong), ('green_mask', C.c_ulong), ('blue_mask', C.c_ulong)]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--out', type=Path, default=ROOT / '.blue-check/native-controls')
    args = parser.parse_args()
    out = args.out.resolve();out.mkdir(parents=True, exist_ok=False)
    display = next(f':{n}' for n in range(170, 220) if not Path(f'/tmp/.X{n}-lock').exists())
    x = C.CDLL(ctypes.util.find_library('X11'));xt = C.CDLL(ctypes.util.find_library('Xtst'))
    x.XOpenDisplay.argtypes=[C.c_char_p];x.XOpenDisplay.restype=C.c_void_p
    x.XDefaultRootWindow.argtypes=[C.c_void_p];x.XDefaultRootWindow.restype=C.c_ulong
    x.XKeysymToKeycode.argtypes=[C.c_void_p,C.c_ulong];x.XKeysymToKeycode.restype=C.c_uint
    x.XStringToKeysym.argtypes=[C.c_char_p];x.XStringToKeysym.restype=C.c_ulong
    x.XFlush.argtypes=[C.c_void_p]
    x.XGetImage.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_int,C.c_uint,C.c_uint,C.c_ulong,C.c_int]
    x.XGetImage.restype=C.POINTER(XImage)
    x.XDestroyImage.argtypes=[C.POINTER(XImage)]
    xt.XTestFakeKeyEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
    server_log=(out/'xvfb.log').open('w');game_log=(out/'game.log').open('w')
    server=subprocess.Popen(['Xvfb',display,'-screen','0','1280x720x24','-nolisten','tcp','-ac'],stdout=server_log,stderr=server_log)
    game=None
    try:
        connection=None
        for _ in range(100):
            connection=x.XOpenDisplay(display.encode())
            if connection:break
            time.sleep(.05)
        assert connection,'Xvfb display did not open'
        env={**os.environ,'DISPLAY':display,'LIBGL_ALWAYS_SOFTWARE':'1','BLUEENGINE_DATA_DIR':str(out/'storage')}
        game=subprocess.Popen([str(args.binary.resolve()),'--mute','--size','1280x720'],cwd=ROOT,env=env,stdout=game_log,stderr=game_log)
        time.sleep(2)
        assert game.poll() is None,'Native client exited; see game.log'
        def key(name):
            code=x.XKeysymToKeycode(connection,x.XStringToKeysym(name.encode()))
            assert code,name
            xt.XTestFakeKeyEvent(connection,code,1,0);x.XFlush(connection);time.sleep(.055)
            xt.XTestFakeKeyEvent(connection,code,0,0);x.XFlush(connection);time.sleep(.17)
        def capture(name):
            im=x.XGetImage(connection,x.XDefaultRootWindow(connection),0,0,1280,720,0xffffffff,2)
            assert im
            v=im.contents;assert v.bits_per_pixel==32 and v.byte_order==0
            data=C.string_at(v.data,v.bytes_per_line*v.height)
            rgb=bytearray(v.width*v.height*3)
            for y in range(v.height):
                row=data[y*v.bytes_per_line:y*v.bytes_per_line+v.width*4];start=y*v.width*3
                rgb[start:start+v.width*3:3]=row[2::4];rgb[start+1:start+v.width*3:3]=row[1::4];rgb[start+2:start+v.width*3:3]=row[0::4]
            raw=b''.join(b'\0'+rgb[y*v.width*3:(y+1)*v.width*3] for y in range(v.height))
            def chunk(tag,b):return struct.pack('>I',len(b))+tag+b+struct.pack('>I',zlib.crc32(tag+b)&0xffffffff)
            png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',v.width,v.height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')
            (out/(name+'.png')).write_bytes(png);x.XDestroyImage(im)
        def state():
            key('k')
            return save_payload(out/'storage/blueengine_backstop_v1_quick')
        capture('00-start');key('Return');capture('01-preview')
        key('space');delivered=state();assert delivered['frame']['crates']==[32]
        key('z');assert state()['frame']['crates']==[30];capture('02-undo')
        key('space');key('k');key('z');key('l');time.sleep(.25)
        assert state()['frame']['crates']==[32];capture('03-loaded-delivery')
        key('Return');assert state()['room']==1
        # Deliberately launch into the bottom edge using real direction keys.
        for k in ['w','Right','s','space']:key(k)
        dead=state();assert dead['deadlocked'];capture('04-recoverable-deadlock')
        key('z');assert not state()['deadlocked']
        key('r');reset=state();assert reset['room']==1 and reset['frame']['crates']==[31]
        # Exercise pause and return to play, then finish through actual native keys.
        key('Escape');capture('05-pause');key('Escape')
        routes=re.findall(r'route: "([^"]+)"', (ROOT/'src/rooms.rs').read_text())
        mapping={'>':'Right','<':'a','^':'Up','v':'s','!':'space','z':'z'}
        for room in range(1,8):
            for char in routes[room]:key(mapping[char])
            observed=state();assert observed['room']==room and not observed['deadlocked']
            if room in (2,4,6,7):capture(f'bay-{room+1:02}-delivered')
            print(f'Native keys completed bay {room+1}',flush=True)
            if room<7:key('Return')
        final=state();assert final['room']==7
        # All crate cells must be target cells in the authored last room.
        assert set(final['frame']['crates'])=={30,32,51}
        result={'software_display':True,'physical_devices_verified':False,'keyboard_path':'XTest -> shared native client -> Intent -> fixed-step rules',
                'checks':['start','Space exact shove','Z undo','K/L save/load','Enter advance','WASD and arrows','deadlock recovery','R current-room retry','Esc pause','eight-room completion'],
                'outcome':'won','captures':sorted(p.name for p in out.glob('*.png'))}
        (out/'report.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
    finally:
        if game is not None:game.terminate();game.wait(timeout=10)
        server.terminate();server.wait(timeout=10);game_log.close();server_log.close()
if __name__=='__main__':main()
