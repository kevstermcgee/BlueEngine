#!/usr/bin/env python3
"""Linux X11 controls/capture check; real XTest events, isolated engine Snapshot storage.

python3 scripts/native_controls.py PATH_TO_BINARY --out .blue-check/native-controls
Requires existing Xvfb, libX11 and libXtst; no installation or desktop changes.
"""
import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time
import zlib


def read_save(path):
    data = path.read_bytes()
    assert data[:8] == b'BE2SAVE\x1a'
    assert hashlib.sha256(data[:-32]).digest() == data[-32:]
    header_size = struct.unpack_from('<I', data, 12)[0]
    payload_size = struct.unpack_from('<Q', data, 16)[0]
    return json.loads(data[24+header_size:24+header_size+payload_size])['state']


class Image(C.Structure):
    _fields_ = [(name, C.c_int) for name in ('width', 'height', 'xoffset', 'format')] + [('data', C.c_void_p)] + [(name, C.c_int) for name in ('byte_order', 'bitmap_unit', 'bitmap_bit_order', 'bitmap_pad', 'depth', 'bytes_per_line', 'bits_per_pixel')] + [(name, C.c_ulong) for name in ('red_mask', 'green_mask', 'blue_mask')]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    x = C.CDLL('libX11.so.6')
    xt = C.CDLL('libXtst.so.6')
    x.XOpenDisplay.argtypes = [C.c_char_p]; x.XOpenDisplay.restype = C.c_void_p
    x.XDefaultRootWindow.argtypes = [C.c_void_p]; x.XDefaultRootWindow.restype = C.c_ulong
    x.XFlush.argtypes = [C.c_void_p]
    x.XKeysymToKeycode.argtypes = [C.c_void_p, C.c_ulong]; x.XKeysymToKeycode.restype = C.c_uint
    x.XGetImage.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint, C.c_uint, C.c_ulong, C.c_int]; x.XGetImage.restype = C.POINTER(Image)
    x.XDestroyImage.argtypes = [C.POINTER(Image)]
    xt.XTestFakeMotionEvent.argtypes = [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]
    xt.XTestFakeButtonEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
    xt.XTestFakeKeyEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
    with tempfile.TemporaryDirectory(prefix='gap-flip-input-') as temporary:
        temp = Path(temporary)
        # Xvfb chooses a free display atomically; local test events never target the user's desktop.
        with (temp/'display').open('w+') as display_file, (out/'native.log').open('w') as log:
            xvfb = subprocess.Popen(['Xvfb', '-displayfd', str(display_file.fileno()), '-screen', '0', '800x450x24', '-nolisten', 'tcp'], pass_fds=[display_file.fileno()], stdout=log, stderr=log)
            game = None
            try:
                for _ in range(100):
                    display_file.seek(0); number = display_file.read().strip()
                    if number: break
                    if (out/'native.log').stat().st_size > 65536 or xvfb.poll() is not None:
                        raise RuntimeError('Xvfb cannot open a local display; no native input checks ran. See native.log.')
                    time.sleep(.05)
                assert number, 'Xvfb did not select a display'
                display_name = ':'+number
                d = x.XOpenDisplay(display_name.encode()); assert d
                env = dict(os.environ, DISPLAY=display_name, LIBGL_ALWAYS_SOFTWARE='1', BLUEENGINE_DATA_DIR=str(temp/'storage'))
                game = subprocess.Popen([str(binary), '--mute', '--capture', str(temp/'unused-captures'), '--frames', '999999', '--exit-after', '1000000', '--size', '800x450'], env=env, stdout=log, stderr=log)
                time.sleep(2)
                assert game.poll() is None, 'native client exited during startup'
                results = []
                def wait(): time.sleep(.22)
                def click(px, py):
                    xt.XTestFakeMotionEvent(d, 0, px, py, 0); x.XFlush(d); time.sleep(.07)
                    xt.XTestFakeButtonEvent(d, 1, 1, 0); x.XFlush(d); time.sleep(.07)
                    xt.XTestFakeButtonEvent(d, 1, 0, 0); x.XFlush(d); wait()
                def key(keysym):
                    code = x.XKeysymToKeycode(d, keysym)
                    xt.XTestFakeKeyEvent(d, code, 1, 0); x.XFlush(d); time.sleep(.07)
                    xt.XTestFakeKeyEvent(d, code, 0, 0); x.XFlush(d); wait()
                def state():
                    key(ord('k'))
                    saved = temp/'storage'/'blueengine_gap-flip_v1_quick'
                    for _ in range(30):
                        if saved.exists(): return read_save(saved)
                        time.sleep(.05)
                    raise AssertionError('K did not produce a Snapshot')
                def shot(name):
                    image = x.XGetImage(d, x.XDefaultRootWindow(d), 0, 0, 800, 450, C.c_ulong(-1).value, 2)
                    assert image
                    im = image.contents; assert im.bits_per_pixel == 32 and im.byte_order == 0
                    raw = C.string_at(im.data, im.bytes_per_line*im.height)
                    rows = bytearray()
                    for y in range(im.height):
                        rows.append(0)
                        for px in range(im.width):
                            b, g, r, _ = raw[y*im.bytes_per_line+px*4:y*im.bytes_per_line+px*4+4]
                            rows.extend((r,g,b))
                    def chunk(kind, data): return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
                    png = b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',im.width,im.height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b'')
                    (out/(name+'.png')).write_bytes(png); x.XDestroyImage(image)
                def checked(name, predicate):
                    s = state(); assert predicate(s), (name,s)
                    results.append({'check': name, 'passed': True, 'room': s['room']+1, 'tokens': s['tokens'], 'undo_depth': len(s['history'])})
                    return s
                shot('01-room-one')
                click(130,226)  # A in room 1 (one gap left, four right)
                checked('mouse selects A',lambda s:s['selected']==0)
                click(674,241)
                checked('equal vertical gaps are disabled',lambda s:s['axis'] is None and s['tokens']==[7])
                click(490,241)
                checked('axis click previews without moving',lambda s:s['axis']=='Horizontal' and s['tokens']==[7])
                shot('02-gap-preview')
                click(274,226)
                checked('ghost click commits reflected landing',lambda s:s['tokens']==[10] and len(s['history'])==1)
                shot('03-room-matched')
                click(460,293)
                checked('undo reverses completion',lambda s:s['tokens']==[7] and not s['history'])
                click(490,241); click(274,226)
                key(ord('r'))
                checked('R restarts current room',lambda s:s['room']==0 and s['tokens']==[7] and not s['history'])
                click(130,226); click(490,241)
                key(ord('k'))  # leave the exact pending preview saved
                click(274,226); key(ord('l'))
                checked('L restores saved preview and undo history',lambda s:s['tokens']==[7] and s['axis']=='Horizontal' and not s['history'])
                key(0xff1b); shot('04-paused')
                click(274,226)
                checked('pause blocks gameplay clicks',lambda s:s['tokens']==[7] and s['axis']=='Horizontal')
                key(0xff1b); click(274,226); click(582,350)
                checked('Next advances to room 2',lambda s:s['room']==1 and s['tokens']==[5])
                key(ord('r'))
                checked('R preserves room progress',lambda s:s['room']==1 and s['tokens']==[5])
                # Real mouse solution to the wall-turn room.
                for token, axis, dest in [(5,0,6),(6,1,16),(16,0,18),(18,1,8)]:
                    def point(cell): return (82+(cell%5)*48+24,106+(cell//5)*48+24)
                    click(*point(token));click(490 if axis==0 else 674,241);click(*point(dest))
                checked('both axes complete wall-turn room',lambda s:s['tokens']==[8] and len(s['history'])==4)
                shot('05-wall-room-matched'); click(582,350)
                shot('06-helper-room')
                # Boundary-changing A / B room through mouse input, then unlimited multi-undo.
                for token, axis, dest in [(0,0,1),(2,0,5),(1,1,7),(7,0,10),(5,0,0)]:
                    def point(cell): return (58+(cell%6)*48+24,154+(cell//6)*48+24)
                    click(*point(token));click(490 if axis==0 else 674,241);click(*point(dest))
                checked('helper token changes real landing',lambda s:s['tokens']==[10,0] and len(s['history'])==5)
                shot('07-helper-room-matched')
                for _ in range(5): click(460,293)
                checked('five consecutive undos recover room entrance',lambda s:s['tokens']==[0,2] and not s['history'])
                shot('08-helper-room-undone')
                (out/'report.json').write_text(json.dumps({'input':'XTest mouse/keyboard through shared native client', 'hardware_playtest':False, 'checks':results},indent=2)+'\n')
                print(json.dumps({'passed':len(results),'captures':str(out)}))
            except Exception as error:
                (out/'report.json').write_text(json.dumps({'passed': False, 'error': str(error), 'native_checks_completed': False},indent=2)+'\n')
                raise
            finally:
                if game is not None:
                    game.terminate()
                    try: game.wait(timeout=5)
                    except subprocess.TimeoutExpired: game.kill(); game.wait()
                xvfb.terminate(); xvfb.wait(timeout=5)


if __name__ == '__main__':
    main()
