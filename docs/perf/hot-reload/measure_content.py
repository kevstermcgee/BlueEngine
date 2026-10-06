#!/usr/bin/env python3
"""Linux/X11 real-frame reload measurement; no builds, X11 root pixels prove the update."""
import argparse
import ctypes as C
import ctypes.util
import hashlib
import json
import os
from pathlib import Path
import signal
import struct
import subprocess
import time
import zlib

ENGINE = Path(__file__).resolve().parents[3]
class XImage(C.Structure):
    _fields_ = [(n,C.c_int) for n in ['width','height','xoffset','format']] + [('data',C.c_void_p)] + [(n,C.c_int) for n in ['byte_order','bitmap_unit','bitmap_bit_order','bitmap_pad','depth','bytes_per_line','bits_per_pixel']] + [(n,C.c_ulong) for n in ['red_mask','green_mask','blue_mask']]
class Display:
    def __init__(self, name):
        self.lib=C.CDLL(ctypes.util.find_library('X11'))
        self.lib.XOpenDisplay.argtypes=[C.c_char_p];self.lib.XOpenDisplay.restype=C.c_void_p
        self.d=self.lib.XOpenDisplay(name.encode());assert self.d
        self.lib.XDefaultRootWindow.argtypes=[C.c_void_p];self.lib.XDefaultRootWindow.restype=C.c_ulong
        self.root=self.lib.XDefaultRootWindow(self.d)
        self.lib.XGetImage.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_int,C.c_uint,C.c_uint,C.c_ulong,C.c_int];self.lib.XGetImage.restype=C.POINTER(XImage)
        self.lib.XDestroyImage.argtypes=[C.POINTER(XImage)]
        self.lib.XCloseDisplay.argtypes=[C.c_void_p]
    def sample(self,x=25,y=25):
        p=self.lib.XGetImage(self.d,self.root,x,y,1,1,0xFFFFFFFF,2)
        assert p
        i=p.contents;b=C.string_at(i.data,i.bytes_per_line);v=int.from_bytes(b[:4],'little' if i.byte_order==0 else 'big');rgb=tuple((v & m)>>((m & -m).bit_length()-1) for m in [i.red_mask,i.green_mask,i.blue_mask]);self.lib.XDestroyImage(p);return rgb
    def capture(self,out):
        p=self.lib.XGetImage(self.d,self.root,0,0,1280,720,0xFFFFFFFF,2);assert p
        i=p.contents;assert i.bits_per_pixel==32 and i.byte_order==0
        raw=C.string_at(i.data,i.bytes_per_line*i.height);data=bytearray()
        for y in range(i.height):
            data.append(0)
            row=raw[y*i.bytes_per_line:y*i.bytes_per_line+i.width*4]
            for x in range(0,len(row),4):data.extend((row[x+2],row[x+1],row[x]))
        self.lib.XDestroyImage(p)
        def chunk(n,b):return struct.pack('>I',len(b))+n+b+struct.pack('>I',zlib.crc32(n+b)&0xFFFFFFFF)
        out.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1280,720,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(data))+chunk(b'IEND',b''))
    def close(self):self.lib.XCloseDisplay(self.d)
def wait_color(display,index,deadline):
    while time.perf_counter()<deadline:
        rgb=display.sample()
        if rgb[index]>160 and all(rgb[index]>rgb[j]*2 for j in range(3) if j!=index):return rgb
        time.sleep(.01)
    raise RuntimeError(f'No visible color {index}; final pixel {display.sample()}')
def edit(path,index):
    v=json.loads(path.read_text());p=v.setdefault('presentation',{});p['objective']='Visible content edit '+str(index);p['hud']={'margin':20,'width':360,'scale':1};p.setdefault('palette',{})['panel']=[[.85,.1,.12,1],[.1,.75,.15,1],[.12,.2,.85,1]][index]
    path.write_text(json.dumps(v));return hashlib.sha256(path.read_bytes()).hexdigest()
