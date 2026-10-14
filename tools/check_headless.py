"""Check resolved normal dependency graphs for each supported ownership boundary."""
import subprocess

# windows-sys may supply OS networking through Tokio; it is not itself a renderer.
FORBIDDEN = {'macroquad', 'miniquad', 'quad-snd', 'alsa-sys', 'png', 'ctrlc', 'gilrs', 'gilrs-core'}
HEAVY_RULES = {'rapier3d', 'quinn', 'tokio', 'rustls'}

# Separate Cargo invocations prevent a wide workspace/feature build from hiding
# an accidental edge in a narrow consumer. Lock membership is never the evidence.
CONFIGURATIONS = (
    ('rules', (), FORBIDDEN | HEAVY_RULES),
    ('stock-server', ('physics', 'networking'), FORBIDDEN),
    ('spatial-rules', ('physics',), FORBIDDEN | {'quinn', 'tokio', 'rustls'}),
    ('custom-networking', ('networking',), FORBIDDEN | {'rapier3d'}),
    ('offline-2d', ('two-d',), HEAVY_RULES),
    ('schema-authoring', ('schema-validation',), FORBIDDEN | HEAVY_RULES),
)


def check_boundaries(run=subprocess.run):
    for name, features, forbidden in CONFIGURATIONS:
        flags = ['--features', ','.join(features)] if features else []
        command = ['cargo', 'tree', '--locked', '--no-default-features', *flags,
                   '--edges', 'normal', '--prefix', 'none', '--format', '{p}']
        result = run(command, check=True, text=True, capture_output=True)
        packages = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
        unexpected = sorted(packages & forbidden)
        if unexpected:
            raise SystemExit(f'ARCH-HEADLESS-001: forbidden {name} dependencies: ' + ', '.join(unexpected)
                             + '\nNext: python tools/be2.py context ARCH-HEADLESS-001')
        print(f'PASS ARCH-HEADLESS-001 {name}')

if __name__ == '__main__':
    check_boundaries()
