#!/usr/bin/env python3
"""Linux development evidence: send real X11 keys/mouse to the native window, never script gameplay.
Run inside xvfb-run: python3 scripts/test_native_controls.py GAME_BINARY NEW_OUTPUT_DIR.
Requires the existing X11/XTest libraries and FFmpeg. Does not install anything or touch shortcuts.
"""
import ctypes as C
import ctypes.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

binary, output = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
output.mkdir(parents=True, exist_ok=False)
x = C.CDLL(ctypes.util.find_library('X11'))
test = C.CDLL(ctypes.util.find_library('Xtst'))
display_t, window_t = C.c_void_p, C.c_ulong
x.XOpenDisplay.argtypes = [C.c_char_p]; x.XOpenDisplay.restype = display_t
x.XDefaultRootWindow.argtypes = [display_t]; x.XDefaultRootWindow.restype = window_t
x.XStringToKeysym.argtypes = [C.c_char_p]; x.XStringToKeysym.restype = C.c_ulong
x.XKeysymToKeycode.argtypes = [display_t, C.c_ulong]; x.XKeysymToKeycode.restype = C.c_ubyte
x.XFlush.argtypes = [display_t]
x.XQueryTree.argtypes = [display_t,window_t,C.POINTER(window_t),C.POINTER(window_t),C.POINTER(C.POINTER(window_t)),C.POINTER(C.c_uint)]
x.XFetchName.argtypes = [display_t,window_t,C.POINTER(C.c_char_p)]
x.XFree.argtypes = [C.c_void_p]
x.XSetInputFocus.argtypes = [display_t,window_t,C.c_int,C.c_ulong]
test.XTestFakeKeyEvent.argtypes = [display_t,C.c_uint,C.c_int,C.c_ulong]
test.XTestFakeButtonEvent.argtypes = [display_t,C.c_uint,C.c_int,C.c_ulong]
test.XTestFakeMotionEvent.argtypes = [display_t,C.c_int,C.c_int,C.c_int,C.c_ulong]
d = x.XOpenDisplay(None)
if not d:
    raise RuntimeError('Run this test inside xvfb-run; no display is open.')

def windows(parent):
    root, parent_out, children, count = window_t(), window_t(), C.POINTER(window_t)(), C.c_uint()
    if not x.XQueryTree(d,parent,C.byref(root),C.byref(parent_out),C.byref(children),C.byref(count)):
        return
    items = [children[i] for i in range(count.value)]
    if children: x.XFree(children)
    for w in items:
        name=C.c_char_p()
        if x.XFetchName(d,w,C.byref(name)) and name.value:
            title=name.value.decode(errors='replace'); x.XFree(name)
            if title == 'Odd Matter': yield w
        yield from windows(w)

def held(key, down):
    code=x.XKeysymToKeycode(d,x.XStringToKeysym(key.encode()))
    if not code: raise RuntimeError(f'Unknown key {key}')
    test.XTestFakeKeyEvent(d,code,int(down),0); x.XFlush(d); time.sleep(0.17)

def press(key):
    held(key,True); held(key,False)

def shot(name):
    time.sleep(0.3)
    subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','x11grab','-video_size','1280x800','-i',os.environ['DISPLAY'],'-frames:v','1','-threads','1',str(output/f'{name}.png')],check=True)

def move(px,py):
    test.XTestFakeMotionEvent(d,-1,px,py,0); x.XFlush(d); time.sleep(0.2)

def button(n,down):
    test.XTestFakeButtonEvent(d,n,int(down),0); x.XFlush(d); time.sleep(0.17)

with (output/'native.log').open('w') as log:
    process = subprocess.Popen([str(binary),'--capture',str(output/'lifecycle'),'--frames','1,90000','--exit-after','90020','--mute','--seed','7','--size','1280x800','--save-dir',str(output/'saves')],stdout=log,stderr=subprocess.STDOUT)
    try:
        window=None
        for _ in range(150):
            if process.poll() is not None: raise RuntimeError('Native game stopped during startup')
            window=next(windows(x.XDefaultRootWindow(d)),None)
            if window: break
            time.sleep(0.1)
        if not window: raise RuntimeError('Odd Matter window did not appear')
        # A titled X11 window can exist before it is mapped. The first native frame
        # is the readiness signal; focusing earlier produces X11 BadMatch.
        for _ in range(150):
            if (output/'lifecycle/shot_00001.png').is_file(): break
            if process.poll() is not None: raise RuntimeError('Native game stopped before its first frame')
            time.sleep(0.1)
        else: raise RuntimeError('Native first-frame capture did not arrive')
        x.XSetInputFocus(d,window,1,0); x.XFlush(d); time.sleep(0.3)
        shot('01-instructions')
        for key in ('2','e','1','d','d'): press(key)
        held('c',True); shot('02-inside-cavity'); held('c',False)
        press('2'); held('Shift_L',True); press('e'); shot('03-crush-forecast'); held('Shift_L',False)
        press('e'); shot('04-crushed'); press('z')
        press('F5'); press('r'); press('F9')
        for key in ('1','d','d','e','e'): press(key)
        move(820,330); button(3,True); move(730,355); move(655,392); button(3,False)
        button(4,True); button(4,False); button(4,True); button(4,False)
        held('c',True); shot('05-exit-and-orbit'); held('c',False)
        press('Escape'); shot('06-shared-pause'); press('d'); press('Escape')
        press('Return'); shot('07-depth-vault'); press('r')
        press('Escape')
        for _ in range(3): press('Down')
        press('Return')
        process.wait(timeout=20)
        if process.returncode: raise RuntimeError(f'Native exit {process.returncode}')
    finally:
        if process.poll() is None:
            process.terminate(); process.wait(timeout=10)
lines=(output/'native.log').read_text().splitlines()
report=next(json.loads(line) for line in reversed(lines) if line.startswith('{"') and '"game":"odd-matter"' in line)
assert report['room']==2 and report['status']=='Playing' and report['drone']==[2,1,4], report
assert report['native_commands']==16 and report['cut_frames']>0 and report['preview_frames']>0, report
assert report['saves']==1 and report['loads']==1 and report['orbit_pixels']>20 and report['zoom_steps']>=2, report
assert report['cleared_vaults']==1 and report['crushes']==1 and report['undos']==1, report
(output/'controls.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'ok':True,'evidence':str(output),'input':'real X11 keyboard and mouse, no --script or --verify-route','result':report}))
