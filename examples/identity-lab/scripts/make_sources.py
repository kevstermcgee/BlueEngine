#!/usr/bin/env python3
"""Reproduce this example's synthetic source clips and editable audio projects; render banks separately."""
import json,math,random,wave,struct
from pathlib import Path
root=Path(__file__).resolve().parents[1]/'assets/audio-source';root.mkdir(parents=True,exist_ok=True)
def clip(name,seconds,mode):
 rng=random.Random(119+len(name));data=[];low=0.
 for i in range(round(44100*seconds)):
  t=i/44100;noise=rng.uniform(-1,1);low+=.18*(noise-low)
  env=min(1,t/.008)*min(1,(seconds-t)/.025)
  if mode=='paper':v=(noise-low)*.19*env*(.65+.35*math.sin(t*70))
  elif mode=='relay':v=(.3*math.sin(2*math.pi*185*t)+.16*low)*math.exp(-t*28)*env
  elif mode=='air':v=low*.06*min(1,t/.1,(seconds-t)/.1)
  else:v=.22*math.sin(2*math.pi*(150+400*t)*t)*math.exp(-t*12)*env
  data.append(round(v*32767))
 with wave.open(str(root/name),'wb') as f:f.setnchannels(1);f.setsampwidth(2);f.setframerate(44100);f.writeframes(struct.pack('<'+'h'*len(data),*data))
clip('pencil.wav',.13,'paper');clip('page.wav',.35,'paper');clip('relay.wav',.18,'relay');clip('latch.wav',.30,'relay');clip('room-air.wav',4.1,'air')
def imported(file,gain=.65):return {'gain':gain,'source':{'kind':'wav','file':file}}
def score(notes,wave='sine',bpm=120,beats=2):
 return {'kind':'score','score':{'bpm':bpm,'beats':beats,'layers':[{'name':'voice','gain':.32,'instrument':{'wave':wave,'attack':.006,'decay':.05,'sustain':.2,'release':.12},'notes':[{'at':at,'beats':duration,'midi':midi,'pan':pan} for at,duration,midi,pan in notes]}]}}
notebook={'version':1,'seed':310,'effects':{'tick':imported('pencil.wav',.5),'gesture':imported('pencil.wav'),'open':imported('page.wav',.5),'reject':{'gain':.4,'source':score([(0,.3,50,0)],'triangle')},'resolve':{'gain':.5,'source':score([(0,.4,62,-.2),(.5,.8,69,.2)],'triangle')}},'music':{'kind':'clips','seconds':4,'crossfade_seconds':.04,'layers':{'air':{'file':'room-air.wav','gain':.5}}}}
instrument={'version':1,'seed':761,'effects':{'tick':imported('relay.wav',.6),'gesture':imported('latch.wav'),'open':imported('latch.wav',.6),'reject':{'gain':.4,'source':score([(0,.15,42,0),(.3,.15,42,0)],'square')},'resolve':{'gain':.5,'source':score([(0,.4,48,0),(.4,.4,60,0)],'triangle')}},'music':None}
arcade={'version':1,'seed':932,'effects':{'tick':{'gain':.45,'source':score([(0,.1,84,0)],'square')},'gesture':{'gain':.5,'source':score([(0,.12,72,-.3),(.15,.18,79,.3)],'triangle')},'open':{'gain':.5,'source':score([(0,.12,60,-.4),(.2,.12,64,0),(.4,.18,67,.4)],'triangle')},'reject':{'gain':.45,'source':score([(0,.2,48,0),(.25,.3,43,0)],'saw')},'resolve':{'gain':.55,'source':score([(0,.2,72,-.4),(.3,.2,76,0),(.6,.3,79,.4),(1,.8,84,0)],'triangle')}},'music':{'kind':'score','score':{'bpm':132,'beats':8,'layers':[{'name':'bounce','gain':.2,'instrument':{'wave':'triangle','attack':.006,'decay':.08,'sustain':.1,'release':.1},'notes':[{'at':n*.5,'beats':.25,'midi':[48,60,55,64][n%4],'pan':(-.3 if n%2 else .3)} for n in range(16)]}]}}}
for name,project in [('notebook',notebook),('instrument',instrument),('arcade',arcade)]:
 (root/(name+'.json')).write_text(json.dumps(project,indent=2)+'\n')
