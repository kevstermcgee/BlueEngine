"""Prepare Leo's reproducible original audio before packaging."""
from render_audio import ensure_audio

def prepare_package(project):
    ensure_audio(project.root)
