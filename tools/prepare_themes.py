"""Prepare local BGBlitz ZIP artwork for the web client; no third-party packages.

Specification: https://www.bgblitz.com/Create_a_theme_How_to.txt
Images are copied verbatim. Nothing is downloaded or extracted as executable code.
"""

import io
import json
import math
from pathlib import Path, PurePosixPath
import struct
import xml.etree.ElementTree as ET
from zipfile import ZipFile, BadZipFile
import zlib

ROOT = Path(__file__).resolve().parents[1]
MAX_ARCHIVE = 32 * 1024 * 1024
MAX_EXPANDED = 64 * 1024 * 1024
MAX_CHECKER_PIXELS = 1024 * 1024


def dimensions(data):
    if data.startswith(b"\x89PNG\r\n\x1a\n") and len(data) >= 24:
        width, height = struct.unpack(">II", data[16:24])
    elif data.startswith(b"\xff\xd8"):
        stream = io.BytesIO(data)
        stream.read(2)
        while True:
            byte = stream.read(1)
            if not byte:
                raise ValueError("JPEG has no size header")
            if byte != b"\xff":
                continue
            marker = stream.read(1)
            while marker == b"\xff":
                marker = stream.read(1)
            if not marker or marker in (b"\xd9", b"\xda"):
                raise ValueError("JPEG has no size header")
            if marker[0] in [0x01, *range(0xD0, 0xD9)]:
                continue
            size = struct.unpack(">H", stream.read(2))[0]
            if marker[0] in [
                0xC0,
                0xC1,
                0xC2,
                0xC3,
                0xC5,
                0xC6,
                0xC7,
                0xC9,
                0xCA,
                0xCB,
                0xCD,
                0xCE,
                0xCF,
            ]:
                _, height, width = struct.unpack(">BHH", stream.read(5))
                break
            if size < 2:
                raise ValueError("Invalid JPEG segment")
            stream.seek(size - 2, 1)
    else:
        raise ValueError("Only PNG and JPEG theme artwork is supported")
    if not (0 < width <= 8192 and 0 < height <= 8192):
        raise ValueError("Image dimensions outside 1–8192")
    return width, height


