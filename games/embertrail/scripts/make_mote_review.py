#!/usr/bin/env python3
"""Review the shipped Mote poses and proposed turnarounds without changing game assets.

Uses the game's original standard-library pixel canvas plus ffmpeg's font/GIF
rendering. All output stays in .blue-check/mote-review.
"""
from pathlib import Path
import ast
import hashlib
import json
import struct
import subprocess
import zipfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.blue-check' / 'mote-review'
RAW = OUT / 'raw'
RAW.mkdir(parents=True, exist_ok=True)
FONT = ROOT / 'assets' / 'fonts' / 'DejaVuSans.ttf'
tree = ast.parse((ROOT / 'scripts' / 'make_world_art.py').read_text())
canvas_node = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'Canvas')
namespace = {'Path': Path, 'struct': struct, 'zlib': zlib, 'OUT': RAW}
exec(compile(ast.Module(body=[canvas_node], type_ignores=[]), '<original pixel canvas>', 'exec'), namespace)
Canvas = namespace['Canvas']
INK = '#152b36'
PAPER = '#fff3cb'
GOLD = '#f5bc65'
MINT = '#a4f1ce'
TEXT = '#eef3e6'
MUTED = '#b6c8ce'


def load_png(path):
    """Decode the game's original RGBA PNGs, all authored with filter type zero."""
    data = path.read_bytes()
    at = 8; compressed = bytearray(); w = h = 0
    while at < len(data):
        count = struct.unpack('>I', data[at:at+4])[0]
        kind = data[at+4:at+8]; block = data[at+8:at+8+count]
        if kind == b'IHDR':
            w, h, depth, color, _, _, _ = struct.unpack('>IIBBBBB', block)
            assert depth == 8 and color == 6
        elif kind == b'IDAT': compressed.extend(block)
        at += count + 12
    raw = zlib.decompress(compressed); stride = 1 + w * 4
    c = Canvas(w, h)
    for y in range(h):
        assert raw[y * stride] == 0, 'Review decoder expects original filter-zero art.'
        row = raw[y * stride + 1:(y+1)*stride]
        c.pixels[y] = [tuple(row[x*4:x*4+4]) for x in range(w)]
    return c


def blit(target, source, x, y, zoom=1):
    for sy, row in enumerate(source.pixels):
        for sx, rgba in enumerate(row):
            if rgba[3]: target.rect(x + sx*zoom, y + sy*zoom, zoom, zoom, rgba)


def base_body(c, facing='front'):
    c.poly([(13, 9), (11, 6), (15, 2), (18, 0), (18, 5), (21, 4), (19, 10)], '#448b7a')
    c.poly([(14, 7), (17, 3), (17, 7), (19, 6), (18, 9)], MINT)
    c.poly([(12, 23), (22, 23), (25, 33), (21, 35), (10, 34), (7, 32)], INK)
    c.poly([(13, 24), (21, 24), (23, 32), (19, 33), (10, 32)], '#4da990')
    c.rect(11, 26, 3, 5, '#8edbb9')
    if facing == 'front':
        c.rect(17, 26, 2, 7, '#b1dfb6')
        c.rect(15, 29, 6, 2, GOLD)
        c.rect(17, 28, 2, 4, PAPER)
    else:
        c.rect(16, 25, 2, 8, '#347c70')
        c.line((17, 27), (12, 31), '#77c4a4')
        c.line((17, 29), (21, 32), '#77c4a4')
    for x in (11, 21):
        c.rect(x, 34, 5, 4, INK); c.rect(x+1, 34, 3, 1, '#a17b57')


