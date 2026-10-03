"""Reproduce the stock acceptance fixture using canonical authoring operations.

Usage: python assets/games/observatory/author.py NEW_DIRECTORY --tools PATH_TO_BE2_TOOLS
Only rules/presentation are authored as JSON; geometry is generated and edited by the toolkit.
"""
import argparse
import json
from pathlib import Path
import subprocess


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def author(directory, tools):
    directory.mkdir(parents=True, exist_ok=False)
    evidence = []

    def run(*args):
        result = subprocess.run([str(tools), *args], cwd=directory, capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr or result.stdout)
        output = json.loads(result.stdout)
        evidence.append({"command": list(args), "result": {key: value for key, value in output.items()
                        if key in {"ok", "written", "operations", "records", "warnings", "rooms",
                                   "scenario", "errors", "findings"}}})
        return output

    blueprint = {
        "name": "Observatory Night Watch", "height": 3.2,
        "rooms": [
            {"id": "control", "rect": [-6, -4, 0, 4], "lamp": True,
             "floor_color": [0.12, 0.18, 0.25], "wall_color": [0.28, 0.36, 0.48]},
            {"id": "telescope", "rect": [0, -4, 6, 4], "lamp": True,
             "floor_color": [0.12, 0.18, 0.25], "wall_color": [0.28, 0.36, 0.48]},
        ],
        "doors": [{"between": ["control", "telescope"], "width": 2}],
        "spawns": [{"id": "watchkeeper", "room": "control", "offset": [-1, 0]}],
    }
    write(directory / "blueprint.json", blueprint)
    run("build", "blueprint.json", "base-map.json")
    write(directory / "geometry.patch.json", [
        {"op": "add_box", "id": "shutter", "label": "Observatory shutter",
         "center": [0, 1.3, 0], "half_extents": [0.12, 1.3, 1], "color": [0.75, 0.45, 0.16]},
        {"op": "add_box", "id": "baffle", "label": "Telescope baffle",
         "center": [2.8, 1.2, 0], "half_extents": [0.18, 1.2, 2.1],
         "color": [0.22, 0.32, 0.45], "structural": True},
    ])
    run("apply", "base-map.json", "geometry.patch.json", "map.json")
    # A valid temporary rule allows add-interactable to validate every intermediate document.
    game = {
        "schema_version": 1, "name": blueprint["name"], "map": "map.json",
        "player_profile": {"height": 1.8, "crouched_height": 1.1, "radius": 0.23,
                           "eye_height": 1.68, "walk_speed": 3.2, "sprint_speed": 5.6,
                           "crouch_speed": 1.3, "jump_height": 0.35},
        "spawn_points": [{"id": "watchkeeper", "feet": [-4, 0, 0], "yaw": 1.5707964}],
        "counters": {"battery": 20, "calibration": 0, "shutter_cycles": 0},
        "interactables": [{"entity": "shutter", "enabled": False}],
        "rules": [{"id": "bootstrap", "on_interact": "shutter", "once": True,
                   "actions": [{"action": "complete"}]}],
    }
    write(directory / "game.json", game)
    for name, at, label, disabled, color in [
        ("shutter-switch", "-3,1.3,-1.5", "Open shutter", False, "0.95,0.65,0.18"),
        ("calibrator", "5,1.3,1", "Calibrate telescope (3 presses)", True, "0.2,0.65,0.95"),
        ("uplink", "5,1.3,-1.7", "Transmit observation", True, "0.25,0.85,0.6"),
    ]:
        run("add-interactable", "game.json", name, f"--at={at}", f"--label={label}",
            f"--color={color}", "--write", *( ["--disabled"] if disabled else []))
    # Demonstrate an edit without recalculating any of the three duplicated bounds.
    write(directory / "edit.patch.json", [{"op": "translate", "nodes": ["calibrator"],
          "colliders": ["calibrator"], "entities": ["calibrator"], "delta": [0, 0, 0.2]}])
    run("apply", "map.json", "edit.patch.json", "edited-map.json")
    game = json.loads((directory / "game.json").read_text(encoding="utf-8"))
    game["map"] = "edited-map.json"
    game["interactables"] = [i for i in game["interactables"] if i["entity"] != "shutter"]
    game["movers"] = [{"id": "shutter-motion", "entity": "shutter", "translation": [0, 3, 0],
                       "duration_ticks": 90, "initial_open": False}]
    game["timers"] = [{"id": "battery-clock", "duration_ticks": 60,
                       "auto_start": True, "repeats": True}]
    game["rules"] = [
        {"id": "open-shutter", "on_interact": "shutter-switch", "once": True,
         "condition": {"counter": "shutter_cycles", "equals": 0}, "actions": [
            {"action": "set_mover", "mover": "shutter-motion", "open": True},
            {"action": "set_enabled", "entity": "calibrator", "enabled": True},
            {"action": "set_enabled", "entity": "shutter-switch", "enabled": False},
            {"action": "increment", "counter": "shutter_cycles", "amount": 1}]},
        {"id": "calibrate", "on_interact": "calibrator", "once": False,
         "condition": {"counter": "calibration", "less_than": 3}, "actions": [
             {"action": "increment", "counter": "calibration", "amount": 1}]},
        {"id": "unlock-uplink", "condition": {"counter": "calibration", "at_least": 3},
         "once": True, "actions": [{"action": "set_enabled", "entity": "uplink", "enabled": True},
                                    {"action": "set_enabled", "entity": "calibrator", "enabled": False}]},
        {"id": "transmit", "on_interact": "uplink", "once": True,
         "actions": [{"action": "complete"}]},
        {"id": "drain", "on_timer": "battery-clock", "once": False,
         "condition": {"counter": "battery", "greater_than": 0},
         "actions": [{"action": "increment", "counter": "battery", "amount": -1}]},
        {"id": "blackout", "on_timer": "battery-clock", "once": True,
         "condition": {"counter": "battery", "equals": 0}, "actions": [{"action": "fail"}]},
    ]
    game["presentation"] = {
        "objective": "Open shutter, calibrate three times, then transmit",
        "success": "Observation transmitted. Night watch complete! E / R to play again",
        "failure": "Battery depleted. E / R to restart the night watch",
        "counters": {"battery": {"label": "Battery", "format": "clock"},
                     "calibration": {"label": "Calibration", "units": "/ 3"},
                     "shutter_cycles": {"visible": False}},
        "palette": {"background": [0.04, 0.07, 0.14, 1], "panel": [0.03, 0.06, 0.12, 0.92],
                    "text": [0.85, 0.92, 1, 1], "accent": [1, 0.74, 0.3, 1],
                    "success": [0.35, 1, 0.75, 1], "failure": [1, 0.45, 0.35, 1]},
        "hud": {"scale": 1, "margin": 16, "width": 880, "crosshair": True},
    }
    write(directory / "game.json", game)
    # A negative variant is still abstractly winnable, but the shutter never opens.
    negative = json.loads(json.dumps(game))
    negative["rules"][0]["actions"] = negative["rules"][0]["actions"][1:]
    write(directory / "closed-gate.json", negative)
    run("game-validate", "game.json")
    run("game-explore", "game.json", "--scenario=win.json")
    run("lint", "edited-map.json", "--game=game.json", "--scenario=win.json")
    write(directory / "authoring-evidence.json", evidence)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--tools", type=Path, required=True)
    args = parser.parse_args()
    author(args.directory.resolve(), args.tools.resolve())