def write_changed(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists() or path.read_bytes() != data:
        temporary = path.with_name(path.name + ".tmp")
        temporary.write_bytes(data)
        temporary.replace(path)


def png_luminance(data):
    """Mean relative luminance (0–255) of the opaque pixels of a PNG image.

    BGBlitz names its two checker sets "red" and "green" after the players, not
    after their colours, so the light set is identified from the artwork.
    """
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("Checker artwork must be PNG")
    position, idat, palette, transparency, header = 8, [], b"", b"", None
    while position + 8 <= len(data):
        (size,) = struct.unpack(">I", data[position : position + 4])
        kind = data[position + 4 : position + 8]
        body = data[position + 8 : position + 8 + size]
        position += 12 + size
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"PLTE":
            palette = body
        elif kind == b"tRNS":
            transparency = body
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if header is None:
        raise ValueError("PNG has no header")
    width, height, depth, color, _, _, interlace = header
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(color)
    if channels is None or depth not in (1, 2, 4, 8, 16) or interlace not in (0, 1):
        raise ValueError("Unsupported PNG format")
    if width * height > MAX_CHECKER_PIXELS:
        raise ValueError("Checker artwork has more than 1024 × 1024 pixels")
    bits = channels * depth
    passes = (
        [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
        if interlace
        else [(0, 0, 1, 1)]
    )
    sizes = [
        ((width - x + dx - 1) // dx, (height - y + dy - 1) // dy)
        for x, y, dx, dy in passes
    ]
    expected = sum(h * (1 + (w * bits + 7) // 8) for w, h in sizes if w and h)
    inflater = zlib.decompressobj()
    raw = inflater.decompress(b"".join(idat), expected + 1)
    if len(raw) != expected:
        raise ValueError("PNG image data has an unexpected size")
    step = max(1, bits // 8)
    total = weight = 0.0
    offset = 0
    for pass_width, pass_height in sizes:
        if not pass_width or not pass_height:
            continue
        stride = (pass_width * bits + 7) // 8
        previous = bytearray(stride)
        for _ in range(pass_height):
            kind, line = raw[offset], bytearray(raw[offset + 1 : offset + 1 + stride])
            offset += 1 + stride
            for i in range(stride):
                left = line[i - step] if i >= step else 0
                up = previous[i]
                corner = previous[i - step] if i >= step else 0
                if kind == 1:
                    line[i] = (line[i] + left) & 255
                elif kind == 2:
                    line[i] = (line[i] + up) & 255
                elif kind == 3:
                    line[i] = (line[i] + ((left + up) >> 1)) & 255
                elif kind == 4:
                    guess = left + up - corner
                    a, b, c = abs(guess - left), abs(guess - up), abs(guess - corner)
                    paeth = left if a <= b and a <= c else up if b <= c else corner
                    line[i] = (line[i] + paeth) & 255
                elif kind != 0:
                    raise ValueError("Invalid PNG filter")
            previous = line
            if depth < 8:
                samples = [
                    (byte >> shift) & ((1 << depth) - 1)
                    for byte in line
                    for shift in range(8 - depth, -1, -depth)
                ][: pass_width * channels]
                scale = 255 / ((1 << depth) - 1)
            else:
                samples = line[:: depth // 8]
                scale = 1
            for pixel in range(pass_width):
                s = samples[pixel * channels : (pixel + 1) * channels]
                if color == 3:
                    index = s[0]
                    if 3 * index + 3 > len(palette):
                        raise ValueError("PNG palette index out of range")
                    r, g, b = palette[3 * index : 3 * index + 3]
                    alpha = transparency[index] if index < len(transparency) else 255
                else:
                    r, g, b = (s[0] * scale,) * 3 if channels <= 2 else s[:3]
                    alpha = s[-1] * scale if channels in (2, 4) else 255
                if alpha >= 128:
                    total += 0.2126 * r + 0.7152 * g + 0.0722 * b
                    weight += 1
    if not weight:
        raise ValueError("Checker artwork has no opaque pixels")
    return total / weight


def load_archive(path, output, used):
    if path.stat().st_size > MAX_ARCHIVE:
        raise ValueError("ZIP exceeds 32 MiB")
    with ZipFile(path) as archive:
        entries = {}
        for entry in archive.infolist():
            p = PurePosixPath(entry.filename)
            if p.is_absolute() or ".." in p.parts or "\\" in entry.filename:
                raise ValueError("Unsafe archive path")
            if not entry.is_dir():
                key = entry.filename.casefold()
                if key in entries:
                    raise ValueError("Ambiguous archive filename")
                entries[key] = entry.filename
        if (
            len(entries) > 2000
            or sum(i.file_size for i in archive.infolist()) > MAX_EXPANDED
        ):
            raise ValueError("Expanded ZIP exceeds theme size limits")
        definitions = []
        for name in entries.values():
            if name.lower().endswith(".xml"):
                data = archive.read(name)
                if (
                    len(data) > 1024 * 1024
                    or b"<!DOCTYPE" in data.upper()
                    or b"<!ENTITY" in data.upper()
                ):
                    raise ValueError("Unsafe or oversized XML")
                node = ET.fromstring(data)
                if node.tag == "theme":
                    definitions.append((name, node))
        if len(definitions) != 1:
            raise ValueError("ZIP must contain exactly one theme definition")
        definition, theme = definitions[0]
        directory = PurePosixPath(definition).parent
        slug = path.stem.lower()
        if not slug or not all(
            c.isascii() and (c.isalnum() or c in "_-") for c in slug
        ):
            raise ValueError("Theme ZIP name must contain only letters, digits, - or _")
        # Canonical directories also work when archives are renamed only by
        # letter case on macOS, then the build is deployed on Linux.
        if output.exists():
            for existing in output.iterdir():
                if (
                    existing.is_dir()
                    and existing.name.lower() == slug
                    and existing.name != slug
                ):
                    temporary = existing.with_name(existing.name + ".case-tmp")
                    existing.rename(temporary)
                    temporary.rename(output / slug)

        artwork = {}

        def element(name):
            value = theme.find(name)
            if value is None:
                raise ValueError("Missing " + name + " definition")
            return value

        def image(name, optional=False):
            requested = directory / PurePosixPath(name)
            if requested.is_absolute() or ".." in requested.parts or "\\" in name:
                raise ValueError("Unsafe image reference")
            actual = entries.get(str(requested).casefold())
            if actual is None:
                if optional:
                    return None
                raise ValueError("Missing image: " + str(requested))
            relative = PurePosixPath(actual).relative_to(directory)
            if relative.suffix.lower() not in (".png", ".jpg", ".jpeg"):
                raise ValueError("Unsupported image extension")
            data = archive.read(actual)
            width, height = dimensions(data)
            target = output / slug / str(relative)
            write_changed(target, data)
            used.add(target)
            from urllib.parse import quote

            result = dict(
                src="/themes/" + quote(slug) + "/" + quote(str(relative)),
                width=width,
                height=height,
            )
            artwork[result["src"]] = data
            return result

        def integer(node, key, default=0):
            value = int(node.get(key, str(default)))
            if abs(value) > 8192:
                raise ValueError("Theme coordinate exceeds limits")
            return value

        background = element("background")
        checker = element("checker")
        dice = element("dice")
        cube = element("cube")
        if (
            background.get("numbersType") != "twoImage"
            or dice.get("type") != "sixImage"
            or cube.get("type") != "sixImage"
        ):
            raise ValueError(
                "This adapter requires two-image numbers and separate dice/cube images"
            )
        columns = [float(n.strip()) for n in (checker.text or "").split(",")]
        board = image(background.attrib["name"])
        if (
            len(columns) != 15
            or not all(math.isfinite(n) and 0 <= n < board["width"] for n in columns)
            or columns != sorted(columns)
        ):
            raise ValueError(
                "Expected 15 ordered checker column coordinates within the board"
            )

        def variations(pattern):
            names = [pattern.replace("?", "")] + [
                pattern.replace("?", str(i))
                for i in range(2, min(integer(checker, "numImages", 1), 32) + 1)
            ]
            found = [
                value
                for n in dict.fromkeys(names)
                if (value := image(n, optional=True)) is not None
            ]
            if not found:
                raise ValueError("Missing checker artwork")
            if any(
                (p["width"], p["height"]) != (found[0]["width"], found[0]["height"])
                for p in found
            ):
                raise ValueError("Checker variants must share dimensions")
            return found

        numbers = {}
        for child in background.findall("numbers"):
            numbers[child.attrib["type"]] = [
                image(child.attrib["name"].replace("?", str(i))) for i in [1, 2]
            ]
        if not all(name in numbers for name in ["LRD", "LRU", "RLD", "RLU"]):
            raise ValueError("Missing number orientation")
        cubes = {
            str(value): image(cube.attrib["name"].replace("?", str(value)))
            for value in [2, 4, 8, 16, 32, 64]
        }
        for side in ["l", "r"]:
            value = image(cube.attrib["name"].replace("?", "64_" + side), optional=True)
            if value:
                cubes["64_" + side] = value
        checker_sets = {
            color: variations(checker.attrib[color]) for color in ("red", "green")
        }
        # NeonDojo's "white" is the light set; player 0 and the White choice
        # use it on every board, independent of BGBlitz's red/green order.
        brightness = {
            color: png_luminance(artwork[images[0]["src"]])
            for color, images in checker_sets.items()
        }
        light, dark = ("red", "green")
        if brightness["green"] > brightness["red"]:
            light, dark = dark, light
        return dict(
            id="bgblitz:" + slug,
            name=theme.attrib["displayName"],
            source="BGBlitz",
            background=board,
            columns=columns,
            checkerOffsetX=integer(checker, "offsetX"),
            checkerOffsetY=integer(checker, "offsetY"),
            checkers=dict(white=checker_sets[light], black=checker_sets[dark]),
            dice={
                side: [
                    image(dice.attrib[color].replace("?", str(i))) for i in range(1, 7)
                ]
                for side, color in [("white", light), ("black", dark)]
            },
            diceLeftX=integer(dice, "leftX"),
            diceRightX=integer(dice, "rightX"),
            cubes=cubes,
            cubeLeftX=integer(cube, "leftX"),
            cubeRightX=integer(cube, "rightX"),
            cubeOffsetY=integer(cube, "offsetY"),
            cubeCenteredY=integer(cube, "centeredCubeYOffset"),
            numbers=numbers,
            numbersX=integer(background, "numbersXoff"),
            numbersY=[
                integer(background, "numbersYoff"),
                integer(background, "numbersYoff2"),
            ],
        )


def prepare(source, output, manifest, options_manifest=None):
    used = set()
    catalog = []
    archives = sorted(source.glob("*.zip"))
    if len({p.stem.lower() for p in archives}) != len(archives):
        raise ValueError("Theme ZIP names must be distinct ignoring letter case")
    for archive in archives:
        try:
            catalog.append(load_archive(archive, output, used))
        except (
            ValueError, KeyError, BadZipFile, ET.ParseError, struct.error, zlib.error
        ) as error:
            raise ValueError(f"{archive.name}: {error}") from error
    if output.exists():
        for stale in output.rglob("*"):
            if stale.is_file() and stale not in used:
                stale.unlink()
    write_changed(
        manifest, (json.dumps(catalog, ensure_ascii=False, indent=2) + "\n").encode()
    )
    if options_manifest is not None:
        options = json.loads(options_manifest.read_text()) if options_manifest.exists() else {}
        options["board_themes"] = ["forest", "walnut", "slate"] + [theme["id"] for theme in catalog]
        write_changed(
            options_manifest,
            (json.dumps(options, indent=2) + "\n").encode(),
        )
    return catalog


if __name__ == "__main__":
    try:
        result = prepare(
            ROOT / "themes",
            ROOT / "web/public/themes",
            ROOT / "web/src/generated-themes.json",
            ROOT / "shared/configuration-options.json",
        )
        print(f"Prepared {len(result)} local BGBlitz themes")
    except (ValueError, OSError) as error:
        raise SystemExit(str(error))
