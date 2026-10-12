#!/usr/bin/env python3
"""Original, hand-authored pixel art. Uses BlueEngine pixel-art export; no runtime authoring dependencies."""
from pathlib import Path
import sys
import tomllib
project = Path(__file__).resolve().parent.parent
dependency = tomllib.loads((project / "Cargo.toml").read_text())["dependencies"]["vesper3d"]["path"]
sys.path.insert(0, str((project / dependency).resolve() / "tools"))
import pixelart
import json
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'assets/sprites'
PALETTE={'.':(0,0,0,0),'#':'#142e37','a':'#217866','b':'#54bfa1','c':'#a6efd1','w':'#f6f5d7','e':'#18384a','g':'#f2bd60','h':'#fff4ad','o':'#cd784b','r':'#ba5b76','p':'#eb92ac','v':'#56506f','t':'#9590b9','s':'#647d98'}
HERO=[
'......g.........','.....ghg........','......a.........','....aacaa.......','...accccba......','..accccccbba....','..acwcwcccbba...','..acwewecccba...','..accccccccba...','...bcccbccba....','....gggggg......','...gggoggggg....','..acbbbbbb..gg..','..acbcwcbba..g..','..abccgcbba.....','...abbcbba......','....abbaa.......','....a..a........','...aa..aa.......','...##..##.......',
]
MITE=['....aaaa......','..aabbbbaa....','.abbcccbba....','abbcccccbba...','abccwccwcba...','abccwccwcba...','abccaccacba...','.abbcccbba....','..aabbbbaa....','..##..##......']
BEETLE=['....gggg......','..ggohhogg....','.gohhhhhoog...','gohhhohhhhoog.','gohhhhhhhhogg.','gohhhohhhoogg.','.goooooooog...','..gggggggg....','..ww##ww##....','...#...#......']
WISP=['.ww.....ww....','wwww...wwww...','.wwww.wwww....','..vvttvv......','.vttttttv.....','vttwttwttv....','vttwttwttv....','.vttttttv.....','..vttttv......','...v..v.......']
SHELL=['..............','..............','..............','..............','...gggggg.....','..gohhhoog....','.gohhhhhoog...','gohhhohhhhoog.','gooooooooog...','.ggggggggg....']
THORN=['.....rp.......','....rppp......','...rppppr.....','..rppwpwpr....','..rppwpwpr....','...rrrrrr.....','.....aa.......','..aa.aa.aa....','.abbaaaaabba..','..aa.aa.aa....','.....aa.......','.....aa.......','.....aa.......','.....aa.......','.....aa.......','.....aa.......','.....aa.......','....aaaa......']
BOSS=[
'.........gggg.............','........gohhog............','.......gooooog............','.......ggggggg............','......vvvvvvvvv...........','....vvtttttttttvv.........','...vtttttttttttttv........','..vtttttttttttttttv.......','..vtttwwttttwwttttv.......','..vtttwwttttwwttttv.......','..vtttggttttggttttv.......','..vtttttttttttttttv.......','..vtttttttttttttttv.......','..vtttttvvvtttttttv.......','..vttttvvvvvttttttv.......','..vtttttvvvtttttttv.......','.vtttttttttttttttttv......','.vtttttttttttttttttv......','vtttttttttttttttttttv.....','vvvvvvvvvvvvvvvvvvvvv.....','....vttv......vttv........','....vvvv......vvvv........','.....gg........gg.........','....gggg......gggg........',
]
def save(name,rows):
    width=max(map(len,rows))
    palette={k:tuple(bytes.fromhex(v[1:]))+(255,) if isinstance(v,str) else v for k,v in PALETTE.items()}
    pixels=pixelart.render([row.ljust(width,'.') for row in rows],palette)
    (OUT/(name+'.png')).write_bytes(pixelart.png_bytes(pixels));return pixels
for i in range(4):
    rows=HERO.copy()
    if i==1:rows[17]='....aa.a........';rows[18]='....##.aaa......';rows[19]='.......###......'
    if i==2:rows[17]='....a.aa........';rows[18]='..aaa.##........';rows[19]='..###...........'
    if i==3:rows[17]='..aaa..aaa......';rows[18]='..###..###......';rows[19]='................'
    save('mote'+str(i),rows)
    save('mote_left'+str(i),[row[::-1] for row in rows])
for name,rows in [('mite',[row.translate(str.maketrans({'a':'v','b':'r','c':'p'})) for row in MITE]),('beetle',BEETLE),('wisp',WISP),('shell',SHELL),('thorn',THORN),('keeper',BOSS)]:save(name,rows)
hero=save('mote0',HERO)
pixelart.write_icon_set(ROOT/'assets',hero,(33,62,70,255),(143,226,189,255))
# Original 16-bar chip score, with authored melodic phrases and harmonic bass.
phrases=[[76,79,81,79,76,72,74,76],[79,83,84,83,79,76,74,72],[77,81,84,81,77,74,76,77],[74,77,79,77,74,71,72,74]]
notes=[]
for bar in range(16):
    phrase=phrases[bar%4]
    for j,midi in enumerate(phrase):notes.append({'at':bar*4+j*0.5,'beats':0.35 if j<7 else 0.45,'midi':midi,'velocity':0.72 if j%2==0 else 0.52,'pan':-0.15 if bar%2==0 else 0.15})
bass=[];glimmer=[]
for bar in range(16):
    root=[48,45,41,43][bar%4]
    for beat in range(4):bass.append({'at':bar*4+beat,'beats':0.7,'midi':root if beat%2==0 else root+7,'velocity':0.58})
    for beat,m in enumerate([root+24,root+28,root+31,root+28]):glimmer.append({'at':bar*4+beat+0.25,'beats':0.2,'midi':m,'velocity':0.4,'pan':0.4 if beat%2 else -0.4})
score={'version':1,'seed':71,'headroom':0.78,'effects':{},'music':{'kind':'score','score':{'bpm':138,'beats':64,'layers':[
{'name':'melody','gain':0.22,'instrument':{'wave':'square','attack':0.005,'decay':0.09,'sustain':0.3,'release':0.09,'lowpass_hz':4200},'notes':notes},
{'name':'bass','gain':0.28,'instrument':{'wave':'triangle','attack':0.006,'decay':0.08,'sustain':0.5,'release':0.07},'notes':bass},
{'name':'glimmer','gain':0.18,'instrument':{'wave':'sine','attack':0.003,'decay':0.08,'sustain':0.15,'release':0.12},'notes':glimmer}]}}}
(ROOT/'assets/audio-source/trail.json').write_text(json.dumps(score,indent=2)+'\n')
print('Authored sprites, lantern icon and original score.')