def stop(p):
    if p.poll() is None:
        p.send_signal(signal.SIGTERM)
        try:p.wait(timeout=5)
        except subprocess.TimeoutExpired:p.kill();p.wait()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True,help='compiled stopped prototype; see ADR 0042')
    parser.add_argument('--output',type=Path,default=ENGINE/'.be2-work/hot-reload')
    args=parser.parse_args();BINARY=args.binary.resolve();TOOLS=BINARY.with_name('be2-tools');ROOT=args.output.resolve();ROOT.mkdir(parents=True,exist_ok=True)
    run=ROOT/('frames-'+str(time.time_ns()));run.mkdir()
    game=run/'game';r=subprocess.run([str(TOOLS),'game-example',str(game)],capture_output=True,text=True);assert r.returncode==0,r.stderr
    display_name=':173';xvfb=subprocess.Popen(['Xvfb',display_name,'-screen','0','1280x720x24','-nolisten','tcp'],stdout=subprocess.DEVNULL,stderr=(run/'xvfb.log').open('w'))
    env={**os.environ,'DISPLAY':display_name,'LIBGL_ALWAYS_SOFTWARE':'1'};display=None;process=None;results=[]
    try:
        for _ in range(100):
            if Path('/tmp/.X11-unix/X173').exists():break
            time.sleep(.02)
        display=Display(display_name)
        for index in range(3):
            t=time.perf_counter();identity=edit(game/'game.json',index);argv=[str(BINARY),'--game',str(game/'game.json'),'--mute','--settings',str(run/'settings.json')]
            with (run/f'before-{index}.log').open('w') as log:
                process=subprocess.Popen(argv,env=env,stdout=log,stderr=log)
                rgb=wait_color(display,index,t+15);elapsed=time.perf_counter()-t;display.capture(run/f'before-{index}.png');stop(process)
            row={'route':'restart','trial':index+1,'edit_to_visible_seconds':round(elapsed,4),'argv':argv,'pixel':rgb,'game_sha256':identity};results.append(row);print(json.dumps(row),flush=True)
            time.sleep(.15)
        # Establish a distinct initial color and wait for the actual watcher-ready event.
        edit(game/'game.json',2)
        argv=[str(BINARY),'--game',str(game/'game.json'),'--watch','--mute','--settings',str(run/'settings.json')]
        log_path=run/'after.log'
        with log_path.open('w') as log:
            process=subprocess.Popen(argv,env=env,stdout=log,stderr=log);deadline=time.perf_counter()+15
            while '"status":"ready"' not in log_path.read_text():
                assert time.perf_counter()<deadline and process.poll() is None
                time.sleep(.01)
            wait_color(display,2,deadline)
            for index in range(3):
                t=time.perf_counter();identity=edit(game/'game.json',index);rgb=wait_color(display,index,t+15);elapsed=time.perf_counter()-t;display.capture(run/f'after-{index}.png')
                row={'route':'watch','trial':index+1,'edit_to_visible_seconds':round(elapsed,4),'argv':argv,'pixel':rgb,'game_sha256':identity};results.append(row);print(json.dumps(row),flush=True)
            good=(game/'game.json').read_bytes();v=json.loads(good);v['schema_version']=999;(game/'game.json').write_text(json.dumps(v));deadline=time.perf_counter()+5
            while '"status":"failed"' not in log_path.read_text():
                assert time.perf_counter()<deadline and process.poll() is None
                time.sleep(.01)
            assert display.sample()[2]>160;display.capture(run/'invalid-keeps-old.png')
            (game/'game.json').write_bytes(good);time.sleep(.5);stop(process)
        events=[]
        for line in log_path.read_text().splitlines():
            try:v=json.loads(line)
            except json.JSONDecodeError:continue
            if v.get('event')=='content_reload':events.append(v)
        report={'version':1,'engine_binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),'builds_triggered':0,'environment':'Linux Xvfb/software OpenGL; warm compiled executable; fresh restart process versus retained watch process','results':results,'events':events,'invalid_kept_old_content':True,'captures_visually_inspected':False}
        (run/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(str(run/'result.json'),flush=True)
    finally:
        if process:stop(process)
        if display:display.close()
        stop(xvfb)
if __name__=='__main__':main()