def concepts():
    c = Canvas(32, 40); base_body(c)
    c.poly([(7, 23), (1, 21), (2, 26), (10, 28), (17, 24)], INK)
    c.poly([(7, 24), (3, 23), (3, 25), (10, 26), (17, 24)], GOLD)
    c.ellipse(6, 8, 24, 18, INK); c.ellipse(8, 10, 20, 14, '#397a6e')
    c.ellipse(10, 11, 16, 13, PAPER)
    c.rect(10, 14, 2, 7, '#e7c995'); c.rect(24, 14, 2, 6, '#e7c995')
    c.rect(14, 16, 2, 3, '#514b43'); c.rect(21, 16, 2, 3, '#514b43')
    c.rect(14, 16, 1, 1, '#a69b7b'); c.rect(21, 16, 1, 1, '#a69b7b')
    c.rect(17, 21, 4, 1, '#b77558')
    c.rect(13, 24, 10, 3, GOLD); c.rect(17, 24, 3, 2, PAPER)
    c.save('proposed-front')
    front = c
    c = Canvas(32, 40); base_body(c, 'back')
    c.ellipse(6, 8, 24, 18, INK); c.ellipse(8, 10, 20, 14, '#397a6e')
    c.ellipse(9, 10, 15, 13, '#4d9b84'); c.rect(10, 13, 2, 7, '#78c5a4')
    c.line((17, 11), (17, 23), '#28685f')
    c.rect(10, 23, 15, 4, INK); c.rect(11, 23, 13, 3, GOLD)
    c.poly([(22, 24), (28, 24), (29, 31), (25, 34), (23, 29)], INK)
    c.poly([(23, 25), (26, 25), (27, 30), (25, 31)], GOLD)
    c.rect(23, 25, 3, 1, PAPER)
    c.save('proposed-back')
    back = c
    c = Canvas(32, 40); base_body(c)
    c.poly([(14, 21), (8, 21), (2, 19), (0, 23), (7, 26), (17, 24)], INK)
    c.poly([(14, 22), (8, 22), (3, 21), (3, 23), (8, 24), (17, 23)], GOLD)
    c.poly([(12, 8), (22, 8), (27, 12), (28, 21), (23, 25), (11, 24), (7, 19), (8, 13)], INK)
    c.poly([(12, 10), (21, 10), (25, 13), (26, 21), (22, 23), (11, 22), (9, 18), (10, 14)], '#397a6e')
    c.poly([(15, 11), (23, 12), (26, 15), (26, 17), (29, 18), (28, 20), (25, 21), (23, 24), (15, 22), (12, 18), (13, 14)], PAPER)
    c.rect(13, 14, 2, 6, '#e7c995')
    c.rect(16, 16, 1, 2, '#514b43'); c.rect(22, 16, 2, 3, '#514b43')
    c.rect(22, 16, 1, 1, '#a69b7b')
    c.rect(22, 21, 4, 1, '#b77558'); c.rect(10, 13, 2, 7, '#63bca2')
    c.rect(14, 24, 10, 3, GOLD); c.rect(19, 25, 2, 1, PAPER)
    c.save('proposed-three-quarter-right'); c.save('proposed-three-quarter-left', True)
    return [front, back, load_png(RAW/'proposed-three-quarter-left.png'), c]


def draw_text(source, destination, labels):
    filters = []
    for i, (text, x, y, size, color) in enumerate(labels):
        textfile = RAW / f'label-{i}.txt'; textfile.write_text(text)
        filters.append(f"drawtext=fontfile={FONT}:textfile={textfile}:fontcolor={color}:fontsize={size}:x={x}:y={y}")
    subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', str(source), '-vf', ','.join(filters), '-frames:v', '1', '-update', '1', str(destination)], check=True)


