"""Compose BlueEngine `audio render` score projects from compact chord/pattern descriptions.

Usage: python3 tools/compose_scores.py OUT_DIR   -> writes prickle-putt.json, puff-pop.json, topsy-turvy.json
Every layer in a score loops at the same length; the client fades layers with GameLogic::audio_level.
"""
import json, sys, pathlib

def inst(wave="sine", attack=0.008, decay=0.08, sustain=0.6, release=0.12, lowpass=None):
    d = {"wave": wave, "attack": attack, "decay": decay, "sustain": sustain, "release": release}
    if lowpass: d["lowpass_hz"] = lowpass
    return d

def note(at, beats, midi, velocity=0.8, pan=0.0):
    n = {"at": round(at, 4), "beats": round(beats, 4), "midi": int(midi)}
    if velocity != 1: n["velocity"] = round(velocity, 3)
    if pan: n["pan"] = round(pan, 3)
    return n

def layer(name, gain, instrument, notes):
    return {"name": name, "gain": gain, "instrument": instrument, "notes": notes}

def pads(chords, per, hold=None, vel=0.7, spread=0.35):
    """Hold each chord `per` beats; voices fanned across the stereo field."""
    hold = hold or per - 0.4
    out = []
    for i, ch in enumerate(chords):
        n = len(ch)
        for j, m in enumerate(ch):
            pan = -spread + 2 * spread * j / max(1, n - 1)
            out.append(note(i * per, hold, m, vel, pan))
    return out

