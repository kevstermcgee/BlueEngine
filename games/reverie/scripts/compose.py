#!/usr/bin/env python3
"""Reproducible original score: eight emotional movements, with independently mixed piano, strings and glints."""
import json, pathlib, subprocess, sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
TITLES=['wonder','longing','grief','joy','unease','anger','tenderness','acceptance']
# Ninth and suspended voicings; melodies answer a four-note motif rather than repeat a stock arpeggio.
HARMONIES=[
 [[48,55,62,64],[45,52,59,60],[41,48,55,57],[43,50,57,59]],
 [[45,52,59,60],[41,48,55,57],[48,55,62,64],[43,50,57,59]],
 [[38,45,52,53],[34,41,48,50],[41,48,55,57],[36,43,50,52]],
 [[50,57,64,66],[47,54,61,62],[43,50,57,59],[45,52,59,61]],
 [[42,49,55,57],[40,47,54,58],[45,52,59,60],[39,46,53,57]],
 [[40,47,52,55],[36,43,50,52],[38,45,52,53],[35,42,49,52]],
 [[48,55,62,64],[41,48,55,57],[45,52,59,60],[43,50,57,60]],
 [[43,50,57,59],[40,47,54,55],[36,43,50,52],[38,45,52,55]]]
MELODIES=[[76,79,74,72,71,74,72,67],[72,71,67,64,69,67,64,62],[65,64,60,57,62,60,57,53],
          [78,81,76,74,73,76,78,69],[69,70,66,64,63,67,66,61],[67,64,62,59,64,62,59,55],
          [76,74,72,67,69,72,71,67],[71,69,67,62,64,67,66,62]]
def note(at,beats,midi,vel,pan):return {'at':at,'beats':beats,'midi':midi,'velocity':vel,'pan':pan}
def project(idx):
 bpm=[68,64,60,76,66,72,64,62][idx]
 # Keep the three-layer budget <=180 stereo seconds.
 beats=40
 piano=[];strings=[];glints=[]
 for bar in range(5):
  chord=HARMONIES[idx][bar%4];start=bar*8
  for voice,midi in enumerate(chord[:3]):
   strings.append(note(start,7.25,midi,0.40 if voice==0 else 0.32,[-0.55,0.10,0.55][voice]))
  for n,offset in enumerate([0,1.45,2.9,4.2,5.65,7.05]):
   midi=chord[[1,2,3,2,1,3][n]]+12
   piano.append(note(start+offset,0.44,midi,0.44+0.17*((bar+n)%3)/2,[-0.32,0.14,0.40,0.22,-0.2,0.0][n]))
  for n,offset in enumerate([0.15,2.35,4.4,6.65]):
   midi=MELODIES[idx][(bar*3+n)%8]
   piano.append(note(start+offset,0.60,midi,0.66 if n==0 else 0.48,0.12))
  if bar%2==0:
   glints.append(note(start+3.1,1.5,chord[3]+24,0.20,-0.6))
   glints.append(note(start+6.25,1.3,chord[2]+24,0.16,0.6))
 layers=[
  {'name':'piano','gain':0.5,'instrument':{'wave':'triangle','attack':0.004,'decay':0.48,'sustain':0.12,'release':1.7,'lowpass_hz':2300},'notes':piano},
  {'name':'strings','gain':0.36,'instrument':{'wave':'saw','attack':1.5,'decay':0.9,'sustain':0.65,'release':2.4,'lowpass_hz':620},'notes':strings},
  {'name':'glints','gain':0.20,'instrument':{'wave':'sine','attack':0.03,'decay':0.4,'sustain':0.20,'release':2.5},'notes':glints}]
 return {'version':1,'seed':410+idx,'headroom':0.7,
  'effects':{'step':{'gain':0.18,'source':{'kind':'preset','preset':'footstep'}},
   'enter':{'gain':0.35,'source':{'kind':'preset','preset':'whoosh'}},
   'wake':{'gain':0.25,'source':{'kind':'preset','preset':'chime'}},
   'key':{'gain':0.40,'source':{'kind':'preset','preset':'success'}},
   'locked':{'gain':0.12,'source':{'kind':'preset','preset':'back'}}},
  'music':{'kind':'score','score':{'bpm':bpm,'beats':beats,'layers':layers}}}
if __name__=='__main__':
 tool=sys.argv[1] if len(sys.argv)>1 else 'be2-tools'
 src=ROOT/'assets'/'score';src.mkdir(parents=True,exist_ok=True)
 (ROOT/'assets'/'audio').mkdir(parents=True,exist_ok=True)
 for idx,title in enumerate(TITLES):
  path=src/(title+'.json');path.write_text(json.dumps(project(idx),indent=2)+'\n')
  out=ROOT/'assets'/'audio'/title
  if out.exists():
   subprocess.run([tool,'audio','check',str(out)],check=True)
  else:subprocess.run([tool,'audio','render',str(path),str(out)],check=True)
 print('Eight original Reverie movements rendered and checked.')
