#!/usr/bin/env python3
"""Athena's Core icon pipeline.

Turns raw generated art (jpg/png) into the full Tauri icon set.

Pipeline: load -> grayscale -> hard threshold (pure 2-tone, kills jpeg grays)
-> autocrop white margin -> optional zoom (tighter crop) -> square-pad ->
resize 1024 -> optional thicken (expand black strokes) -> macOS superellipse
squircle alpha mask (n=5.0) -> save master -> `cargo tauri icon` -> backfill
sizes tauri icon doesn't emit (16/64/256/512) -> sync branding/out/.

Usage:
  python3 branding/process_icon.py branding/<art>.jpg [options]

Options:
  --thresh N    grayscale threshold for pure B/W split (default 140)
  --thicken N   number of 1px black-stroke expansion passes at 1024 (default 1)
  --zoom Z      crop-in factor around subject center; 1.0 = fit, 1.15 = 15% tighter (default 1.0)
  --out PATH    master output path (default branding/master-1024.png)
  --no-install  only build the master, skip cargo tauri icon + backfill + out sync

Requires Pillow (Homebrew python3).
"""

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageFilter

SIZE = 1024
SQUIRCLE_N = 5.0
ROOT = Path(__file__).resolve().parent.parent


def load_binarize(path: Path, thresh: int) -> Image.Image:
    img = Image.open(path).convert("L")
    bw = img.point(lambda p: 255 if p >= thresh else 0)
    # Ensure black-ink-on-white polarity (invert if the art came out white-on-black).
    if sum(bw.histogram()[0:128]) > sum(bw.histogram()[128:256]):
        bw = bw.point(lambda p: 255 - p)
    return bw


def autocrop(img: Image.Image, inset: int) -> Image.Image:
    # bbox of non-white pixels; shrink edges by `inset` px to drop jpeg halos.
    bbox = img.point(lambda p: 255 - p).getbbox()
    if bbox is None:
        sys.exit("ERROR: no non-white content found in source image")
    l, t, r, b = bbox
    l, t = min(l + inset, r), min(t + inset, b)
    r, b = max(r - inset, l), max(b - inset, t)
    return img.crop((l, t, r, b))


def square_zoom(img: Image.Image, zoom: float) -> Image.Image:
    w, h = img.size
    side = max(w, h)
    canvas = Image.new("L", (side, side), 255)
    canvas.paste(img, ((side - w) // 2, (side - h) // 2))
    if zoom > 1.0:
        inner = int(side / zoom)
        off = (side - inner) // 2
        canvas = canvas.crop((off, off, off + inner, off + inner))
    return canvas.resize((SIZE, SIZE), Image.LANCZOS)


def thicken(img: Image.Image, passes: int) -> Image.Image:
    # MinFilter expands dark (ink) regions by 1px per pass.
    for _ in range(passes):
        img = img.filter(ImageFilter.MinFilter(3))
    return img


def squircle_alpha(size: int) -> Image.Image:
    ss = 4  # supersample for a smooth edge
    n = SQUIRCLE_N
    mask = Image.new("L", (size * ss, size * ss), 0)
    px = mask.load()
    half = size / 2.0
    for y in range(size * ss):
        dy = abs((y + 0.5) / ss - half) / half
        for x in range(size * ss):
            dx = abs((x + 0.5) / ss - half) / half
            if (dx ** n + dy ** n) <= 1.0:
                px[x, y] = 255
    return mask.resize((size, size), Image.LANCZOS)


def build_master(src: Path, thresh: int, thicken_passes: int, zoom: float, out: Path) -> None:
    img = load_binarize(src, thresh)
    print(f"  binarized: {img.size}, pure 2-tone at thresh={thresh}")
    img = autocrop(img, inset=4)
    print(f"  autocrop:  {img.size}")
    img = square_zoom(img, zoom)
    if thicken_passes:
        img = thicken(img, thicken_passes)
        print(f"  thickened: {thicken_passes} pass(es), +{thicken_passes}px stroke width")
    alpha = squircle_alpha(SIZE)
    master = Image.merge("RGBA", (img, img, img, alpha))
    master.save(out)
    verify_alpha(master)
    print(f"  master saved: {out}")


def verify_alpha(master: Image.Image) -> None:
    a = master.getchannel("A")
    w, h = a.size
    corners = [a.getpixel(p) for p in [(2, 2), (w - 3, 2), (2, h - 3), (w - 3, h - 3)]]
    center = a.getpixel((w // 2, h // 2))
    transparent = a.histogram()[0]
    pct = 100.0 * transparent / (w * h)
    print(f"  alpha check: corners={corners} center={center} transparent={pct:.1f}%")
    if any(c != 0 for c in corners) or center == 0:
        sys.exit("ERROR: alpha mask looks wrong — inspect master before installing")


def run_tauri_icon(master: Path) -> None:
    cargo = shutil.which("cargo")
    if not cargo:
        sys.exit("ERROR: cargo not found on PATH")
    icons_dir = ROOT / "src-tauri" / "icons"
    subprocess.run([cargo, "tauri", "icon", str(master), "-o", str(icons_dir)], check=True, cwd=ROOT)


def resize_master(size: int) -> Image.Image:
    master = Image.open(ROOT / "branding" / "master-1024.png")
    return master.resize((size, size), Image.LANCZOS)


def backfill_and_sync() -> None:
    icons_dir = ROOT / "src-tauri" / "icons"
    out_dir = ROOT / "branding" / "out"
    out_dir.mkdir(exist_ok=True)

    # Sizes `cargo tauri icon` does not emit but the project keeps.
    for size, name in [(16, "16x16.png"), (64, "64x64.png"), (256, "256x256.png"), (512, "512x512.png")]:
        resize_master(size).save(icons_dir / name)

    # Mirror masked PNGs into branding/out/.
    for size in (16, 32, 48, 64, 128, 256, 512, 1024):
        dest = out_dir / f"{size}x{size}.png"
        if size == 1024:
            shutil.copy2(icons_dir / "icon.png", dest)
        else:
            resize_master(size).save(dest)
    for name in ("icon.icns", "icon.ico", "128x128.png", "128x128@2x.png", "32x32.png"):
        src = icons_dir / name
        if src.exists():
            shutil.copy2(src, out_dir / name)
    print(f"  backfilled src-tauri/icons (16/64/256/512) and synced {out_dir}")


def main() -> None:
    ap = argparse.ArgumentParser(description="Athena's Core icon pipeline")
    ap.add_argument("source", type=Path)
    ap.add_argument("--thresh", type=int, default=140)
    ap.add_argument("--thicken", type=int, default=1)
    ap.add_argument("--zoom", type=float, default=1.0)
    ap.add_argument("--out", type=Path, default=ROOT / "branding" / "master-1024.png")
    ap.add_argument("--no-install", action="store_true")
    args = ap.parse_args()

    if not args.source.exists():
        sys.exit(f"ERROR: source not found: {args.source}")

    print(f"Processing {args.source.name}:")
    build_master(args.source, args.thresh, args.thicken, args.zoom, args.out)

    if not args.no_install:
        print("Running cargo tauri icon:")
        run_tauri_icon(args.out)
        backfill_and_sync()
        print("Done — icon set installed. Rebuild/reinstall the app to see it.")
    else:
        print("Done (master only, --no-install).")


if __name__ == "__main__":
    main()
