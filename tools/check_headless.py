"""Fail if graphics/window/audio packages enter the rendering-free dependency graph."""
import subprocess

# windows-sys may supply OS networking through Tokio; it is not itself a renderer.
FORBIDDEN = {'macroquad', 'miniquad', 'quad-snd', 'alsa-sys', 'png', 'ctrlc'}

if __name__ == '__main__':
    result = subprocess.run(
        ['cargo', 'tree', '--locked', '--no-default-features', '--edges', 'normal',
         '--prefix', 'none', '--format', '{p}'],
        check=True, text=True, capture_output=True)
    packages = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
    unexpected = sorted(packages & FORBIDDEN)
    if unexpected:
        raise SystemExit('Headless dependency regression: ' + ', '.join(unexpected))
    print('Headless dependency boundary passed')
