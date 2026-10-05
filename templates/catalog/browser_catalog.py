"""Merge verified browser artifacts into the existing native game feed; no engine checkout needed."""
import datetime
import html
import json
import re
import shutil
from pathlib import Path

def esc(value):return html.escape(str(value),quote=True)
def attributes(slug,presentation,networking,browser,native):
    return f' data-id="{esc(slug)}" data-presentation="{esc(presentation)}" data-networking="{esc(networking)}" data-browser="{str(browser).lower()}" data-native="{str(native).lower()}"'
def star(slug,title):
    return f'<button class="star" data-star="{esc(slug)}" aria-pressed="false" aria-label="Star {esc(title)}">☆</button>'
def card(game,prefix="web/"):
    slug=game['id'];title=game['title'];description=game['description'];presentation=game['presentation'];network=game['networking']
    created=datetime.datetime.fromtimestamp(game['built_at_epoch'],datetime.timezone.utc).date().isoformat()
    play=prefix+game['play'];thumb=prefix+game['thumbnail']
    native=game.get('native_download')
    downloads=f'<a class="dl" href="{esc(native)}">Download</a>' if native else ''
    body=f'''<article class="card" data-name="{esc(title.lower())}" data-created="{created}" data-kind="native" data-size="0" data-text="{esc((title+' '+description).lower())}"{attributes(slug,presentation,network,True,bool(native))}>
<img class="thumb" src="{esc(thumb)}" alt="" loading="lazy"><div class="body"><h2>{esc(title)} {star(slug,title)}</h2><p class="desc">{esc(description)}</p><p class="meta">{presentation.upper()} · {'Singleplayer' if network=='offline' else 'Multiplayer'}</p><a class="dl play" href="{esc(play)}">Play / Install</a>{downloads}</div></article>'''
    return {'card':body,'created':created,'name':title}
def merge_games(games,rows,web_root,out,games_dir):
    browser={}
    if (web_root/'catalog.json').is_file():
        catalog=json.loads((web_root/'catalog.json').read_text())
        browser={g['id']:g for g in catalog['games']}
        shutil.copytree(web_root,out/'web',dirs_exist_ok=True)
    result=[]
    for row,game in zip(rows,games):
        slug=row['slug'];web=browser.pop(slug,None)
        project_path=Path(games_dir)/slug/'game.project.json'
        project=json.loads(project_path.read_text()) if project_path.is_file() else {}
        presentation=(web or project).get('presentation','3d')
        network=(web or project).get('networking','native-multiplayer' if row.get('online_args_b64') else 'unknown')
        body=game['card'].replace('<article ', '<article '+attributes(slug,presentation,network,bool(web),True)+' ',1)
        body=body.replace('</h2>',star(slug,row['name'])+'</h2>',1)
        body=body.replace('<p class="meta">',f'<p class="meta">{esc(presentation.upper())} · ',1)
        if web:body=body.replace('<a class="dl"',f'<a class="dl play" href="web/{esc(web["play"])}">Play / Install</a><a class="dl"',1)
        result.append(game|{'card':body})
    result.extend(card(g) for g in browser.values())
    return result

def enhance_page(page):
    page=re.sub(r'<p class="tagline">.*?</p>','<p class="tagline">Play in your browser or install locally. Star favorites to keep them first on this device.</p>',page,flags=re.S)
    page=page.replace('</head>','<link rel="stylesheet" href="catalog.css"></head>',1) if '</head>' in page else page.replace('<header>','<link rel="stylesheet" href="catalog.css"><header>',1)
    page=page.replace('<li>Windows x64 only.</li>','<li>Browser games support desktop and mobile controls, and offline installation. Downloads list their supported platform.</li>')
    filters='''<select id="presentation" aria-label="Presentation"><option value="">2D + 3D + Hybrid</option><option value="2d">2D</option><option value="3d">3D</option><option value="hybrid">Hybrid</option></select>
<select id="distribution" aria-label="Distribution"><option value="">All ways to play</option><option value="browser">Play in browser</option><option value="native">Download</option></select>
<select id="networking" aria-label="Networking"><option value="">All networking</option><option value="offline">Singleplayer</option><option value="native-multiplayer">Multiplayer</option></select>'''
    page=page.replace('<div class="kinds"',filters+'<div class="kinds"',1)
    page=page.replace('<main id="grid">','<p id="favorites-status" role="status"></p><main id="grid">',1)
    return page
