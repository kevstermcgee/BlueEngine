#!/usr/bin/env python3
"""Rebuild the sandbox's deterministic, source-derived content. No third-party packages."""
import copy
import argparse
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
import assets as library

OUT = ROOT / 'assets/games/blueengine-sandbox'
BASE = 'assets/games/blueengine-sandbox/'
CHECK = False


def save(path, data):
    serialized = json.dumps(data, indent=2) + '\n'
    if CHECK:
        if not path.is_file() or path.read_text(encoding='utf-8') != serialized:
            raise ValueError(f'Sandbox content is stale: {path}. Run scripts/build_sandbox_content.py')
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(serialized, encoding='utf-8')


def scene():
    s = copy.deepcopy(library.read(ROOT / 'assets/props/be2-chair.json'))
    s['nodes'] = []
    s['materials'] = {}
    s['audio'] = None
    s['world'].update(sky=[0.36, 0.55, 0.73], ambient=0.65, fog=0)
    return s


def bounds(center, half):
    return {'min': [round(c-h, 5) for c,h in zip(center,half)],
            'max': [round(c+h, 5) for c,h in zip(center,half)]}


def node(s, name, pos, half, color, shape='box'):
    s['materials'][name] = {'color': color, 'roughness': 0.85, 'metallic': 0, 'emission': 0}
    s['nodes'].append({'id': name, 'shape': shape, 'material': name,
                       'pos': pos, 'rot': [0,0,0], 'scale': half})


class Map:
    def __init__(self, name, half=(20,20), spawn=(0,0,16)):
        self.doc = {'schema_version': 1, 'name': name, 'scene': scene(),
                    'colliders': {}, 'entities': [],
                    'default_spawn': {'feet': list(spawn), 'yaw': 0}}
        self.signs = []
        self.box('floor', [0,-0.15,0], [half[0],0.15,half[1]], [0.34,0.40,0.44])
        for name, pos, ext in [('north',[0,0.6,-half[1]],[half[0],0.6,0.18]),
                               ('south',[0,0.6,half[1]],[half[0],0.6,0.18]),
                               ('east',[half[0],0.6,0],[0.18,0.6,half[1]]),
                               ('west',[-half[0],0.6,0],[0.18,0.6,half[1]])]:
            self.box('edge-'+name,pos,ext,[0.17,0.24,0.30])

    def box(self, name, pos, half, color, collide=True):
        node(self.doc['scene'], name, pos, half, color)
        if collide:
            self.doc['colliders'][name] = bounds(pos, half)

    def sign(self, text, pos, width=4):
        self.box('sign-'+str(len(self.signs)), [pos[0]+width/2,pos[1]+0.15,pos[2]-0.04],
                 [width/2,0.22,0.035], [0.87,0.92,0.91], False)
        self.signs.append({'text': text, 'position': pos, 'height': min(0.28, width/max(len(text),1)*1.6)})

    def place(self, asset, name, at, yaw=0):
        s = library.read(ROOT / asset['source'])
        rad = math.radians(yaw)
        c,t = math.cos(rad),math.sin(rad)
        remap = {}
        for key in sorted({n['material'] for n in s['nodes']}):
            mat = s['materials'][key]
            dest = key
            if dest in self.doc['scene']['materials'] and self.doc['scene']['materials'][dest] != mat:
                dest = key+'/'+asset['id'].replace('/','-')
            self.doc['scene']['materials'][dest] = mat
            remap[key] = dest
        for i,n in enumerate(s['nodes']):
            n = copy.deepcopy(n)
            x,y,z = n['pos']
            n['pos'] = [at[0]+c*x+t*z,at[1]+y,at[2]-t*x+c*z]
            n['rot'][1] += yaw
            n['id'] = name+'/'+str(i)
            n['material'] = remap[n['material']]
            self.doc['scene']['nodes'].append(n)
        x,y,z = asset['half']
        b = bounds([at[0],at[1]+y,at[2]], [abs(c)*x+abs(t)*z,y,abs(t)*x+abs(c)*z])
        # Component collision for new structural kits preserves openings and legs.
        if asset['group'] == 'Sandbox kit':
            for n in self.doc['scene']['nodes'][-len(s['nodes']):]:
                if n['shape'] == 'box' and yaw % 180 == 0:
                    self.doc['colliders'][n['id']] = bounds(n['pos'], n['scale'])
                elif n['shape'] == 'box' and yaw % 90 == 0:
                    h=n['scale']; self.doc['colliders'][n['id']] = bounds(n['pos'],[h[2],h[1],h[0]])
                else:
                    self.doc['colliders'][n['id']] = bounds(n['pos'],n['scale'])
        else:
            self.doc['colliders'][name] = b
        self.doc['entities'].append({'id':name,'label':asset['name'],'bounds':b,'action':'inspect'})


