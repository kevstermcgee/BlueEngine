# BlueEngine audio backend patch

Source: quad-snd 0.2.8 (crates.io), upstream commit `5043a949ce02bc2119cfb7fb804ebec75ff71447`
(https://github.com/not-fl3/quad-snd), by Fedor Logachev, MIT/Apache-2.0.
This local package retains the upstream mixer and platform implementations.
BlueEngine uses it directly so standalone consumers receive the same fixes without
Cargo root patch overrides. Macroquad remains the rendering/window backend.
Linux/Windows device and worker failures become explicit status; PCM setup/runtime
resources are released on failure. Mixer submission is bounded. Decode errors
remain errors, and a ready device is never proof of audible playback.
Other platform implementations retain upstream behavior and are outside the
supported Linux/Windows verification lanes. Keep changes small and upstreamable.
