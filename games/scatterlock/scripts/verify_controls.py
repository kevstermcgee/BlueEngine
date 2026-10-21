#!/usr/bin/env python3
"""X11 software control review. Run under xvfb-run; sends real native input events.
This is software evidence, never a physical-device attestation.
"""
import ctypes as C
import os
import subprocess
import sys
import time
from pathlib import Path
import struct
import zlib
import argparse
root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary')
parser.add_argument('--out',type=Path,default=root/'evidence'/'current-controls')
parser.add_argument('--opening-only',action='store_true',help='Capture the text-only opening card, then stop the review child.')
args=parser.parse_args()
out=args.out.resolve()
out.mkdir(parents=True,exist_ok=False)
x=C.CDLL('libX11.so.6');xt=C.CDLL('libXtst.so.6')
x.XOpenDisplay.restype=C.c_void_p;d=x.XOpenDisplay(None)
if not d:raise SystemExit('X11 display unavailable; run under xvfb-run')
x.XDefaultRootWindow.argtypes=[C.c_void_p];x.XDefaultRootWindow.restype=C.c_ulong
window=x.XDefaultRootWindow(d)
x.XStringToKeysym.argtypes=[C.c_char_p];x.XStringToKeysym.restype=C.c_ulong
x.XKeysymToKeycode.argtypes=[C.c_void_p,C.c_ulong];x.XKeysymToKeycode.restype=C.c_uint
x.XFlush.argtypes=[C.c_void_p]
xt.XTestFakeKeyEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
xt.XTestFakeButtonEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
xt.XTestFakeMotionEvent.argtypes=[C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_ulong]
def key(name):
    code=x.XKeysymToKeycode(d,x.XStringToKeysym(name.encode()))
    xt.XTestFakeKeyEvent(d,code,1,0);x.XFlush(d);time.sleep(.16)
    xt.XTestFakeKeyEvent(d,code,0,0);x.XFlush(d);time.sleep(.6)
def button(n,down):xt.XTestFakeButtonEvent(d,n,down,0);x.XFlush(d)
def motion(a,b):xt.XTestFakeMotionEvent(d,0,a,b,0);x.XFlush(d);time.sleep(.12)
class XImage(C.Structure):
    _fields_=[('width',C.c_int),('height',C.c_int),('xoffset',C.c_int),('format',C.c_int),('data',C.c_void_p),('byte_order',C.c_int),('bitmap_unit',C.c_int),('bitmap_bit_order',C.c_int),('bitmap_pad',C.c_int),('depth',C.c_int),('bytes_per_line',C.c_int),('bits_per_pixel',C.c_int)]
x.XGetImage.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_int,C.c_uint,C.c_uint,C.c_ulong,C.c_int];x.XGetImage.restype=C.POINTER(XImage)
x.XDestroyImage.argtypes=[C.POINTER(XImage)]
def shot(name,selected=None):
    time.sleep(.3)
    p=x.XGetImage(d,window,0,0,1280,800,0xffffffff,2);im=p.contents
    stride=im.bytes_per_line
    data=C.string_at(im.data,stride*im.height)
    rows=[]
    for y in range(im.height):
        row=data[y*im.bytes_per_line:y*im.bytes_per_line+im.width*4]
        rgb=bytearray(im.width*3)
        rgb[0::3]=row[2::4];rgb[1::3]=row[1::4];rgb[2::3]=row[0::4]
        rows.append(b'\0'+rgb)
    def chunk(kind,payload):
        return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload))
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',im.width,im.height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(rows)))+chunk(b'IEND',b'')
    (out/f'{name}.png').write_bytes(png)
    x.XDestroyImage(p)
    if selected is not None:
        # Read the selected sidebar row's background, away from text and borders.
        # This checks that a real click/key selected B, even if later Tab input could mask a failure.
        for i in range(2):
            offset=(294+i*34)*stride+30*4
            blue,green,red=data[offset:offset+3]
            highlighted=red>20 and green>45 and blue>45
            if highlighted != (i==selected):
                raise AssertionError(f'{name}: beacon row {i} selection incorrect; inspect capture')
exe=args.binary
alsa=out/'alsa-null.conf'
alsa.write_text('pcm.!default { type null }\nctl.!default { type null }\n')
process=subprocess.Popen([exe,'--size','1280x800','--mute','--save-dir',str(out/'saves')],cwd=root,env={**os.environ,'ALSA_CONFIG_PATH':str(alsa)},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
try:
    if args.opening_only:
        time.sleep(.2)
        shot('00-opening')
        raise SystemExit(0)
    time.sleep(2)
    shot('01-start',selected=0)
    key('2');shot('01b-number-beacon-b',selected=1)
    key('1');shot('01c-number-beacon-a',selected=0)
    motion(790,335);button(1,1);time.sleep(.12);button(1,0);time.sleep(.4)
    shot('01d-world-beacon-b',selected=1);key('1')
    key('q');key('space');key('e');key('space');shot('02-exhausted')
    key('z');shot('03-undo');key('r')
    motion(100,335);button(1,1);time.sleep(.12);button(1,0);time.sleep(.4);shot('04-click-beacon',selected=1)
    key('Tab');key('space');key('Tab');key('q');key('space');shot('05-secured')
    key('z');key('F5');key('r');key('F9');key('space');shot('06-save-resumed')
    motion(800,400);button(3,1);motion(845,420);button(3,0);button(4,1);button(4,0);time.sleep(.4);shot('07-orbit-zoom')
    key('Escape');shot('08-paused');key('Escape')
    # Window close is the native client's standard quit request.
    # Exit via the game menu: Escape, Down to Quit (second item), Enter.
    key('Escape');key('Down');key('Down');key('Return')
    try:log=process.communicate(timeout=5)[0]
    except subprocess.TimeoutExpired:
        process.terminate();log=process.communicate(timeout=5)[0]
    (out/'native-input.log').write_text(log)
    print(log)
    if 'remaining=0 docked=2' not in log:raise SystemExit('Real-input completion not confirmed; inspect captures/log')
finally:
    if process.poll() is None:process.terminate();process.wait()
