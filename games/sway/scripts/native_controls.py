"""Exercise real window keys and Windows single/multiple file-drop messages.

Linux development verifies the same importer through CLI files and XTest keys;
the published Windows app additionally verifies native Unicode path-only drops.
"""
import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import wave

def fixture(path):
    import math, struct
    with wave.open(str(path), 'wb') as out:
        out.setparams((2, 2, 16000, 0, 'NONE', 'not compressed'))
        out.writeframes(b''.join(struct.pack('<hh', int(6000*math.sin(math.tau*220*i/16000)),
                                             int(4000*math.sin(math.tau*330*i/16000))) for i in range(16000*24)))

class Windows:
    def __init__(self, pid):
        self.u=c.WinDLL('user32',use_last_error=True)
        self.k=c.WinDLL('kernel32',use_last_error=True)
        self.u.GetWindowThreadProcessId.argtypes=[c.c_void_p,c.POINTER(c.c_ulong)]
        self.u.SendMessageW.argtypes=[c.c_void_p,c.c_uint,c.c_size_t,c.c_ssize_t]
        self.u.SendMessageW.restype=c.c_ssize_t
        self.u.GetWindowTextW.argtypes=[c.c_void_p,c.c_wchar_p,c.c_int]
        callback=c.WINFUNCTYPE(c.c_int,c.c_void_p,c.c_ssize_t)
        self.u.EnumWindows.argtypes=[callback,c.c_ssize_t]
        found=[]
        def visit(hwnd, _):
            owner=c.c_ulong();self.u.GetWindowThreadProcessId(hwnd,c.byref(owner))
            title=c.create_unicode_buffer(256);self.u.GetWindowTextW(hwnd,title,256)
            if owner.value==pid and title.value=='Sway': found.append(hwnd)
            return 1
        self.u.EnumWindows(callback(visit),0)
        if not found: raise RuntimeError('Sway native window missing')
        self.hwnd=found[0]
        self.k.GlobalAlloc.argtypes=[c.c_uint,c.c_size_t];self.k.GlobalAlloc.restype=c.c_void_p
        self.k.GlobalLock.argtypes=[c.c_void_p];self.k.GlobalLock.restype=c.c_void_p
        self.k.GlobalUnlock.argtypes=[c.c_void_p]
    def key(self,name):
        code={'Space':0x20,'Right':0x27}[name]
        self.u.SendMessageW(self.hwnd,0x100,code,0)
        self.u.SendMessageW(self.hwnd,0x101,code,0)
    def drop(self, paths):
        import struct
        # Real DROPFILES layout and double-NUL-terminated UTF-16 paths. Ownership
        # transfers to the receiving window, which calls DragFinish.
        text=('\0'.join(str(p.resolve()) for p in paths)+'\0\0').encode('utf-16-le')
        data=struct.pack('<IiiII',20,0,0,0,1)+text
        handle=self.k.GlobalAlloc(0x42,len(data))
        if not handle: raise c.WinError(c.get_last_error())
        pointer=self.k.GlobalLock(handle)
        if not pointer: raise c.WinError(c.get_last_error())
        c.memmove(pointer,data,len(data));self.k.GlobalUnlock(handle)
        self.u.SendMessageW(self.hwnd,0x233,handle,0)

class Linux:
    def __init__(self,pid):
        self.x=c.CDLL('libX11.so.6'); self.xt=c.CDLL('libXtst.so.6')
        self.x.XOpenDisplay.argtypes=[c.c_char_p];self.x.XOpenDisplay.restype=c.c_void_p
        self.x.XStringToKeysym.argtypes=[c.c_char_p];self.x.XStringToKeysym.restype=c.c_ulong
        self.x.XKeysymToKeycode.argtypes=[c.c_void_p,c.c_ulong];self.x.XKeysymToKeycode.restype=c.c_uint
        self.x.XFlush.argtypes=[c.c_void_p]
        self.xt.XTestFakeKeyEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
        self.display=self.x.XOpenDisplay(os.environ['DISPLAY'].encode())
    def key(self,name):
        name={'Space':'space'}.get(name,name).encode()
        code=self.x.XKeysymToKeycode(self.display,self.x.XStringToKeysym(name))
        self.xt.XTestFakeKeyEvent(self.display,code,1,0);self.xt.XTestFakeKeyEvent(self.display,code,0,0)
        self.x.XFlush(self.display)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe',required=True)
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    qa=root/'.blue-check'/('controls-'+str(time.time_ns()));qa.mkdir(parents=True)
    first=qa/'Café first.wav';second=qa/'Second song.wav';fixture(first);fixture(second)
    cli=[str(Path(args.exe).resolve()),'--capture',str(qa/'frames'),'--frames','1,60,120,180,240,300,360',
         '--exit-after','361','--size','960x540','--mute','--seed','7']
    windows=sys.platform=='win32'
    if not windows: cli+=['--file',str(first),'--file',str(second),'--file',str(first)]
    game=subprocess.Popen(cli,cwd=root,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,encoding='utf-8',errors='replace')
    records={};controller=None
    try:
        for line in game.stdout:
            if not line.startswith('{'): continue
            row=json.loads(line);frame=row.get('frame')
            if frame is None: continue
            records[frame]=row
            if frame==1:
                controller=Windows(game.pid) if windows else Linux(game.pid)
                if windows:controller.drop([first])
            if frame==60:
                assert row['listens']>=1, row
                if windows:
                    assert row['playlist']==1,row
                    controller.drop([second,first])
                controller.key('Space')
            if frame==180:controller.key('Space')
            if frame==240:controller.key('Right')
        game.wait(timeout=5)
        if game.returncode:raise RuntimeError('Native app failed with '+str(game.returncode))
        a,b=records[120],records[180]
        assert a['paused'] and b['paused'], (a,b)
        for key in ('animation_time','audio_position','title_time','state_hash'):
            assert a[key]==b[key], (key,a[key],b[key])
        assert Path(a['path']).read_bytes()==Path(b['path']).read_bytes(),'Paused picture changed'
        assert not records[240]['paused']
        assert records[240]['animation_time']>b['animation_time']
        assert records[240]['audio_position']>b['audio_position']
        assert records[300]['listens']>records[240]['listens']
        assert records[300]['song']=='Second song'
        assert records[120]['playlist']==3
        report={'ok':True,'platform':sys.platform,'native_windows_single_and_multi_drop':windows,
                'unicode_path_import':True,'space_pauses_pcm_position_and_animation':True,
                'paused_picture_identical':True,'space_resumes_both':True,'next_track_resets_title':True,
                'records':records}
        (qa/'controls.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k!='records'}))
    finally:
        if game.poll() is None:game.terminate();game.wait(timeout=5)
if __name__=='__main__':main()