def card(c, x, y, w, h):
    c.rect(x, y, w, h, '#233944')
    c.rect(x, y, w, 3, '#4f6d76')
    c.rect(x+15, y+44, w-30, h-75, '#304954')
    c.rect(x+w//2-91, y+h-40, 182, 3, '#243c45')


def contact(current, proposed):
    c = Canvas(1760, 1460); c.rect(0, 0, c.w, c.h, '#142b36')
    labels = [('MOTE / CHARACTER & ANIMATION REVIEW',64,40,40,TEXT),
              ('Current in-game sprites are shown first. New angles below are concepts for review.',64,102,22,MUTED),
              ('CURRENT / IN GAME — the actual runtime sprites',64,156,28,MINT)]
    frame_names = ['Idle', 'Run · stride A', 'Run · stride B', 'Jump']
    for row, direction in enumerate(['RIGHT', 'LEFT']):
        for frame in range(4):
            x=64+414*frame; y=205+308*row; card(c,x,y,390,286)
            blit(c,current[row*4+frame],x+115,y+51,5)
            name=('mote_left' if row else 'mote')+str(frame)+'.png'
            labels += [(f'{direction} / {frame_names[frame]}',x+18,y+15,22,TEXT),(name,x+18,y+263,17,MUTED)]
    labels += [('PROPOSED / TURNAROUND CONCEPTS',64,848,28,GOLD),
               ('These four views are new proposals. They have not replaced Mote in the game.',64,889,22,MUTED)]
    for i,(pose,name) in enumerate(zip(proposed,['Front','Back · hood & scarf','Three-quarter left','Three-quarter right'])):
        x=64+414*i; y=937; card(c,x,y,390,296); blit(c,pose,x+115,y+56,5)
        labels += [(name,x+18,y+15,22,TEXT),('PROPOSED — review only',x+18,y+269,17,GOLD)]
    labels += [('Direction cues: the face leads, the scarf trails. The proposed back view has no face.',64,1279,22,TEXT),
               ('Current run loop: idle → stride A → stride B. Jump currently uses one airborne pose.',64,1318,22,MUTED),
               ('Complete sprites, enlarged 5× with nearest-neighbor pixels. Alternate angles remain proposals.',64,1372,20,MUTED)]
    c.save('contact-unlabelled')
    draw_text(RAW/'contact-unlabelled.png',OUT/'mote-turnaround-review.png',labels)
    # A current-only sheet is also useful for inspecting the animation independently.
    c.h=825; c.pixels=c.pixels[:825]; c.save('current-unlabelled')
    draw_text(RAW/'current-unlabelled.png',OUT/'mote-current-profiles.png',labels[:19])


def animation(current):
    labels=[('MOTE / CURRENT IN-GAME ANIMATION',32,24,30,TEXT),
            ('Exact game poses • nearest-neighbor zoom • no new angles installed',32,68,20,MUTED)]
    names=['Idle','Run · 3 poses','Jump · 1 pose']
    for row,direction in enumerate(['RIGHT','LEFT']):
        labels.append((direction,32,116+row*305,20,MINT))
        for col,name in enumerate(names): labels.append((name,105+col*305,119+row*305,22,TEXT))
    labels += [('The run and jump sequences use the existing game art, without tweening.',32,752,18,MUTED)]
    for frame in range(12):
        c=Canvas(1000,800); c.rect(0,0,1000,800,'#142b36')
        for row in range(2):
            for col in range(3):
                x=81+col*305; y=153+row*305
                c.rect(x,y,260,252,'#304954')
                pose=0 if col==0 else [0,1,2][frame%3] if col==1 else 3
                blit(c,current[row*4+pose],x+34,y+6,6)
        name=f'anim-raw-{frame:02d}'; c.save(name)
        draw_text(RAW/(name+'.png'),RAW/f'anim-{frame:02d}.png',labels)
    subprocess.run(['ffmpeg','-v','error','-y','-framerate','8','-i',str(RAW/'anim-%02d.png'),'-filter_complex','split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=none','-loop','0',str(OUT/'mote-current-animation.gif')],check=True)


def eye_comparison(current):
    archive = OUT / 'before-eyes'
    if not (archive / 'mote0.png').exists(): return
    old_right = load_png(archive / 'mote0.png')
    old_left = load_png(archive / 'mote_left0.png')
    c = Canvas(1200, 930); c.rect(0, 0, c.w, c.h, '#142b36')
    labels = [('MOTE / EYE REFINEMENT',48,30,36,TEXT),
              ('Same profile and silhouette. Smaller, warmer pupils sit inside the face.',48,85,22,MUTED)]
    for col, (pose, title, note, color) in enumerate([
        (old_right, 'BEFORE', 'Large edge block + bright white corner', GOLD),
        (current[0], 'AFTER / updated runtime art', 'Small warm-brown pupil, inset from hood', MINT)]):
        x = 48 + col * 564; y = 137
        card(c, x, y, 540, 436)
        head = Canvas(26, 23)
        for sy in range(head.h):
            for sx in range(head.w): head.pixels[sy][sx] = pose.pixels[sy+8][sx+5]
        blit(c, head, x+114, y+75, 12)
        blit(c, pose, x+476, y+30, 1)
        labels += [(title,x+20,y+18,25,color),(note,x+20,y+391,20,MUTED)]
        labels += [('1× actual',x+449,y+77,16,MUTED)]
    labels += [('Both travel directions / complete runtime sprites',48,612,24,TEXT)]
    for col, pair in enumerate([(old_left,old_right),(current[4],current[0])]):
        x = 48 + col*564
        c.rect(x,652,540,173,'#233944')
        blit(c,pair[0],x+104,658,4); blit(c,pair[1],x+308,658,4)
    labels += [('Face, hood, scarf and animation silhouettes are unchanged.',48,850,22,TEXT),
               ('Eight runtime frames updated. The alternate angles are still proposals.',48,890,20,MUTED)]
    c.save('eyes-before-after-unlabelled')
    draw_text(RAW/'eyes-before-after-unlabelled.png',OUT/'mote-eyes-before-after.png',labels)


def html_preview():
    current={name:'raw/'+name for name in [f'mote{side}{n}.png' for side in ('','_left') for n in range(4)]}
    html='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Mote — sprite review</title>
<style>body{margin:0;background:#142b36;color:#eef3e6;font:18px system-ui,sans-serif}main{max-width:1160px;margin:auto;padding:32px}h1{font-size:32px}p{line-height:1.6;color:#b6c8ce}button{font:inherit;margin:6px;padding:10px 18px;border:1px solid #6c9197;background:#233944;color:#eef3e6;border-radius:6px;cursor:pointer}button[aria-pressed=true]{border-color:#f5bc65;color:#f5bc65}.stage{display:grid;grid-template-columns:repeat(2,1fr);gap:20px}.view{background:#304954;text-align:center;padding:24px}.sprite{image-rendering:pixelated;width:192px;height:240px;object-fit:contain}.concepts{display:grid;grid-template-columns:repeat(4,1fr);gap:16px}.concepts figure{margin:0;background:#233944;text-align:center;padding:18px 0}.concepts img{image-rendering:pixelated;width:128px;height:160px}.sheet{max-width:100%;height:auto}.proposed{color:#f5bc65}@media(max-width:700px){.concepts{grid-template-columns:repeat(2,1fr)}.sprite{width:128px;height:160px}}</style>
<main><h1>Mote / actual animation & proposed angles</h1><p>The animation below uses the current in-game sprites, including the smaller inset eyes. Select a pose to inspect both travel directions. The turnaround concepts below are proposals and have not been installed in the game.</p>
<div id="controls"><button data-pose="run" aria-pressed="true">Run loop</button><button data-pose="idle">Idle</button><button data-pose="jump">Jump pose</button><button id="pause">Pause animation</button></div><div class="stage"><div class="view"><h2>Facing left</h2><img id="left" class="sprite" alt="Mote facing left"></div><div class="view"><h2>Facing right</h2><img id="right" class="sprite" alt="Mote facing right"></div></div>
<h2 class="proposed">Proposed turnaround views / review only</h2><div class="concepts">'''
    for file,label in [('proposed-front','Front'),('proposed-back','Back'),('proposed-three-quarter-left','Three-quarter left'),('proposed-three-quarter-right','Three-quarter right')]:
        html+=f'<figure><img src="raw/{file}.png" alt="Proposed Mote {label.lower()}"><figcaption>{label}</figcaption></figure>'
    html+='''</div><p>Direction cues: the lantern face leads, the saffron scarf trails, and the back view has no eyes. The current jump has one airborne pose.</p><h2>Full comparison sheet</h2><img class="sheet" src="mote-turnaround-review.png" alt="Mote current profiles and proposed turnaround contact sheet"></main>
<script>let pose='run',playing=true,frame=0;function draw(){let n=pose==='idle'?0:pose==='jump'?3:frame%3;document.querySelector('#left').src='raw/mote_left'+n+'.png';document.querySelector('#right').src='raw/mote'+n+'.png'}document.querySelectorAll('[data-pose]').forEach(b=>b.onclick=()=>{pose=b.dataset.pose;document.querySelectorAll('[data-pose]').forEach(x=>x.setAttribute('aria-pressed',x===b));draw()});document.querySelector('#pause').onclick=e=>{playing=!playing;e.target.textContent=playing?'Pause animation':'Play animation'};setInterval(()=>{if(playing){frame++;draw()}},125);draw();</script></html>'''
    (OUT/'index.html').write_text(html)


def bundle():
    (OUT/'OPEN-ME.txt').write_text('Mote character review\n\nOpen index.html in a browser for the current left/right run, idle and jump poses.\nRuntime profiles now have smaller inset brown pupils.\nFront, back and three-quarter angles are proposals, not installed views.\n\nmote-eyes-before-after.png shows the eye refinement at actual size and zoomed.\nmote-turnaround-review.png is the complete comparison sheet.\nmote-current-animation.gif is the current animation preview.\n')
    names = ['index.html','OPEN-ME.txt','mote-current-profiles.png','mote-turnaround-review.png','mote-current-animation.gif','manifest.json','mote-eyes-before-after.png']
    files = [OUT/name for name in names if (OUT/name).exists()]
    files += list(RAW.glob('mote*.png')) + list(RAW.glob('proposed*.png'))
    with zipfile.ZipFile(OUT/'mote-review.zip','w',zipfile.ZIP_DEFLATED) as archive:
        for p in files: archive.write(p,p.relative_to(OUT))


paths=[ROOT/'assets'/'sprites'/f'mote{side}{n}.png' for side in ('','_left') for n in range(4)]
before={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
current=[]
for p in paths:
    (RAW/p.name).write_bytes(p.read_bytes()); current.append(load_png(p))
proposed=concepts()
contact(current,proposed)
eye_comparison(current)
print('Current profiles and turnaround PNGs ready.',flush=True)
html_preview()
animation(current)
after={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
assert before==after, 'Shipped sprites changed during review generation.'
original_manifest=OUT/'before-eyes'/'manifest.json'
original=json.loads(original_manifest.read_text())['current_asset_sha256'] if original_manifest.exists() else before
changed=[name for name,digest in before.items() if original.get(name)!=digest]
(OUT/'manifest.json').write_text(json.dumps({'review_generation_does_not_modify_runtime_assets':True,'runtime_eye_refinement_applied':bool(changed),'runtime_changed_assets':changed,'current_asset_sha256':before,'concepts_installed':False,'zoom':'nearest neighbor','outputs':['mote-current-profiles.png','mote-turnaround-review.png','mote-current-animation.gif','index.html','mote-eyes-before-after.png']},indent=2)+'\n')
bundle()
print('Animation GIF and playable HTML ready. Review generation did not modify runtime assets.',flush=True)