def bass(roots, per, rhythm, vel=0.85):
    """rhythm: list of (offset_in_bar, length, interval_from_root). Repeats every 4 beats."""
    out = []
    for i, r in enumerate(roots):
        for bar in range(int(per // 4)):
            base = i * per + bar * 4
            for off, length, iv in rhythm:
                out.append(note(base + off, length, r + iv, vel))
    return out

def arp(chords, per, step, order, vel=0.75, octave=0, pan_swing=0.5, length=None):
    """Arpeggiate chord tones; `order` indexes chord tones (len(ch) means octave of root)."""
    out = []
    length = length or step * 0.9
    for i, ch in enumerate(chords):
        tones = list(ch) + [ch[0] + 12]
        k = 0
        t = 0.0
        while t < per - 1e-9:
            idx = order[k % len(order)]
            m = tones[idx % len(tones)] + octave
            pan = pan_swing * (1 if k % 2 else -1)
            out.append(note(i * per + t, length, m, vel, pan))
            k += 1
            t += step
    return out

def melody(seq, vel=0.8, pan=0.0):
    """seq: list of (at, beats, midi) absolute within the score."""
    return [note(a, b, m, vel, pan) for a, b, m in seq]

def drums(total_beats, kick_offsets, kick_midi=36, kick_vel=0.9, snare_offsets=(), snare_midi=79):
    out = []
    for bar in range(int(total_beats // 4)):
        for off in kick_offsets:
            out.append(note(bar * 4 + off, 0.2, kick_midi, kick_vel))
        for off in snare_offsets:
            out.append(note(bar * 4 + off, 0.06, snare_midi, 0.5))
    return out

def project(seed, bpm, beats, layers, headroom=0.8):
    for l in layers:
        for n in l["notes"]:
            assert 0 <= n["at"] and n["at"] + n["beats"] <= beats + 1e-6, (l["name"], n)
            assert 12 <= n["midi"] <= 108, (l["name"], n)
    return {"version": 1, "seed": seed, "headroom": headroom, "effects": {},
            "music": {"kind": "score", "score": {"bpm": bpm, "beats": beats, "layers": layers}}}

# ---------------------------------------------------------------- Prickle Putt: G major, folksy, 104 bpm
def prickle_putt():
    per, beats = 8, 32
    chords = [[55, 59, 62, 67], [52, 55, 59, 64], [48, 52, 55, 60], [50, 54, 57, 62]]  # G Em C D
    roots = [43, 40, 36, 38]
    tune = [  # a whistled 8-bar tune in G (pentatonic leaning), (at, beats, midi)
        (0, 1, 67), (1, 0.5, 71), (1.5, 0.5, 74), (2, 1.5, 71), (3.5, 0.5, 69),
        (4, 1, 67), (5, 1, 64), (6, 2, 62),
        (8, 1, 64), (9, 0.5, 67), (9.5, 0.5, 71), (10, 1.5, 67), (11.5, 0.5, 64),
        (12, 1, 62), (13, 1, 64), (14, 2, 59),
        (16, 1, 60), (17, 0.5, 64), (17.5, 0.5, 67), (18, 1, 72), (19, 1, 67),
        (20, 1, 64), (21, 1, 60), (22, 2, 64),
        (24, 1, 66), (25, 0.5, 69), (25.5, 0.5, 74), (26, 1, 69), (27, 1, 66),
        (28, 1.5, 62), (29.5, 0.5, 66), (30, 2, 67),
    ]
    layers = [
        layer("meadow", 0.26, inst("triangle", 0.5, 0.6, 0.7, 1.2, 1800), pads(chords, per, 7.4, 0.65)),
        layer("stroll", 0.34, inst("square", 0.01, 0.15, 0.5, 0.15, 520),
              bass(roots, per, [(0, 0.9, 0), (1.5, 0.4, 7), (2, 0.9, 0), (3, 0.4, 12), (3.5, 0.4, 7)])),
        layer("plink", 0.36, inst("sine", 0.003, 0.22, 0.0, 0.18), arp(chords, per, 0.5, [0, 2, 1, 3, 2, 4, 3, 1], 0.7, 12)),
        layer("whistle", 0.3, inst("triangle", 0.04, 0.2, 0.6, 0.3, 4200), melody(tune, 0.75)),
    ]
    return project(7, 104, beats, layers)

# ---------------------------------------------------------------- Puff Pop: C major 7ths, tropical, 112 bpm
def puff_pop():
    per, beats = 8, 32
    chords = [[60, 64, 67, 71], [57, 60, 64, 67], [53, 57, 60, 64], [55, 59, 62, 65]]  # Cmaj7 Am7 Fmaj7 G7
    roots = [48, 45, 41, 43]
    danger = []
    for bar in range(8):
        for k in range(8):
            m = 52 if k % 2 == 0 else 53  # E to F half-step throb
            danger.append(note(bar * 4 + k * 0.5, 0.42, m, 0.85, 0.0))
    layers = [
        layer("reef", 0.22, inst("saw", 0.4, 0.6, 0.6, 1.0, 700), pads(chords, per, 7.5, 0.6, 0.4)),
        layer("groove", 0.34, inst("square", 0.008, 0.12, 0.55, 0.12, 420),
              bass(roots, per, [(0, 0.75, 0), (1, 0.25, 0), (1.5, 0.5, 12), (2.5, 0.5, 7), (3, 0.5, 0), (3.5, 0.5, 10)])),
        layer("bubbles", 0.33, inst("sine", 0.002, 0.12, 0.0, 0.15), arp(chords, per, 0.25, [0, 4, 2, 4, 1, 4, 3, 4, 2, 0, 4, 1, 3, 4, 0, 2], 0.6, 12, 0.7)),
        layer("steel", 0.3, inst("triangle", 0.003, 0.3, 0.1, 0.4, 3500),
              melody([(0, 1, 76), (1.5, 0.5, 79), (2, 1, 83), (3.5, 0.5, 79), (4, 1, 76), (5.5, 1.5, 74), (7, 1, 72),
                      (8, 1, 72), (9.5, 0.5, 76), (10, 1, 79), (11.5, 0.5, 76), (12, 1, 72), (13.5, 1.5, 71), (15, 1, 69),
                      (16, 1, 69), (17.5, 0.5, 72), (18, 1, 76), (19.5, 0.5, 72), (20, 1, 69), (21.5, 1.5, 67), (23, 1, 65),
                      (24, 1, 67), (25.5, 0.5, 71), (26, 1, 74), (27.5, 0.5, 71), (28, 1, 67), (29.5, 1.5, 65), (31, 1, 64)], 0.7, 0.15)),
        layer("thump", 0.3, inst("sine", 0.001, 0.09, 0.0, 0.05), drums(beats, [0, 2, 3.5], 38, 0.9)),
        layer("danger", 0.3, inst("saw", 0.01, 0.1, 0.4, 0.1, 1200), danger),
    ]
    return project(11, 112, beats, layers)

# ---------------------------------------------------------------- Topsy-Turvy: E minor, driving chip-synth, 128 bpm
def topsy_turvy():
    per, beats = 8, 32
    chords = [[52, 55, 59, 64], [48, 52, 55, 60], [50, 54, 57, 62], [47, 50, 54, 59]]  # Em C D Bm
    roots = [40, 36, 38, 35]
    echo = []
    for i, ch in enumerate(chords):
        for j, off in enumerate([0, 3, 5.5]):
            echo.append(note(i * per + off, 1.2, ch[(j + 1) % 4] + 24, 0.55, -0.6 if j % 2 else 0.6))
    layers = [
        layer("cave", 0.24, inst("saw", 0.3, 0.5, 0.65, 0.8, 450), pads(chords, per, 7.6, 0.6, 0.3)),
        layer("pulse", 0.36, inst("square", 0.004, 0.08, 0.6, 0.06, 620),
              bass(roots, per, [(0, .42, 0), (.5, .42, 0), (1, .42, 12), (1.5, .42, 0), (2, .42, 0), (2.5, .42, 12), (3, .42, 0), (3.5, .42, 7)], 0.9)),
        layer("wings", 0.27, inst("square", 0.004, 0.1, 0.3, 0.1, 3200), arp(chords, per, 0.25, [0, 1, 2, 4, 2, 1, 0, 4], 0.65, 12, 0.55)),
        layer("echo", 0.26, inst("sine", 0.01, 0.4, 0.2, 1.5), echo),
        layer("kick", 0.34, inst("sine", 0.001, 0.08, 0.0, 0.04), drums(beats, [0, 1, 2, 3], 36, 0.95, (1, 3), 81)),
    ]
    return project(13, 128, beats, layers)

if __name__ == "__main__":
    out = pathlib.Path(sys.argv[1]); out.mkdir(parents=True, exist_ok=True)
    for name, fn in [("prickle-putt", prickle_putt), ("puff-pop", puff_pop), ("topsy-turvy", topsy_turvy)]:
        p = fn()
        (out / f"{name}.json").write_text(json.dumps(p, indent=1) + "\n")
        notes = sum(len(l["notes"]) for l in p["music"]["score"]["layers"])
        print(name, "layers", [l["name"] for l in p["music"]["score"]["layers"]], "notes", notes,
              "seconds", round(p["music"]["score"]["beats"] * 60 / p["music"]["score"]["bpm"], 2))
