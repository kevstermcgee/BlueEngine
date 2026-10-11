#!/usr/bin/env python3
"""Original score: four independent D-Lydian/B-minor memories in prime-length phrases.
Render each JSON with BlueEngine's audio authoring tool; no third-party recordings.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHORDS = [
    [[50,57,61,66],[47,54,61,66],[55,62,66,69],[50,57,64,68]],
    [[62,66,69,73],[59,66,71,73],[64,68,71,74],[62,69,73,78]],
    [[47,54,59,66],[50,57,61,64],[52,59,64,68],[54,61,66,69]],
    [[57,61,64,69],[55,62,66,71],[50,57,64,68],[47,54,61,66]],
]
MELODIES = [
    [78,76,73,69,71,73,76,78,80,78,76,73],
    [81,78,76,73,76,80,78,73,71,69,73,76],
    [73,71,66,69,73,76,74,73,71,69,66,73],
    [76,73,69,68,71,73,78,76,73,71,69,66],
]
for index,(name,seconds) in enumerate(zip(['home','light','memory','beyond'],[43,47,53,59])):
    ground=[]; stars=[]
    slot=seconds/4
    for chord,notes in enumerate(CHORDS[index]):
        for voice,midi in enumerate(notes):
            ground.append(dict(at=round(chord*slot,4),beats=round(slot,4),midi=midi,velocity=0.45+voice*0.07,pan=(voice-1.5)*0.25))
    for n,midi in enumerate(MELODIES[index]):
        at=round(1.3+n*(seconds-4)/12+(0.15 if n%3==1 else 0),4)
        stars.append(dict(at=at,beats=round(min(1.2+(n%3)*0.65,seconds-at),4),midi=midi,velocity=[0.52,0.36,0.42][n%3],pan=(-0.5 if n%2 else 0.5)))
        if n%4==0:
            stars.append(dict(at=at+0.3,beats=0.9,midi=midi-12,velocity=0.24,pan=0.0))
    project=dict(version=1,seed=2401+index,headroom=0.72,effects={},music=dict(kind='score',score=dict(bpm=60,beats=seconds,layers=[
        dict(name='ground',gain=0.40,instrument=dict(wave='triangle',attack=3.2,decay=2.5,sustain=0.6,release=4.8,lowpass_hz=680+index*110),notes=ground),
        dict(name='stars',gain=0.28,instrument=dict(wave='sine',attack=0.018,decay=0.8,sustain=0.10,release=3.9,lowpass_hz=4600),notes=stars),
    ])))
    (ROOT/'assets/score'/f'{name}.json').write_text(json.dumps(project,indent=2)+'\n')
