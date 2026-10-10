#!/usr/bin/env python3
"""Real native captures and audio submissions, from an isolated copy with packaged assets.

Run under xvfb-run on Linux. Never modifies the original package or player storage.
Evidence is preserved on failure; it does not certify physical listening or controllers.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary',type=Path)
    p.add_argument('output',type=Path)
    p.add_argument('--assets',type=Path,help='development assets; omit to require assets beside the packaged binary')
    args=p.parse_args()
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=False)
    package=output/'isolated';package.mkdir()
    binary=package/args.binary.name;shutil.copy2(args.binary,binary)
    assets=args.assets or args.binary.resolve().parent/'assets'
    shutil.copytree(assets,package/'assets')
    # No original game files or source fallback; saves/settings live in this run's private directory.
    env={**os.environ,'BLUEENGINE_PACKAGE_ROOT':str(package),'BLUEENGINE_DATA_DIR':str(output/'storage'), 'LIBGL_ALWAYS_SOFTWARE':'1'}
    if sys.platform=='linux':
        alsa=output/'null-alsa.conf';alsa.write_text('pcm.!default { type null }\n')
        env['ALSA_CONFIG_PATH']=str(alsa)
    def run(label,style,script,frames='1,4,11,15,17,19,23,25,37,39',size='960x540',mute=False):
        target=output/label
        command=[str(binary),'--identity',style,'--capture',str(target),'--frames',frames,'--exit-after',str(max(map(int,frames.split(',')))+1),
                 '--script',script,'--size',size,'--mute' if mute else '--audible']
        done=subprocess.run(command,cwd=package,env=env,text=True,capture_output=True,timeout=120)
        (output/(label+'.log')).write_text(done.stdout+done.stderr)
        rows=[json.loads(line) for line in done.stdout.splitlines() if line.startswith('{')]
        return done,{r['frame']:r for r in rows if 'frame' in r}
    hashes=[]
    for style,start,a,b in [('notebook',(100,205),(455,200),(455,325)),('instrument',(100,365),(142,203),(492,203)),('arcade',(450,130),(425,140),(425,305))]:
        click=lambda point,frame:f'click:{point[0]}/{point[1]}@{frame}'
        script=','.join([click(start,3),click(a,10),'save@12','pause@14',click((20,20),16),click(start,18),click(b,22),'restart@25',
                         click(a,27),click(a,30),click(a,33),click(a,36),'load@39'])
        done,rows=run(style,style,script)
        assert done.returncode==0,done.stderr
        assert rows[1]['screen']=='start' and rows[4]['screen']=='playing'
        assert rows[15]['screen']=='paused' and rows[17]['hash']==rows[15]['hash'],'paused click changed authority'
        assert rows[19]['screen']=='playing' and rows[23]['screen']=='won'
        assert rows[25]['tick']==0,'restart frame must consume gameplay input'
        assert rows[37]['screen']=='lost' and rows[39]['hash']==rows[11]['hash'],'shared load did not resume saved authority'
        submitted=rows[23]['audio']['submitted']
        assert submitted.get('confirm:identity/gesture')==2 and submitted.get('objective:identity/resolve')==1,submitted
        assert rows[23]['audio']['banks']['identity']['loaded_effects']==5
        assert not rows[23]['audio']['banks']['identity']['load_failures']
        hashes.append([rows[n]['hash'] for n in sorted(rows)])
        focus_script=','.join([click(start,3),click(a,10),'focus:0@14-17',click(b,16),click(start,20)])
        done,focus=run(style+'-focus',style,focus_script,frames='13,15,18,19,21')
        assert done.returncode==0 and focus[15]['screen']=='paused' and focus[18]['screen']=='paused',done.stderr
        assert focus[13]['hash']==focus[15]['hash']==focus[19]['hash'],'unfocused input changed authority'
        assert focus[21]['screen']=='playing'

        done,small=run(style+'-small',style,script,size='640x360')
        assert done.returncode==0 and [small[n]['hash'] for n in sorted(small)]==hashes[-1],done.stderr
        done,tall=run(style+'-tall',style,script,size='800x800')
        assert done.returncode==0 and [tall[n]['hash'] for n in sorted(tall)]==hashes[-1],done.stderr
    assert hashes[0]==hashes[1]==hashes[2],'presentation changed the same rule/lifecycle route'
    # Negative checks use only the isolated copy. Failed bank assets never become fallbacks.
    effect=package/'assets/audio/notebook/sfx-tick-0.wav';original=effect.read_bytes();effect.unlink()
    done,_=run('missing-effect','notebook','start@2',frames='4')
    assert done.returncode!=0 and 'sfx-tick-0.wav' in done.stderr,done.stderr
    done,rows=run('muted-missing-effect','notebook','start@2',frames='4',mute=True)
    assert done.returncode==0 and rows[4]['audio']['muted'] and not rows[4]['audio']['submitted']
    effect.write_bytes(original)
    bank=package/'assets/audio/notebook/bank.json';original=bank.read_bytes();data=json.loads(original)
    data['effects']['typo']=data['effects'].pop('tick');bank.write_text(json.dumps(data))
    done,_=run('missing-binding','notebook','start@2',frames='4')
    assert done.returncode!=0 and 'missing effect identity/tick' in done.stderr,done.stderr
    bank.write_bytes(original)
    font=package/'assets/fonts/LiberationSerif-Italic.ttf';original=font.read_bytes();font.write_bytes(b'invalid font')
    done,_=run('invalid-font','notebook','start@2',frames='4',mute=True)
    assert done.returncode!=0 and 'Font face' in done.stderr,done.stderr
    font.unlink()
    done,_=run('missing-font','notebook','start@2',frames='4',mute=True)
    assert done.returncode!=0 and 'Font face' in done.stderr,done.stderr
    font.write_bytes(original)
    evidence={'ok':True,'presentations':3,'sizes':['960x540','640x360','800x800'],'same_authority_hashes':True,
              'shared_click_start_pause_resume_save_load_restart_win_loss':True,'named_audio_submitted':True,
              'missing_effect_and_binding_fail':True,'mute_bypass':True,'missing_invalid_fonts_fail':True,
              'physical_audio_controller_verified':False}
    (output/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n');print(json.dumps(evidence))

if __name__=='__main__':main()
