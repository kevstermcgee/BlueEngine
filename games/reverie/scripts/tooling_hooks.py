"""Game-owned rendering of Reverie's eight authored musical movements."""
import sys
from ship import ConfigError, ShipError, run_process, log
from be2_game_tooling import fresh_tools

def prepare_package(project):
    """A clean source checkout renders the authored soundtrack before packaging."""
    movements = ('wonder', 'longing', 'grief', 'joy', 'unease', 'anger', 'tenderness', 'acceptance')
    if all((project.assets / 'audio' / name / 'bank.json').is_file() for name in movements):
        return
    engine = project.engine()
    if engine is None:
        raise ConfigError('Reverie soundtrack rendering requires its declared BlueEngine checkout')
    tool = fresh_tools(engine)
    render = run_process([sys.executable, project.root / 'scripts' / 'compose.py', tool], cwd=project.root)
    if render.returncode:
        log(render.stdout + render.stderr)
        raise ShipError('could not render the complete Reverie soundtrack')
    log('prepared eight original musical movements from their authored scores')

