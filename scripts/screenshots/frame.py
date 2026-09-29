"""Rounded window corners, a hairline border and a soft shadow on transparent padding.

python frame.py <out dir> <shot.png>...   (needs Pillow)
"""
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

RADIUS = 16   # Windows 11 uses 8 px; screenshots are 2x
PAD = 48

def frame(src: Path, dst: Path):
    img = Image.open(src).convert("RGBA")
    w, h = img.size
    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, w - 1, h - 1), RADIUS, fill=255)
    dark = sum(img.getpixel((5, 5))[:3]) < 300
    border = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(border).rounded_rectangle(
        (0, 0, w - 1, h - 1), RADIUS,
        outline=(255, 255, 255, 40) if dark else (0, 0, 0, 38), width=2)
    img = Image.alpha_composite(img, border)
    img.putalpha(mask)

    out = Image.new("RGBA", (w + 2 * PAD, h + 2 * PAD), (0, 0, 0, 0))
    shadow = Image.new("L", out.size, 0)
    ImageDraw.Draw(shadow).rounded_rectangle((PAD, PAD + 12, PAD + w, PAD + h + 12), RADIUS, fill=90)
    shadow = shadow.filter(ImageFilter.GaussianBlur(20))
    out.paste((0, 0, 0, 255), (0, 0), shadow)
    out.putalpha(shadow)
    out.alpha_composite(img, (PAD, PAD))
    out.save(dst, optimize=True)

for s in sys.argv[2:]:
    p = Path(s)
    frame(p, Path(sys.argv[1]) / p.name)
    print(p.name)
