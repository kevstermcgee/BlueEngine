"""Canonical dimensionality/target requirements; copied into standalone game tooling."""
import json,re
from pathlib import Path
class ProjectError(ValueError): pass
def validate_project(game):
    p=game/'game.project.json'
    if not p.is_file():raise ProjectError('Project requirements need game.project.json; create with new-game NAME DIR ENGINE two-d. Legacy projects without this metadata keep their native path.')
    project=json.loads(p.read_text())
    if not isinstance(project,dict):raise ProjectError('game.project.json must contain an object')
    allowed={'schema_version','id','presentation','targets','networking','input','description','session_minutes','complexity','mobile_controls','runtime','web_build'}
    unknown=set(project)-allowed
    if unknown:raise ProjectError(f"Unknown project fields {sorted(unknown)}; correct spelling before building")
    if project.get('schema_version')!=1:raise ProjectError('Unsupported game.project schema_version; expected 1')
    if not isinstance(project.get('id'),str) or not re.fullmatch('[a-z][a-z0-9-]{0,47}',project['id']):raise ProjectError('Game ID must be [a-z][a-z0-9-]{0,47}')
    if project.get('presentation') not in ('2d','3d','hybrid'):raise ProjectError('presentation must be 2d, 3d or hybrid; hybrid leaves authors free to mix both')
    runtime=project.get('runtime','portable' if project['presentation']=='2d' else 'legacy-native')
    if runtime not in ('portable','legacy-native'):raise ProjectError('runtime must be portable (native, any presentation) or legacy-native')
    mobile=project.get('mobile_controls',{'layout':'dpad','action_label':'Action'})
    if not isinstance(mobile,dict) or set(mobile)-{'layout','action_label'} or mobile.get('layout') not in ('dpad','paddle','tap'):raise ProjectError('mobile_controls needs layout dpad, paddle or tap; optional action_label names the button')
    if mobile.get('action_label','Action') is not None and (not isinstance(mobile.get('action_label','Action'),str) or not 1<=len(mobile.get('action_label','Action'))<=24):raise ProjectError('mobile_controls.action_label must be null (no button) or 1–24 characters')
    if project.get('web_build') is not None:
        raise ProjectError('Browser gameplay is retired: remove web_build deliberately after selecting a native executable; see docs/BROWSER_WORKFLOW.md')
    targets=project.get('targets');network=project.get('networking')
    if isinstance(targets,list) and any(t in ('web','browser') for t in targets if isinstance(t,str)):
        raise ProjectError('Browser gameplay target is retired: choose native windows/linux/macos explicitly; metadata alone does not port a game. See docs/BROWSER_WORKFLOW.md')
    if not isinstance(targets,list) or not targets or any(not isinstance(t,str) for t in targets) or len(set(targets))!=len(targets) or set(targets)-{'linux','windows','macos'}:raise ProjectError('targets must be unique entries from linux/windows/macos')
    if network not in ('offline','native-multiplayer'):raise ProjectError('networking must be offline or native-multiplayer; native transport requirements remain explicit')
    if runtime=='portable' and network!='offline':raise ProjectError('The portable client supports offline only; use NetGame/ClientView with a custom native presentation for multiplayer.')
    inputs=project.get('input')
    if not isinstance(inputs,list) or not inputs or any(not isinstance(i,str) for i in inputs) or set(inputs)-{'keyboard','mouse','controller'} or len(set(inputs))!=len(inputs):raise ProjectError('input must declare keyboard, mouse and/or controller, without duplicates')
    if not isinstance(project.get('description'),str) or not project['description'].strip():raise ProjectError('Write a nonempty game description')
    if type(project.get('session_minutes')) is not int or not 1<=project['session_minutes']<=60:raise ProjectError('session_minutes must be an integer from 1 to 60')
    if project.get('complexity') not in ('low','medium','high'):raise ProjectError('complexity must be low, medium or high')
    return project

def native_target(game,platform_name):
    if not (Path(game)/'game.project.json').exists(): return None # legacy native projects
    project=validate_project(Path(game))
    if platform_name not in project['targets']: raise ProjectError(f"Target {platform_name} is not declared; targets are {project['targets']}. Edit game.project.json deliberately; no fallback.")
    return project
