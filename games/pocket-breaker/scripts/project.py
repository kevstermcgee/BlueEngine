"""Canonical dimensionality/target requirements; copied into standalone game tooling."""
import json,re
from pathlib import Path
class ProjectError(ValueError): pass
def validate_project(game):
    p=game/'game.project.json'
    if not p.is_file():raise ProjectError('Web target requires game.project.json; create with new-game NAME DIR ENGINE two-d. Existing native games are unchanged.')
    project=json.loads(p.read_text())
    if not isinstance(project,dict):raise ProjectError('game.project.json must contain an object')
    allowed={'schema_version','id','presentation','targets','networking','input','description','session_minutes','complexity'}
    unknown=set(project)-allowed
    if unknown:raise ProjectError(f"Unknown project fields {sorted(unknown)}; correct spelling before building")
    if project.get('schema_version')!=1:raise ProjectError('Unsupported game.project schema_version; expected 1')
    if not isinstance(project.get('id'),str) or not re.fullmatch('[a-z][a-z0-9-]{0,47}',project['id']):raise ProjectError('Game ID must be [a-z][a-z0-9-]{0,47}')
    if project.get('presentation') not in ('2d','3d'):raise ProjectError('presentation must explicitly be 2d or 3d')
    targets=project.get('targets');network=project.get('networking')
    if not isinstance(targets,list) or not targets or any(not isinstance(t,str) for t in targets) or len(set(targets))!=len(targets) or set(targets)-{'web','linux','windows','macos'}:raise ProjectError('targets must be unique entries from web/linux/windows/macos')
    if network not in ('offline','native-multiplayer'):raise ProjectError('networking must be offline or native-multiplayer; browser networking is not yet supported')
    if 'web' in targets and (project['presentation']!='2d' or network!='offline'):raise ProjectError('Web currently supports 2d + offline only. Select native targets for 3d or native-multiplayer; no fallback is performed.')
    if project['presentation']=='2d' and network!='offline':raise ProjectError('The 2D client supports offline only; select offline or use the established 3D/native multiplayer path.')
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
    if project['presentation']=='2d' and project['networking']!='offline': raise ProjectError('The 2D client supports offline only; use the established 3D/native multiplayer path.')
    return project
