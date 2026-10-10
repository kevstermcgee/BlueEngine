#!/usr/bin/env python3
"""Linux QA: inject actual X11 keyboard/mouse events, then inspect engine-owned saves."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import struct
import subprocess
import time

def payload(path):
    data=path.read_bytes()
    header_len=struct.unpack_from('<I',data,12)[0]
    payload_len=struct.unpack_from('<Q',data,16)[0]
    return json.loads(data[24+header_len:24+header_len+payload_len])

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',required=True)
    p.add_argument('--output',required=True)
    args=p.parse_args()
    out=Path(args.output).resolve();out.mkdir(parents=True,exist_ok=False)
    display=':'+str(100+os.getpid()%5000)
    server=subprocess.Popen(['Xvfb',display,'-screen','0','1280x720x24','-nolisten','tcp'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    game=None
    x=ctypes.CDLL('libX11.so.6');xt=ctypes.CDLL('libXtst.so.6')
    x.XOpenDisplay.argtypes=[ctypes.c_char_p];x.XOpenDisplay.restype=ctypes.c_void_p
    x.XStringToKeysym.argtypes=[ctypes.c_char_p];x.XStringToKeysym.restype=ctypes.c_ulong
    x.XKeysymToKeycode.argtypes=[ctypes.c_void_p,ctypes.c_ulong];x.XKeysymToKeycode.restype=ctypes.c_uint
    x.XFlush.argtypes=[ctypes.c_void_p]
    xt.XTestFakeKeyEvent.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_int,ctypes.c_ulong]
    xt.XTestFakeButtonEvent.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_int,ctypes.c_ulong]
    (out/'asound.conf').write_text('pcm.!default { type null }\nctl.!default { type null }\n')
    d=None
    try:
        for _ in range(40):
            d=x.XOpenDisplay(display.encode())
            if d:break
            time.sleep(0.05)
        if not d:raise RuntimeError('Xvfb did not open')
        def key(name,down):
            code=x.XKeysymToKeycode(d,x.XStringToKeysym(name.encode()));xt.XTestFakeKeyEvent(d,code,int(down),0);x.XFlush(d)
        def tap(name):
            key(name,True);time.sleep(0.12);key(name,False);time.sleep(0.12)
        with (out/'game.log').open('w') as log:
            game=subprocess.Popen([str(Path(args.binary).resolve()),'--capture',str(out/'frames'),'--frames','30,80','--exit-after','100','--size','1280x720','--mute','--save-dir',str(out/'saves')],env={**os.environ,'DISPLAY':display,'LIBGL_ALWAYS_SOFTWARE':'1','ALSA_CONFIG_PATH':str(out/'asound.conf')},stdout=log,stderr=log)
            time.sleep(2)
            tap('e');time.sleep(0.5);tap('F5');time.sleep(0.25)
            key('w',True);time.sleep(1.2);tap('Shift_L');time.sleep(0.4);key('w',False)
            xt.XTestFakeButtonEvent(d,3,1,0);x.XFlush(d);time.sleep(0.15);xt.XTestFakeButtonEvent(d,3,0,0)
            xt.XTestFakeButtonEvent(d,1,1,0);x.XFlush(d);time.sleep(0.7);xt.XTestFakeButtonEvent(d,1,0,0);x.XFlush(d)
            tap('F5')
            game.wait(timeout=90)
        if game.returncode:raise RuntimeError((out/'game.log').read_text()[-2000:])
        first=payload(out/'saves/quick.be2save.bak');last=payload(out/'saves/quick.be2save')
        # Snapshot payload wraps the game's state to preserve f32 bits; unwrap its state.
        if 'state' in first:first=first['state'];last=last['state']
        assert first['phase']=='Combat' and last['phase']=='Combat', 'E must reach the real game'
        p1=first['player']['p'];p2=last['player']['p']
        assert sum((a-b)**2 for a,b in zip(p1,p2))>0.25, 'W must actually move the controller'
        assert last['dash_cd']>0., 'Actual Shift must trigger dash'
        assert last['heat']>0., 'Actual left mouse button must fire'
        assert last['pulse_cd']>0., 'Actual right mouse button must trigger the pulse'
        print(json.dumps({'real_keyboard_mouse':'passed','position_before':p1,'position_after':p2,'heat':last['heat'],'pulse_cooldown':last['pulse_cd'],'captures':str(out/'frames')}))
    finally:
        if game and game.poll() is None:game.terminate();game.wait(timeout=10)
        server.terminate();server.wait(timeout=10)
if __name__=='__main__':main()
