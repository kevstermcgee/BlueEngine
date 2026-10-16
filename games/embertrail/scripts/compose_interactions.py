#!/usr/bin/env python3
"""Render original, behavior-specific Embertrail foley without external audio tools.

Deterministic modal synthesis, filtered noise and authored bell gestures; no borrowed
recordings or stock game-engine coin preset. Only assets/audio/sfx is written.
The bank uses BlueEngine's checked 44.1 kHz stereo PCM WAV contract.
"""
import array
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import wave

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "assets/audio/sfx"
RATE = 44100
TAU = math.tau
# Stable order matches src/cues.rs. The prose is also the reviewable sound brief.
DESIGNS = [
    ("spark-pickup", .17, 2, "Tiny warm glass tick with an airy tail; collectible spark"),
    ("shard-pickup", .50, 1, "Three irregular crystalline overtones, longer than a spark"),
    ("bloom-unfurl", .60, 1, "Soft leaf rustle opening into rounded petal resonances"),
    ("ember-ignite", .44, 1, "Breathy ignition whoosh with a warm low flame and tiny crackle"),
    ("invincibility-shimmer", .85, 1, "A spreading high glass shimmer; brief power activation only"),
    ("extra-life", .84, 1, "Four ascending soft lantern notes; reward heard once"),
    ("clay-break", .27, 2, "Dry granular crack, low pottery body and falling ceramic chips"),
    ("block-reveal", .29, 2, "Hollow wooden knock and rustling lid; an object opens"),
    ("checkpoint", .88, 1, "Two warm shelter bells with a low reassuring root note"),
    ("jump", .14, 2, "Quiet short upward breath; low rounded lift, no pickup notes"),
    ("spring", .47, 2, "Elastic wooden twang with a decaying oscillating pitch"),
    ("seed-cast", .16, 2, "Soft hollow puff and seed flick, kept below the score"),
    ("enemy-bounce", .23, 2, "Damped seedpod thump with a short rounded rebound"),
    ("shell-kick", .21, 2, "Hard wooden shell clack with a low rolling body"),
    ("boss-hit", .49, 2, "Deep night-collar crack and low resonant impact"),
    ("guardian-freed", 1.24, 1, "Collar glass breaks, then a warm bell resolves the guardian's release"),
    ("hurt", .35, 1, "Cracked lantern rattle and falling uneven resonance"),
    ("death", .78, 1, "A soft descending extinguishing breath, distinct from taking a hit"),
    ("grotto-enter", .55, 1, "Hollow root knocks descend beneath the ground with woody breath"),
    ("grotto-exit", .55, 1, "Hollow root knocks rise toward daylight with an open airy finish"),
    ("menu-confirm", .10, 1, "Muted round button click; no world reward motif"),
    ("course-clear", 1.15, 1, "Three resolving dawn-bell strikes and a rounded wooden knock"),
    ("homecoming", 1.72, 1, "Broad warm bell chord opening into the recovered Dawnheart's answer"),
    ("menu-navigate", .055, 1, "Very quiet brushed wooden tick, only for a changed selection"),
    ("menu-back", .13, 1, "Lower soft wooden button release; distinct from confirming"),
]


