#!/usr/bin/env python3
"""Reproducible original Embertrail score: composed notes, offline additive synthesis.

Every piece has its own melody, progression, meter, orchestration and four long
sections. No borrowed recordings, generated ambient loops, or pitch-shifted songs.
Run from any directory; needs a C compiler, no Python audio dependencies.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "assets/audio/score"
# id, title, bpm, meter, tonic, scale, progression, lead, melody A, melody B, mood
PIECES = [
 ("title", "A Small Light, A Long Road", 112, 4, 60, [0,2,4,5,7,9,11], [0,3,5,4,0,5,1,4], 1, [0,2,4,6,4,2,1,3,5,4,2,0,2,1,0,4], [7,6,4,5,3,2,4,6,5,3,1,2,4,2,1,0], "Warm lute, woodwind and lantern bells; Mote's promise"),
 ("meadow", "First Sparks in Fernlight", 124, 4, 62, [0,2,4,6,7,9,11], [0,4,1,3,5,2,3,4], 0, [0,2,4,2,6,5,4,1,3,5,7,6,4,2,1,0], [4,6,8,7,5,4,3,1,5,3,2,4,6,5,2,0], "Buoyant flute and plucked strings for early exploration"),
 ("moon", "When Crystals Learn to Sing", 92, 3, 57, [0,2,3,5,7,8,10], [0,5,2,6,3,0,4,4], 2, [0,4,7,5,3,2,4,6,5,1,3,4,2,0,-1,0], [7,9,8,5,6,4,3,5,8,6,4,2,1,3,2,0], "Swaying cave nocturne, glass melody and soft brushed frame drum"),
 ("marsh", "Reeds Beneath Copper Rain", 108, 4, 55, [0,2,3,5,7,9,10], [0,3,0,5,1,4,6,4], 3, [0,1,3,4,2,5,4,1,6,4,3,1,2,0,3,4], [5,7,6,3,4,2,0,2,6,5,3,4,1,2,0,-1], "Syncopated reeds, hollow percussion and warm bass through wetlands"),
 ("frost", "Snowglass Waltz", 88, 3, 65, [0,2,4,5,7,9,11], [0,2,5,3,1,4,0,4], 7, [4,7,6,2,5,4,1,3,2,0,2,4,6,5,3,1], [7,5,8,6,4,2,5,3,6,4,2,1,3,5,2,0], "Celesta and bowed chords, spacious three-beat skating pulse"),
 ("dunes", "Caravans of the Sunseed", 126, 4, 57, [0,1,4,5,7,8,10], [0,1,4,0,5,3,1,4], 1, [0,1,4,3,1,0,2,4,5,3,2,1,4,6,3,0], [7,6,4,5,3,1,2,4,6,5,3,4,2,1,-1,0], "Lute flourishes, hand drums and winding sunlit phrases"),
 ("sky", "Kites Above the Sleeping Sea", 116, 4, 64, [0,2,4,6,7,9,11], [0,1,3,4,5,2,1,4], 0, [7,6,4,2,3,5,6,8,7,4,5,3,2,4,1,0], [4,5,7,9,8,6,4,5,7,5,3,1,2,4,2,0], "Airborne woodwind duet with rising strings and light marimba"),
 ("forge", "Lanterns in the Briar Choir", 110, 4, 50, [0,2,3,5,7,8,10], [0,6,3,4,0,2,5,4], 5, [0,4,2,3,6,4,5,2,1,3,4,0,2,5,3,1], [7,4,6,8,5,3,4,2,6,5,2,4,1,3,0,-1], "Bowed woodland choir, seedpod percussion and uneasy dusk harmonies"),
 ("citadel", "Ashes Keep Their Promises", 120, 4, 55, [0,2,3,5,7,8,11], [0,5,3,4,1,6,2,4], 5, [0,2,3,6,4,1,2,5,3,0,-1,2,4,3,1,0], [7,6,4,8,5,3,1,4,6,5,2,3,1,2,-1,0], "Resolute strings and low horns for the climb to the stolen Dawnheart"),
 ("underroots", "Where the Roots Hide Stars", 84, 3, 60, [0,2,3,5,7,9,10], [0,3,5,2,1,6,3,4], 2, [0,4,2,6,3,5,1,4,7,5,2,3,6,4,1,0], [7,4,8,6,3,5,2,4,6,5,1,3,4,2,-1,0], "Secret grotto music: waterlike bells and curious pizzicato"),
 ("warden-root", "Thorns Around a Tender Heart", 142, 4, 50, [0,2,3,5,7,8,10], [0,5,3,4,0,6,2,4], 3, [0,1,4,3,6,4,2,0,3,5,4,1,2,4,1,-1], [7,5,6,8,4,3,5,2,6,4,2,3,1,0,2,-1], "Agile reed duels and rattling seedpods for the woodland guardians"),
 ("warden-tide", "Keepers of Stone and Water", 138, 5, 53, [0,2,3,5,7,9,10], [0,3,6,4,1,5,2,4], 7, [0,3,1,4,2,6,4,3,0,2,5,4,1,3,2,-1], [7,4,6,3,5,8,6,2,4,5,1,3,4,2,0,-1], "Five-beat bronze rhythm for Chimeback and the marsh pump-keeper"),
 ("warden-gale", "A Crown of Ice and Thunder", 148, 4, 57, [0,2,4,5,7,8,11], [0,5,2,4,3,6,1,4], 4, [0,4,5,2,6,3,4,1,7,5,4,2,3,6,1,0], [7,8,6,4,5,3,1,4,6,5,2,4,1,3,-1,0], "Bright horns cut through ice bells and galloping strings"),
 ("warden-iron", "The Lion Behind the Sun", 144, 4, 48, [0,1,3,5,7,8,10], [0,1,5,4,3,6,1,4], 4, [0,1,3,4,6,3,2,0,4,5,3,1,2,4,1,-1], [7,6,8,4,5,3,6,2,5,4,1,3,2,0,1,-1], "Heavy hand drums, low brass and sunglass chimes for Solarch"),
 ("vesper", "Vesper's Last Toll", 150, 4, 50, [0,1,3,5,6,8,11], [0,1,4,0,5,3,1,4], 4, [0,1,4,3,6,4,1,0,5,3,2,4,6,3,1,-1], [7,6,4,8,5,3,6,2,5,4,1,3,2,0,-1,0], "The Bell Regent's final confrontation: urgent horns, dark bells and Mote's answer"),
 ("victory", "Every Window Holds the Dawn", 118, 4, 62, [0,2,4,5,7,9,11], [0,3,5,4,1,3,0,4], 0, [0,2,4,7,6,4,5,3,2,4,6,5,3,1,2,0], [7,9,8,6,4,5,7,6,8,5,3,4,2,1,-1,0], "Homecoming reprise with full warm ensemble and a new soaring resolution"),
]
# A and B phrase rhythms are authored separately for every piece, in quarter beats.
# Triple and five-beat pieces retain their own meter; these are not transpositions.
RHYTHMS = {
 "title": ([0,1,1.75,3], [0,.5,2,2.75]),
 "meadow": ([0,.5,1.5,2,3.25], [0,.75,1.25,2.5,3]),
 "moon": ([0,1,2.25], [0,.5,1.5,2]),
 "marsh": ([0,.75,1.75,2.5,3.5], [0,.5,1.25,2.75]),
 "frost": ([0,1.5,2], [0,.5,1,2]),
 "dunes": ([0,.5,1.75,2.5,3,3.5], [0,.75,1.5,2.25,3.25]),
 "sky": ([0,1.25,2,3.5], [0,.75,1.5,2.75]),
 "forge": ([0,1.5,2.5,3.25], [0,.5,1.75,3]),
 "citadel": ([0,.5,1.5,2.75], [0,1,1.75,2.5,3.5]),
 "underroots": ([0,1.5,2.25], [0,.75,1.25,2.5]),
 "warden-root": ([0,.5,1.25,2.25,3.5], [0,.75,1.5,2,3.25]),
 "warden-tide": ([0,1,1.5,3,4.25], [0,.75,2,3.25,4]),
 "warden-gale": ([0,.75,1.5,2,3], [0,.5,1.25,2.5,3.5]),
 "warden-iron": ([0,.5,1.5,2.5,3.25], [0,1,1.75,2.25,3.5]),
 "vesper": ([0,.5,1.25,2,2.75,3.5], [0,.75,1.5,2.5,3.25]),
 "victory": ([0,1,2,2.5,3.25], [0,.5,1.5,2.75,3.5]),
}

# Quiet, material-specific battle accents preserve each existing composition.
# Positions are beats within its own meter; the five-beat keeper retains3+2 phrasing.
BATTLE_ACCENTS = {
 "warden-root": ([.22,2.22], [(1.75,45),(3.25,50)], "Seedpod taps and a soft double heartbeat beneath the reeds"),
 "warden-tide": ([.18,3.18], [(2.5,43),(4.5,48)], "Low bronze-frame pulse grouped3+2, with a round bass answer"),
 "warden-gale": ([.20,2.20], [(1.5,55),(3.5,52)], "Light galloping frame-drum accents beneath ice bells and horns"),
 "warden-iron": ([.24,2.24], [(1.75,38),(3.25,43)], "Restrained low hand-drum heartbeat beneath the lion's brass"),
 "vesper": ([.16,2.16], [(1.25,42),(2.75,47),(3.5,45)], "Uneven night-bell drum answers and a quiet final-heartbeat pulse"),
}

def compose(piece):
    ident,title,bpm,meter,root,scale,progression,lead,a,b,mood = piece
    # Each complete piece has four eight-bar sections; slow triple pieces use 40 bars.
    bars = 40 if meter == 3 else 40 if meter == 4 and bpm >= 138 else 32
    if ident == "warden-tide": bars = 32
    beat = 60 / bpm
    seconds = bars * meter * beat
    notes=[]
    def pitch(degree, octave=0):
        return root + scale[degree % 7] + 12*(degree//7+octave)
    def add(at,length,key,volume,instrument,pan=0):
        notes.append((at*beat,length*beat,key,volume,pan,instrument))
    boss = ident.startswith("warden") or ident == "vesper"
    for bar in range(bars):
        section = min(3,bar*4//bars)
        chord = progression[(bar + (2 if section==2 else 0)) % 8]
        t=bar*meter
        # Individually voiced harmony moves on every bar; bridge opens its register.
        for ci,degree in enumerate((chord,chord+2,chord+4)):
            add(t+.025*ci,meter-.2,pitch(degree,-1 if ci==0 else 0),.11 if section!=2 else .13,5,(-.45,0,.45)[ci])
        # Bass answers rather than endlessly repeating the same riff.
        add(t,meter*.42,pitch(chord,-2),.27,6,-.06)
        add(t+meter*.5,meter*.4,pitch(chord+(4 if bar%2==0 else 2),-2),.2,6,.06)
        motif = a if section in (0,3) else b
        offset=(bar%8)*2
        rhythm = RHYTHMS[ident][1 if section==1 else 0]
        if section==2: rhythm = [0,meter*.5,meter*.75]
        if bar%8 in (3,7): rhythm = rhythm[:-1] # phrasing breath
        for ni,pos in enumerate(rhythm):
            degree=motif[(offset+ni)%len(motif)]
            if section==3 and ni==len(rhythm)-1:degree+=2
            length=(.48 if boss else .68) if ni<len(rhythm)-1 else .95
            add(t+pos,min(length,meter-pos-.08),pitch(degree,1),.19 if section!=2 else .16,lead,-.18)
        # Answering voice enters in section B; the bridge changes the instrumentation.
        if section in (1,2,3) and bar%2:
            for ni,pos in enumerate([.5,1.5,meter-.5]):
                answer = PIECES[0][8] if ident=="vesper" and section==3 else b if section!=2 else a
                add(t+pos,.38,pitch(answer[(bar*3+ni)%16],0),.09,2 if lead!=2 else 1,.38)
        if section!=2 or bar%2==0:
            for k in range(meter*2):
                if (k+bar)%3!=0:
                    degree=chord+(0,2,4,6)[(k+bar)%4]
                    add(t+k*.5,.24,pitch(degree,0),.065 if not boss else .08,1 if lead!=1 else 7,(-.48,.48)[k%2])
        # Hand-authored groove families: waltz, brushes, marching, hand drums.
        if ident not in ("title","underroots") or section in (1,3):
            for k in range(meter):
                if k==0 or (boss and k%2==0):add(t+k,.12,42,.2 if boss else .13,8)
                if k==meter-1 or (meter==4 and k==1):add(t+k,.14,54,.12 if boss else .065,9,.15)
                add(t+k+.5,.06,72,.10 if boss else .07,10,(-.3,.3)[k%2])
            if ident in ("dunes","marsh","warden-tide","warden-iron"):
                for k in (0,1.5,meter-.5):add(t+k,.18,45+(bar%3)*2,.13,11,-.25)
        # Drum fills occur only at phrase ends and evolve by section.
        if boss and bar%8==7:
            for k in range(4):add(t+meter-1+k*.25,.1,48+k*2,.09+k*.015,11,(k-1.5)*.15)
        if ident in BATTLE_ACCENTS:
            heartbeats,accents,_ = BATTLE_ACCENTS[ident]
            strength=(.72,.90,.45,1.0)[section]
            # Leave a breath at phrase ends and in alternate bridge bars; tension
            # comes from orchestration and rhythm, not a master-volume increase.
            if section!=2 or bar%2==0:
                for pos in heartbeats[:1 if bar%4==3 else 2]:
                    add(t+pos,.10,40,.055*strength,8)
                for j,(pos,key) in enumerate(accents):
                    if (bar+j)%3!=2:
                        add(t+pos,.11,key+(bar%2)*2,.040*strength,11,(-.18,.18)[j%2])
                if bar%4!=3:
                    add(t+meter-.75,.19,pitch(chord+(2 if bar%2 else 4),-2),.058*strength,6,.04)
        if section==3 and bar%4==0:
            add(t+.04,meter*.6,pitch(chord+7,1),.10,2,.4)
    return seconds,notes,bars

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--only",nargs="*");args=parser.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    catalog=[]
    with tempfile.TemporaryDirectory(prefix="embertrail-score-") as temporary:
        temporary=Path(temporary);binary=temporary/"render-score"
        subprocess.run(["cc","-O3",str(ROOT/"scripts/render_score.c"),"-lm","-o",str(binary)],check=True)
        for piece in PIECES:
            ident,title,bpm,meter,root,scale,progression,lead,a,b,mood=piece
            seconds,notes,bars=compose(piece)
            catalog.append(dict(id=ident,title=title,seconds=round(seconds,6),bpm=bpm,meter=meter,bars=bars,mood=mood))
            if args.only and ident not in args.only:continue
            directory=OUT/ident;directory.mkdir(exist_ok=True)
            events=temporary/(ident+".txt");events.write_text("".join(" ".join(map(str,n))+"\n" for n in notes))
            info=temporary/(ident+".json")
            subprocess.run([str(binary),str(events),str(seconds),str(directory/"song.wav"),str(info)],check=True)
            metadata=json.loads(info.read_text())
            bank=dict(version=1,sample_rate=44100,effects={},music={"song":metadata},music_scale=1.0)
            (directory/"bank.json").write_text(json.dumps(bank,indent=2)+"\n")
            composition=dict(title=title,bpm=bpm,meter=meter,bars=bars,tonic_midi=root,scale=scale,progression=progression,melody_a=a,melody_b=b,rhythm_a=RHYTHMS[ident][0],rhythm_b=RHYTHMS[ident][1],sections=["A: introduce","B: answer","C: change register and texture","A prime: counterline and resolution"],note_events=len(notes),provenance="Original Embertrail composition and offline additive instruments; no samples")
            if ident in BATTLE_ACCENTS:
                composition["battle_arrangement"]=dict(version=1,heartbeats=BATTLE_ACCENTS[ident][0],frame_drum_accents=BATTLE_ACCENTS[ident][1],description=BATTLE_ACCENTS[ident][2],section_strength=[.72,.90,.45,1.0],extra_bass_answer="Short chord-tone answer before the next downbeat; rests at phrase ends")
            (directory/"composition.json").write_text(json.dumps(composition,indent=2)+"\n")
            print(f"{title}: {seconds:.1f}s, {len(notes)} events, RMS {metadata['rms']:.3f}, seam {metadata['seam_jump']:.5f}",flush=True)
    (OUT/"tracks.json").write_text(json.dumps(catalog,indent=2)+"\n")
if __name__=="__main__":main()
