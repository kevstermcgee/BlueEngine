"""Draws the Blue Test Lab shortcut icon (Feta the white lab rat on a glowing blue tile) -> scripts/test_lab.ico. Needs Pillow."""
import pathlib
from PIL import Image, ImageDraw

S = 1024  # draw big, shrink with Lanczos for silky smooth edges
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# Tile: Vibrant Blue Engine rounded square with cyan inner glow and navy bevel rim
# Outer shadow/border
d.rounded_rectangle((20, 20, S - 20, S - 20), radius=210, fill=(10, 25, 60, 255))
# Main deep sapphire blue tile
d.rounded_rectangle((44, 44, S - 44, S - 44), radius=190, fill=(24, 76, 175, 255))
# Cyan/sky top highlight glow band
d.rounded_rectangle((70, 60, S - 70, S // 2), radius=170, fill=(56, 138, 248, 255))
d.rounded_rectangle((44, S // 2 - 30, S - 44, S - 44), radius=190, fill=(24, 76, 175, 255))

# Subtle sci-fi grid / accent lines on the tile
d.line((70, S // 2 + 100, S - 70, S // 2 + 100), fill=(40, 105, 220, 180), width=6)
d.line((S // 2, 70, S // 2, S - 70), fill=(40, 105, 220, 180), width=6)

# Feta the white lab rat: pure white/cream fur, soft pink ears, ruby red eyes
fur = (250, 250, 246)
fur_shadow = (212, 216, 224)
pink = (255, 170, 188)
pink_shadow = (225, 140, 160)
ruby_eye = (195, 12, 35)
ruby_dark = (130, 8, 22)

cx, cy = S // 2, 560

# Ears (big round rat ears, soft pink inside)
for ex in (cx - 270, cx + 270):
    d.ellipse((ex - 190, 170, ex + 190, 550), fill=fur_shadow)
    d.ellipse((ex - 180, 180, ex + 180, 540), fill=fur)
    d.ellipse((ex - 145, 215, ex + 145, 505), fill=pink_shadow)
    d.ellipse((ex - 135, 225, ex + 135, 495), fill=pink)

# Head shape
d.ellipse((cx - 330, cy - 290, cx + 330, cy + 320), fill=fur_shadow)
d.ellipse((cx - 315, cy - 290, cx + 315, cy + 300), fill=fur)

# Cheeks / muzzle
d.ellipse((cx - 185, cy + 20, cx + 185, cy + 300), fill=(255, 255, 255))

# Big cute ruby red eyes (characteristic of albino lab rat Feta!)
for ex in (cx - 155, cx + 155):
    # Eye socket
    d.ellipse((ex - 66, cy - 124, ex + 66, cy + 34), fill=ruby_dark)
    # Ruby iris
    d.ellipse((ex - 60, cy - 120, ex + 60, cy + 30), fill=ruby_eye)
    # Bright specular highlights
    d.ellipse((ex - 32, cy - 105, ex + 8, cy - 65), fill=(255, 255, 255))
    d.ellipse((ex + 16, cy - 50, ex + 36, cy - 30), fill=(255, 220, 230))

# Pink twitchy nose
d.ellipse((cx - 52, cy + 55, cx + 52, cy + 130), fill=pink_shadow)
d.ellipse((cx - 48, cy + 58, cx + 48, cy + 124), fill=pink)

# Philtrum and smile
d.line((cx, cy + 124, cx, cy + 172), fill=(140, 80, 100), width=14)
d.arc((cx - 110, cy + 110, cx, cy + 225), 20, 160, fill=(140, 80, 100), width=14)
d.arc((cx, cy + 110, cx + 110, cy + 225), 20, 160, fill=(140, 80, 100), width=14)

# Two adorable buck teeth
for tx in (cx - 46, cx + 4):
    d.rounded_rectangle((tx, cy + 188, tx + 42, cy + 262), radius=10, fill=(255, 255, 250), outline=(180, 175, 170), width=6)

# Long white whiskers
for sign in (-1, 1):
    for dy, dx in ((-15, 260), (35, 280), (85, 250)):
        d.line((cx + sign * 130, cy + 110 + dy // 2, cx + sign * (130 + dx), cy + 110 + dy * 2 - 30), fill=(255, 255, 255, 230), width=12)

out_ico = pathlib.Path(__file__).parent / "test_lab.ico"
out_png = pathlib.Path(__file__).parent / "test_lab.png"

# Save multi-resolution Windows ICO
img.resize((256, 256), Image.LANCZOS).save(
    out_ico,
    format="ICO",
    sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)]
)
img.resize((512, 512), Image.LANCZOS).save(out_png)
print(f"Generated {out_ico} and {out_png}")