def main():
    _, _, records = library.load_catalog()
    assets = [{'id':r['id'],'name':r['label'],'description':r['description'],
               'group':'Native props' if r['pack']=='core/native' else 'Interior prefabs',
               'source':r['source']['path'], 'half':r['geometry']['half_extents'],
               'tags':' '.join(r['taxonomy']['tags'])} for r in records]
    blue=[0.08,0.35,0.58]; wood=[0.51,0.31,0.15]; dark=[0.11,0.16,0.20]
    kits = [
      ('shipping-crate','Shipping crate',[0.6,0.6,0.6],[( [0,0.6,0],[0.58,0.58,0.58],wood,'box')]+
       [([x,0.6,z],[0.045,0.6,0.045],dark,'box') for x in [-0.555,0.555] for z in [-0.555,0.555]]),
      ('pallet','Timber pallet',[0.7,0.09,0.55], [([0,0.13,z],[0.7,0.05,0.075],wood,'box') for z in [-0.475,-0.2375,0,0.2375,0.475]]+
       [([x,0.04,0],[0.08,0.04,0.55],dark,'box') for x in [-0.6,0,0.6]]),
      ('bollard','Safety bollard',[0.16,0.6,0.16],[([0,0.5,0],[0.10,0.5,0.10],[0.94,0.60,0.12],'box'),([0,1.05,0],[0.10,0.05,0.10],dark,'box'),([0,0.04,0],[0.16,0.04,0.16],dark,'box')]),
      ('planter','Courtyard planter',[0.7,0.55,0.7],[([0,0.30,0],[0.7,0.30,0.7],[0.68,0.64,0.53],'box'),([0,0.78,0],[0.58,0.30,0.58],[0.19,0.40,0.24],'sphere')]),
      ('pine','Low-poly pine',[1.0,2.0,1.0],[([0,0.65,0],[0.15,0.65,0.15],wood,'box'),([0,1.5,0],[1,1,1],[0.10,0.31,0.25],'cone'),([0,2.5,0],[0.8,0.9,0.8],[0.15,0.42,0.30],'cone'),([0,3.35,0],[0.5,0.65,0.5],[0.22,0.49,0.33],'cone')]),
      ('gantry','Modular gantry',[2.2,1.7,0.22],[([x,1.5,0],[0.18,1.5,0.22],blue,'box') for x in [-2,2]]+[([0,3.2,0],[2.2,0.2,0.22],blue,'box')]),
      ('bench','Courtyard bench',[1.15,0.48,0.35],[([0,0.47,0],[1.15,0.07,0.35],wood,'box'),([0,0.80,-0.30],[1.15,0.16,0.05],wood,'box')]+[([x,0.2,0],[0.07,0.2,0.27],dark,'box') for x in [-0.95,0.95]]),
      ('work-light','Work light',[0.35,1.4,0.35],[([0,0.06,0],[0.35,0.06,0.35],dark,'box'),([0,1.3,0],[0.055,1.25,0.055],dark,'box'),([0,2.65,0],[0.3,0.15,0.12],[0.96,0.76,0.27],'box'),([0,2.65,0.125],[0.24,0.10,0.01],[0.95,0.94,0.75],'box')]),
    ]
    pack=[]
    for slug,label,half,parts in kits:
        s=scene()
        for i,(pos,ext,col,shape) in enumerate(parts): node(s,slug+'-'+str(i),pos,ext,col,shape)
        save(OUT/'kit'/f'{slug}.json',s)
        assets.append({'id':'sandbox/'+slug,'name':label,'description':'Reusable sandbox '+label.lower()+'. MIT; generated primitive geometry.',
                       'group':'Sandbox kit','source':BASE+'kit/'+slug+'.json','half':half,'tags':'sandbox outdoor industrial modular '+slug})
        pack.append({'id':slug,'label':label,'description':assets[-1]['description'],'type':'prefab',
                     'source':{'path':'kit/'+slug+'.json','format':'vesper-scene-v1','method':'generated'},
                     'taxonomy':{'categories':['environment'],'tags':['sandbox',slug],'aliases':[]},
                     'geometry':{'units':'meters','origin':'bottom-center','half_extents':half},
                     'physics':{'mobility':'static','collision':'compound'},'lifecycle':{'status':'experimental'}})
    save(OUT/'assets.json',{'schema_version':1,'pack':{'id':'sandbox','name':'BlueEngineSandbox kit','version':'1.0.0','scope':'game-local','license':'MIT'},'assets':pack})
    byid={a['id'].split('/')[-1]:a for a in assets}
    for a in assets:
        slug=a['id'].replace('/','-')
        m=Map(a['name'],(8,8),(0,0,5))
        m.place(a,'specimen',[0,0,0])
        # 1 m measurement lines are visual only; they never trip a player.
        for i in range(-7,8):
            m.box('grid-x'+str(i),[i,0.006,0],[0.009,0.005,7],[0.48,0.57,0.62],False)
            m.box('grid-z'+str(i),[0,0.006,i],[7,0.005,0.009],[0.48,0.57,0.62],False)
        a['map']=BASE+'previews/'+slug+'.json'
        save(ROOT/a['map'],m.doc)
    maps=[]
    def finish(m, slug, desc, route):
        save(OUT/'maps'/f'{slug}.json',m.doc)
        save(OUT/'routes'/f'{slug}.json',[{'x':x,'z':z,'feet':0} for x,z in route])
        maps.append({'id':slug,'name':m.doc['name'],'description':desc,'group':'Sandbox maps',
                     'path':BASE+'maps/'+slug+'.json','signs':m.signs})

    m=Map('Asset Atrium',(22,39),(0,0,36))
    m.box('spine',[0,0.008,0],[1.4,0.008,37],blue,False)
    for i,a in enumerate(assets):
        row,col=divmod(i,6); x=[-17,-11,-5,5,11,17][col]; z=30-row*5
        raised = a['half'][1] < 0.55 and max(a['half'][0],a['half'][2]) < 0.85
        if raised:
            m.box('plinth-'+str(i),[x,0.36,z],[1.0,0.36,0.9],[0.19,0.28,0.34])
        m.place(a,'exhibit-'+a['id'].replace('/','-'),[x,0.72 if raised else 0,z])
        m.sign(a['name'],[x-1.5,0.25,z+1.5],3)
    for z in [-35,0,34]:
        m.place(byid['gantry'],'gateway-'+str(z),[0,0,z])
    m.sign('BLUEENGINE / ASSET ATRIUM',[-5,3.7,-36],10)
    finish(m,'atrium','A walkable collection of every registered prop and prefab. Central blue aisle; E inspects an exhibit.',[(0,30),(0,0),(0,-34),(20,-34),(20,34)])

    m=Map('Calibration Range',(20,22),(0,0,18))
    for x in range(-18,19,2): m.box('line-'+str(x),[x,0.005,0],[0.012,0.005,20],[0.63,0.69,0.68],False)
    for z in range(-20,21,2): m.box('cross-'+str(z),[0,0.005,z],[18,0.005,0.012],[0.63,0.69,0.68],False)
    for i in range(8):
        h=(i+1)*0.16
        m.box('step-'+str(i),[-10,h/2,8-i*0.6],[2,h/2,0.3],[0.21,0.43,0.60])
    m.box('landing',[-10,0.64,2],[2,0.64,1.4],blue)
    for i,h in enumerate([0.42,1.18,1.9]):
        z=6-i*6
        for x in [7,11]: m.box(f'tunnel-{i}-{x}',[x,1.5,z],[0.15,1.5,1.5],dark)
        m.box('lintel-'+str(i),[9,h+0.15,z],[1.85,0.15,1.5],[0.76,0.52,0.22])
        m.sign(f'{h:.2f} m CLEARANCE',[7.2,h+0.28,z+1.6],3.6)
    for i in range(5): m.place(byid['shipping-crate'],'cover-'+str(i),[-14+i*6,0,-14])
    m.sign('MOVEMENT / SCALE / COLLISION',[-5,2.4,-19],10)
    finish(m,'calibration','Metric grid, 16 cm stairs, cover and 0.42 / 1.18 / 1.90 m clearance tunnels for both character profiles.',[(0,10),(0,-10),(16,-10),(16,15),(-16,15),(-16,-10)])

    m=Map('Cedar Courtyard',(22,22),(0,0,18))
    m.box('lawn',[0,0.004,0],[20,0.004,20],[0.25,0.40,0.27],False)
    m.box('path-ns',[0,0.010,0],[2.2,0.005,20],[0.76,0.70,0.58],False)
    m.box('path-ew',[0,0.012,0],[20,0.005,2.2],[0.76,0.70,0.58],False)
    for x in [-13,13]:
        for z in [-13,13]:
            m.place(byid['pine'],f'pine-{x}-{z}',[x,0,z])
            m.place(byid['planter'],f'planter-{x}-{z}',[x*0.5,0,z*0.5])
    for x in [-9,9]:
        for z in [-5,5]: m.place(byid['bench'],f'bench-{x}-{z}',[x,0,z],180 if z<0 else 0)
    for x in [-5,5]: m.place(byid['gantry'],'pergola-'+str(x),[x,0,-16])
    for z in [-17,-16,-15]: m.box('pergola-beam-'+str(z),[0,3.48,z],[7.2,0.08,0.09],wood)
    m.sign('CEDAR / COURTYARD',[-4,2.7,-20],8)
    finish(m,'courtyard','Open outdoor circulation, shaded seating and planted corners. A general-purpose social and visibility test map.',[(0,0),(18,0),(18,-18),(0,-18),(-18,-18),(-18,0),(0,0)])

    m=Map('Foundry Yard',(24,24),(0,0,20))
    for x in [-15,15]:
        m.box('shed-back-'+str(x),[x,2.5,-12],[6,2.5,0.25],[0.24,0.37,0.44])
        for side in [-1,1]: m.box(f'shed-side-{x}-{side}',[x+side*6,2.5,-5],[0.25,2.5,7],[0.24,0.37,0.44])
        m.box('roof-'+str(x),[x,5.1,-5],[6.3,0.15,7.3],dark)
        for i in range(4):
            m.place(byid['pallet'],f'pallet-{x}-{i}',[x-4+i*2.6,0,-9])
            m.place(byid['shipping-crate'],f'crate-{x}-{i}',[x-4+i*2.6,0.18,-9])
        for i in [-4,4]: m.place(byid['work-light'],f'light-{x}-{i}',[x+i,0,-2])
    for z in [-18,-6,6]:
        m.place(byid['gantry'],'gantry-'+str(z),[0,0,z])
    for x in [-7,7]:
        for z in [4,8,12]:m.place(byid['bollard'],f'bollard-{x}-{z}',[x,0,z])
    for z in range(-20,21,4): m.box('lane-'+str(z),[0,0.008,z],[0.10,0.008,0.85],[0.93,0.72,0.25],False)
    m.sign('FOUNDRY / LOADING YARD',[-5,3.6,-21],10)
    finish(m,'foundry','Two open loading sheds, pallets, crates, work lights and a clear central vehicle lane. Test interiors, cover and sightlines.',[(0,14),(0,-16),(-7,-16),(-7,2),(-5,2),(-5,14),(-15,14),(-15,-4),(-15,14),(15,14),(15,-4)])
    for slug,label in [('house','House'),('school-wing','School Wing'),('office','Office'),('convenience-store','Convenience Store')]:
        legacy=library.read(ROOT/'assets/maps/starters'/f'{slug}.json')
        legacy['default_spawn']={'feet':[0,0,4.6],'yaw':-0.1}
        save(OUT/'maps'/f'{slug}.json',legacy)
        maps.append({'id':slug,'name':label,'description':'Original furnished BlueEngine starter, including its expanded grounds.',
                     'group':'Starter maps','path':BASE+'maps/'+slug+'.json','signs':[]})
    maps.append({'id':'test-lab','name':'Blue Test Lab','description':'The native engine test fixture, including its original assets and collision geometry.',
                 'group':'Engine maps','path':'builtin:test-lab','signs':[]})
    save(OUT/'catalog.json',{'version':1,'assets':assets,'maps':maps})
    print(f'Generated {len(assets)} specimens, {len(kits)} reusable kits, {len(maps)} destinations.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Fail on stale generated content without changing files')
    CHECK = parser.parse_args().check
    main()
