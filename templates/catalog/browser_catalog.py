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
    return f'<button class="star" data-star="{esc(slug)}" aria-pressed="false" aria-label="Star {esc(title)}"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m12 2.5 2.94 5.96 6.58.96-4.76 4.64 1.12 6.55L12 17.52l-5.88 3.09 1.12-6.55L2.48 9.42l6.58-.96Z"/></svg></button>'
def heading(slug,title):
    return f'<div class="card-heading"><h2><a class="game-title" href="games/{esc(slug)}/">{esc(title)}</a></h2>{star(slug,title)}</div>'

def write_details(out,slug,title,description,presentation,network,web=None,native=None,prefix="web/"):
    directory=out/'games'/slug;directory.mkdir(parents=True,exist_ok=True)
    file=directory/'index.html'
    info=f'<p class="meta">{esc(presentation.upper())} · {"Singleplayer" if network=="offline" else "Multiplayer" if network=="native-multiplayer" else "Networking not specified"}</p>'
    if file.is_file():
        page=file.read_text()
        page=re.sub(r'<!-- browser-details -->.*?<!-- /browser-details -->','',page,flags=re.S)
        if 'href="../../catalog.css"' not in page:
            page=page.replace('</head>','<link rel="stylesheet" href="../../catalog.css"></head>',1)
    else:
        thumbnail='../../'+prefix+web['thumbnail'] if web else ''
        image=f'<img class="thumb" src="{esc(thumbnail)}" alt="">' if thumbnail else ''
        download=f'<a class="dl" href="{esc(native)}">Download native</a>' if native else ''
        page=f'<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{esc(title)} · BlueEngine Games</title><link rel="stylesheet" href="../../style.css"><link rel="stylesheet" href="../../catalog.css"></head><body><header><a href="../../">← All games</a><h1>{esc(title)}</h1></header><main><section class="game-summary">{image}<div><p>{esc(description)}</p>{info}{download}</div></section></main></body></html>'
    if web:
        controls=' + '.join(web.get('input',[]))
        actions=f'<!-- browser-details --><section class="browser-details"><h2>Play in your browser</h2>{info}<p>{esc(web["description"])}</p><p>Controls: {esc(controls)}. Mobile controls appear below the game. Press F for fullscreen on desktop.</p><a class="dl play" href="../../{esc(prefix+web["play"])}">Play in browser</a><p>Install from the browser to play offline after the first complete load. Progress stays on this device and browser; it is independent of native saves.</p></section><!-- /browser-details -->'
        # Keep both ways to play above the native release-history table.
        page=page.replace('</section>','</section>'+actions,1) if '</section>' in page else page.replace('</main>',actions+'</main>',1)
    file.write_text(page)
def card(game,prefix="web/"):
    slug=game['id'];title=game['title'];description=game['description'];presentation=game['presentation'];network=game['networking']
    created=datetime.datetime.fromtimestamp(game['built_at_epoch'],datetime.timezone.utc).date().isoformat()
    play=prefix+game['play'];thumb=prefix+game['thumbnail']
    native=game.get('native_download')
    downloads=f'<a class="dl" href="{esc(native)}">Download</a>' if native else ''
    body=f'''<article class="card" data-name="{esc(title.lower())}" data-created="{created}" data-kind="native" data-size="0" data-text="{esc((title+' '+description).lower())}"{attributes(slug,presentation,network,True,bool(native))}>
<img class="thumb" src="{esc(thumb)}" alt="" loading="lazy"><div class="body">{heading(slug,title)}<p class="desc">{esc(description)}</p><p class="meta">{presentation.upper()} · {'Singleplayer' if network=='offline' else 'Multiplayer'}</p><a class="dl play" href="{esc(play)}">Play / Install</a>{downloads}</div></article>'''
    return {'card':body,'created':created,'name':title}
def merge_games(games,rows,web_root,out,games_dir):
    # The native builder copies its own CSS; the shared catalog additions must ship too.
    out.mkdir(parents=True,exist_ok=True)
    shutil.copy2(Path(__file__).with_name('catalog.css'),out/'catalog.css')
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
        body=re.sub(r'<h2>(.*?)</h2>',lambda m: f'<div class="card-heading"><h2>{m.group(1)}</h2>{star(slug,row["name"])}</div>',body,count=1,flags=re.S)
        write_details(out,slug,row['name'],row.get('description',''),presentation,network,web)
        body=body.replace('<p class="meta">',f'<p class="meta">{esc(presentation.upper())} · ',1)
        if web:body=body.replace('<a class="dl"',f'<a class="dl play" href="web/{esc(web["play"])}">Play / Install</a><a class="dl"',1)
        result.append(game|{'card':body})
    for game in browser.values():
        result.append(card(game))
        write_details(out,game['id'],game['title'],game['description'],game['presentation'],game['networking'],game,game.get('native_download'))
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