class Foley:
    def __init__(self, seconds, seed):
        self.left = [0.0] * round(seconds * RATE)
        self.right = [0.0] * len(self.left)
        self.random = random.Random(seed)

    def add(self, start, duration, source, gain=1.0, pan=0.0, attack=.003):
        offset = round(start * RATE)
        count = round(duration * RATE)
        for i in range(min(count, len(self.left) - offset)):
            t = i / RATE
            # Smooth both boundaries. Material envelopes are supplied by each voice.
            edge = min(1.0, t / attack, (duration - t) / .009)
            value = source(t) * gain * max(0.0, edge)
            self.left[offset+i] += value * (1.0 - max(0.0, pan) * .42)
            self.right[offset+i] += value * (1.0 + min(0.0, pan) * .42)

    def modal(self, start, duration, frequency, gain=.4, material="wood", pan=0):
        # Inharmonic mode ratios distinguish wood, glass, clay and soft membranes.
        modes = {
            "wood": [(1, 1, 10), (2.72, .38, 22), (4.93, .16, 36)],
            "glass": [(1, 1, 4.5), (2.61, .31, 7), (4.08, .18, 11)],
            "bell": [(1, 1, 3.2), (2.0, .42, 5), (2.76, .21, 7), (4.1, .08, 11)],
            "clay": [(1, 1, 21), (1.71, .55, 27), (3.3, .24, 39)],
            "skin": [(1, 1, 20), (1.59, .22, 32), (2.14, .12, 43)],
        }[material]
        self.add(start, duration, lambda t: sum(
            level * math.sin(TAU * frequency * ratio * t) * math.exp(-decay*t)
            for ratio, level, decay in modes), gain, pan)

    def breath(self, start, duration, gain=.3, cutoff=.15, decay=8, pan=0):
        low = 0.0
        def source(t):
            nonlocal low
            low += cutoff * (self.random.uniform(-1, 1) - low)
            return low * math.exp(-decay*t)
        self.add(start, duration, source, gain, pan, attack=.007)

    def glide(self, start, duration, begin, end, gain=.3, decay=12, wobble=0):
        slope = (end - begin) / duration
        def source(t):
            phase = TAU * (begin*t + slope*t*t*.5 + wobble*math.sin(35*t)*.02)
            return (math.sin(phase) + .15*math.sin(phase*2)) * math.exp(-decay*t)
        self.add(start, duration, source, gain)

    def render(self):
        # Remove residual DC before a final edge fade, then retain comfortable headroom.
        for channel in (self.left, self.right):
            dc = sum(channel) / len(channel)
            for i in range(len(channel)):
                edge = min(1, i / 90, (len(channel)-1-i) / 180)
                channel[i] = (channel[i] - dc) * edge
        peak = max(abs(x) for channel in (self.left, self.right) for x in channel)
        scale = .63 / max(peak, .001)
        pcm = array.array("h")
        for left, right in zip(self.left, self.right):
            pcm.append(round(left * scale * 32767))
            pcm.append(round(right * scale * 32767))
        if struct.pack("=H", 1) != struct.pack("<H", 1):
            pcm.byteswap()
        return pcm


def design(ident, seconds, variant):
    sound = Foley(seconds, 731 + 53 * variant + sum(map(ord, ident)))
    detune = (1, .965)[variant]
    def modal(at, length, pitch, gain=.4, material="wood", pan=0):
        sound.modal(at, length, pitch * detune, gain, material, pan)
    if ident == "spark-pickup":
        modal(0, .17, 1397, .36, "glass", -.12)
        sound.breath(0, .06, .18, .3, 28)
    elif ident == "shard-pickup":
        for at, pitch, pan in [(0, 1046, -.3), (.027, 1661, .3), (.075, 2489, 0)]:
            modal(at, seconds-at, pitch, .27, "glass", pan)
    elif ident == "bloom-unfurl":
        sound.breath(0, .34, .55, .035, 5)
        for at, pitch, pan in [(.06, 392, -.3), (.13, 588, .2), (.23, 784, .4)]:
            modal(at, seconds-at, pitch, .23, "wood", pan)
    elif ident == "ember-ignite":
        sound.breath(0, .4, .7, .08, 7)
        sound.glide(.025, .32, 95, 240, .23, 9)
        for at in (.02, .047, .095, .16):
            sound.breath(at, .035, .13, .8, 60)
    elif ident == "invincibility-shimmer":
        for i, pitch in enumerate([784, 1175, 1568, 1976, 2349, 3136]):
            modal(i*.06, seconds-i*.06, pitch, .19, "glass", (-.5, .5)[i%2])
    elif ident == "extra-life":
        for i, pitch in enumerate([523.25, 659.25, 784, 1046.5]):
            modal(i*.12, seconds-i*.12, pitch, .24, "bell", (i-1.5)*.15)
    elif ident == "clay-break":
        sound.breath(0, .18, .9, .65, 22)
        modal(0, .22, 230, .35, "clay")
        for i in range(4):
            modal(.02+i*.027, .1, 1100+i*170, .08, "clay", (i-1.5)*.2)
    elif ident == "block-reveal":
        modal(0, .2, 290, .6)
        modal(.035, .16, 510, .15, "wood", .2)
        sound.breath(.06, .21, .28, .06, 13)
    elif ident == "checkpoint":
        modal(0, .88, 392, .22, "bell", -.15)
        modal(.13, .75, 587.33, .24, "bell", .2)
        modal(.13, .67, 196, .11, "wood")
    elif ident == "jump":
        sound.glide(0, .14, 170*detune, 320*detune, .4, 22)
        sound.breath(0, .12, .3, .055, 18)
    elif ident == "spring":
        sound.glide(0, .47, 270*detune, 70*detune, .4, 8, 140)
        modal(0, .16, 640, .14, "wood")
    elif ident == "seed-cast":
        sound.breath(0, .16, .6, .12, 21)
        sound.glide(0, .12, 460*detune, 180*detune, .2, 25)
    elif ident == "enemy-bounce":
        modal(0, .23, 185, .65, "skin")
        sound.glide(.015, .18, 260, 100, .19, 19)
    elif ident == "shell-kick":
        modal(0, .18, 480, .55)
        modal(.009, .16, 730, .24, "wood", .1)
        modal(.015, .19, 125, .2, "skin")
    elif ident == "boss-hit":
        sound.breath(0, .21, .7, .35, 20)
        modal(0, .49, 93, .55, "clay")
        modal(.035, .34, 670, .18, "glass", -.2)
    elif ident == "guardian-freed":
        sound.breath(0, .21, .4, .6, 21)
        modal(0, .32, 1270, .15, "glass")
        for at, pitch, pan in [(.15, 293.66, -.3), (.24, 440, .3), (.36, 587.33, 0)]:
            modal(at, seconds-at, pitch, .3, "bell", pan)
    elif ident == "hurt":
        sound.breath(0, .15, .25, .35, 22)
        sound.glide(0, .35, 410, 135, .45, 13, 45)
        modal(.025, .22, 890, .12, "clay")
    elif ident == "death":
        sound.glide(0, .73, 340, 70, .32, 4)
        sound.breath(.12, .64, .45, .025, 5)
        modal(0, .32, 520, .12, "wood")
    elif ident in ("grotto-enter", "grotto-exit"):
        notes = [360, 275, 196] if ident.endswith("enter") else [196, 275, 360]
        for i, pitch in enumerate(notes):
            modal(i*.095, seconds-i*.095, pitch, .27, "wood", (i-1)*.15)
        sound.breath(.09, .42, .32, .03, 5)
    elif ident == "menu-confirm":
        modal(0, .1, 540, .5, "wood")
        modal(.012, .075, 810, .08, "skin")
    elif ident == "course-clear":
        modal(0, .2, 260, .2, "wood")
        for at, pitch in [(0, 523.25), (.14, 659.25), (.3, 784)]:
            modal(at, seconds-at, pitch, .27, "bell", at*.5-.1)
    elif ident == "homecoming":
        for i, pitch in enumerate([261.63, 392, 523.25, 659.25]):
            modal(i*.045, seconds-i*.045, pitch, .21, "bell", (i-1.5)*.25)
        for i, pitch in enumerate([784, 1046.5]):
            modal(.36+i*.15, 1.35-i*.15, pitch, .2, "glass", (i-.5)*.4)
    elif ident == "menu-navigate":
        modal(0, .055, 730, .25, "wood")
        sound.breath(0, .04, .25, .08, 45)
    elif ident == "menu-back":
        modal(0, .13, 320, .5, "wood")
        sound.breath(.01, .08, .13, .035, 30)
    else:
        raise ValueError(ident)
    return sound.render()


