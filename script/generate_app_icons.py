#!/usr/bin/env python3
import argparse
import json
import math
import shutil
import subprocess
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
MASTER_ICON = ROOT / "assets" / "icons" / "securechat-icon-1024.png"
IOS_APPICON = (
    ROOT
    / "apps"
    / "ios"
    / "SecureChatIOS"
    / "SecureChatIOS"
    / "Assets.xcassets"
    / "AppIcon.appiconset"
)
MAC_RESOURCES = ROOT / "apps" / "macos" / "SecureChatMac" / "Resources"
MAC_ICONSET = MAC_RESOURCES / "SecureChatMac.iconset"
MAC_ICNS = MAC_RESOURCES / "SecureChatMac.icns"


IOS_IMAGES = [
    {"idiom": "iphone", "size": "20x20", "scale": "2x", "pixels": 40},
    {"idiom": "iphone", "size": "20x20", "scale": "3x", "pixels": 60},
    {"idiom": "iphone", "size": "29x29", "scale": "2x", "pixels": 58},
    {"idiom": "iphone", "size": "29x29", "scale": "3x", "pixels": 87},
    {"idiom": "iphone", "size": "40x40", "scale": "2x", "pixels": 80},
    {"idiom": "iphone", "size": "40x40", "scale": "3x", "pixels": 120},
    {"idiom": "iphone", "size": "60x60", "scale": "2x", "pixels": 120},
    {"idiom": "iphone", "size": "60x60", "scale": "3x", "pixels": 180},
    {"idiom": "ipad", "size": "20x20", "scale": "1x", "pixels": 20},
    {"idiom": "ipad", "size": "20x20", "scale": "2x", "pixels": 40},
    {"idiom": "ipad", "size": "29x29", "scale": "1x", "pixels": 29},
    {"idiom": "ipad", "size": "29x29", "scale": "2x", "pixels": 58},
    {"idiom": "ipad", "size": "40x40", "scale": "1x", "pixels": 40},
    {"idiom": "ipad", "size": "40x40", "scale": "2x", "pixels": 80},
    {"idiom": "ipad", "size": "76x76", "scale": "1x", "pixels": 76},
    {"idiom": "ipad", "size": "76x76", "scale": "2x", "pixels": 152},
    {"idiom": "ipad", "size": "83.5x83.5", "scale": "2x", "pixels": 167},
    {"idiom": "ios-marketing", "size": "1024x1024", "scale": "1x", "pixels": 1024},
]

MAC_IMAGES = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]


def dark_corner_fill(width: int, height: int, x: int, y: int) -> tuple[int, int, int]:
    t = (x + y) / max(width + height - 2, 1)
    pulse = 0.5 + 0.5 * math.sin((x / width) * math.pi)
    r = round(14 + 8 * t)
    g = round(22 + 10 * pulse)
    b = round(28 + 12 * (1 - t))
    return (r, g, b)


def normalize_source(source: Path) -> Image.Image:
    image = Image.open(source).convert("RGB")
    side = min(image.size)
    left = (image.width - side) // 2
    top = (image.height - side) // 2
    image = image.crop((left, top, left + side, top + side))
    pixels = image.load()
    for y in range(image.height):
        for x in range(image.width):
            r, g, b = pixels[x, y]
            max_c = max(r, g, b)
            min_c = min(r, g, b)
            if max_c > 210 and max_c - min_c < 36:
                pixels[x, y] = dark_corner_fill(image.width, image.height, x, y)
    return image.resize((1024, 1024), Image.Resampling.LANCZOS)


def save_resized(master: Image.Image, path: Path, pixels: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    resized = master.resize((pixels, pixels), Image.Resampling.LANCZOS)
    resized.convert("RGB").save(path, "PNG", optimize=True)


def generate_ios(master: Image.Image) -> None:
    if IOS_APPICON.exists():
        shutil.rmtree(IOS_APPICON)
    IOS_APPICON.mkdir(parents=True)
    images = []
    for item in IOS_IMAGES:
        filename = f"AppIcon-{item['idiom']}-{item['size'].replace('.', '_')}-{item['scale']}.png"
        save_resized(master, IOS_APPICON / filename, item["pixels"])
        images.append(
            {
                "filename": filename,
                "idiom": item["idiom"],
                "scale": item["scale"],
                "size": item["size"],
            }
        )
    (IOS_APPICON / "Contents.json").write_text(
        json.dumps({"images": images, "info": {"author": "xcode", "version": 1}}, indent=2)
        + "\n"
    )
    contents = IOS_APPICON.parent / "Contents.json"
    if not contents.exists():
        contents.write_text('{"info":{"author":"xcode","version":1}}\n')


def generate_macos(master: Image.Image) -> None:
    if MAC_ICONSET.exists():
        shutil.rmtree(MAC_ICONSET)
    MAC_ICONSET.mkdir(parents=True, exist_ok=True)
    for filename, pixels in MAC_IMAGES:
        save_resized(master, MAC_ICONSET / filename, pixels)
    if MAC_ICNS.exists():
        MAC_ICNS.unlink()
    subprocess.run(["iconutil", "-c", "icns", str(MAC_ICONSET), "-o", str(MAC_ICNS)], check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate SecureChat app icons.")
    parser.add_argument("source", type=Path, help="Source square icon image")
    args = parser.parse_args()

    master = normalize_source(args.source)
    MASTER_ICON.parent.mkdir(parents=True, exist_ok=True)
    master.save(MASTER_ICON, "PNG", optimize=True)
    generate_ios(master)
    generate_macos(master)
    print(f"master={MASTER_ICON}")
    print(f"ios={IOS_APPICON}")
    print(f"macos={MAC_ICNS}")


if __name__ == "__main__":
    main()