def fnv64(data):
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return f"{value:016x}"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    bank = dict(version=1, sample_rate=RATE, effects={}, music={}, music_scale=1.0)
    review = []
    for index, (ident, seconds, variants, brief) in enumerate(DESIGNS):
        bank["effects"][ident] = []
        for variant in range(variants):
            pcm = design(ident, seconds, variant)
            filename = f"{ident}-{variant+1}.wav"
            path = OUT / filename
            with wave.open(str(path), "wb") as wav:
                wav.setnchannels(2)
                wav.setsampwidth(2)
                wav.setframerate(RATE)
                wav.writeframes(pcm.tobytes())
            channels = [[sample/32768 for sample in pcm[channel::2]] for channel in range(2)]
            info = dict(file=filename, checksum=fnv64(path.read_bytes()), frames=len(pcm)//2,
                        peak=max(abs(x) for channel in channels for x in channel),
                        rms=max(math.sqrt(sum(x*x for x in channel)/len(channel)) for channel in channels),
                        dc=max(abs(sum(channel)/len(channel)) for channel in channels),
                        seam_jump=max(abs(channel[0]-channel[-1]) for channel in channels))
            bank["effects"][ident].append(info)
        review.append(dict(cue=index, id=ident, seconds=seconds, variants=variants,
                           behavior=brief, synthesis="Original deterministic modal/noise synthesis"))
        print(f"{index:2} {ident}: {seconds:.3f}s, {variants} variant(s)")
    (OUT / "bank.json").write_text(json.dumps(bank, indent=2)+"\n")
    (OUT / "design.json").write_text(json.dumps(dict(
        title="Embertrail — objects have voices", sample_rate=RATE,
        provenance="Original authored procedural foley; no recordings or stock presets",
        generator="scripts/compose_interactions.py", cues=review,
        content_sha256=hashlib.sha256(json.dumps(bank, sort_keys=True).encode()).hexdigest(),
        review_scope="Numerical waveform and contract checks; physical speaker review remains human",
    ), indent=2)+"\n")


if __name__ == "__main__":
    main()
