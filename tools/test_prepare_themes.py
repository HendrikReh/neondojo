import json
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile
import struct
import zlib
from prepare_themes import png_luminance, prepare

ROOT = Path(__file__).resolve().parents[1]
ADAM7 = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]


def png(width, height, pixel=(0, 255, 0, 255), color=6, palette=None, interlace=False):
    """Encode a single-colour 8-bit PNG; `pixel` holds the raw channel samples."""

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data))
        )

    def scanlines(w, h):
        return (b"\0" + bytes(pixel) * w) * h

    if interlace:
        raw = b"".join(
            scanlines((width - x + dx - 1) // dx, (height - y + dy - 1) // dy)
            for x, y, dx, dy in ADAM7
            if width > x and height > y
        )
    else:
        raw = scanlines(width, height)
    extra = b""
    if palette:
        colors, alphas = palette
        extra = chunk(b"PLTE", colors) + chunk(b"tRNS", alphas)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, color, 0, 0, int(interlace)))
        + extra
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def synthetic_archive(path, red_checker, green_checker):
    """Write a minimal BGBlitz theme whose red/green artwork the caller chooses."""
    xml = (
        '<theme displayName="Synthetic"><background name="board.png" numbersType="twoImage">'
        + "".join(
            f'<numbers type="{direction}" name="{direction}?.png"/>'
            for direction in ["RLD", "RLU", "LRD", "LRU"]
        )
        + '</background><checker red="red?.png" green="green?.png">'
        + ",".join(str(i * 8) for i in range(15))
        + '</checker><dice type="sixImage" red="redDie?.png" green="greenDie?.png"/>'
        + '<cube type="sixImage" name="cube?.png"/></theme>'
    )
    with ZipFile(path, "w") as z:
        z.writestr("themes/synthetic/theme.xml", xml)
        z.writestr("themes/synthetic/board.png", png(128, 96))
        z.writestr("themes/synthetic/red.png", red_checker)
        z.writestr("themes/synthetic/green.png", green_checker)
        for name in [
            *[f"{side}Die{i}" for side in ["red", "green"] for i in range(1, 7)],
            *[f"cube{i}" for i in [2, 4, 8, 16, 32, 64]],
            *[f"{d}{i}" for d in ["RLD", "RLU", "LRD", "LRU"] for i in [1, 2]],
        ]:
            z.writestr(f"themes/synthetic/{name}.png", png(4, 4))


class ThemePreparationTests(unittest.TestCase):
    def test_preparation_preserves_non_theme_configuration(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            source = target / "source"
            source.mkdir()
            options = target / "configuration-options.json"
            engines = {"gnubg": {"analysis": [0, 1, 2, 3, 4]}, "bgsage": {"analysis": [2]}}
            options.write_text(json.dumps({"board_themes": [], "engine_profiles": engines}))
            prepare(source, target / "public", target / "catalog.json", options)
            self.assertEqual(json.loads(options.read_text()).get("engine_profiles"), engines)

    def test_configuration_options_include_builtin_and_generated_theme_ids(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            source = target / "source"
            source.mkdir()
            prepare(source, target / "public", target / "catalog.json",
                    target / "configuration-options.json")
            self.assertEqual(
                json.loads((target / "configuration-options.json").read_text()),
                {"board_themes": ["forest", "walnut", "slate"]},
            )

    @unittest.skipUnless(
        list((ROOT / "themes").glob("*.zip")), "optional local artwork"
    )
    def test_real_archives_have_original_assets_geometry_and_case_sensitive_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            catalog = prepare(
                ROOT / "themes", target / "public", target / "catalog.json"
            )
            self.assertEqual(len(catalog), len(list((ROOT / "themes").glob("*.zip"))))
            self.assertEqual(len({t["id"] for t in catalog}), len(catalog))
            self.assertEqual(json.loads((target / "catalog.json").read_text()), catalog)
            for theme in catalog:
                self.assertEqual(len(theme["columns"]), 15)
                self.assertGreater(theme["background"]["width"], 1000)
                self.assertEqual(len(theme["dice"]["white"]), 6)
                self.assertEqual(len(theme["dice"]["black"]), 6)
                self.assertIn("64", theme["cubes"])
                for image in [
                    theme["background"],
                    *theme["checkers"]["white"],
                    *theme["checkers"]["black"],
                    *theme["dice"]["white"],
                    *theme["dice"]["black"],
                    *theme["cubes"].values(),
                ]:
                    data = (
                        target / "public" / image["src"].removeprefix("/themes/")
                    ).read_bytes()
                    self.assertTrue(data)
                    self.assertGreater(image["width"], 0)
                light, dark = (
                    png_luminance(
                        (target / "public" / theme["checkers"][side][0]["src"].removeprefix("/themes/")).read_bytes()
                    )
                    for side in ("white", "black")
                )
                self.assertGreater(light, dark, theme["id"])
            rain = next((t for t in catalog if t["name"] == "CL Green (Rain)"), None)
            if rain:
                self.assertIn("cube64_L.png", rain["cubes"]["64_l"]["src"])
            self.assertEqual(
                prepare(ROOT / "themes", target / "public", target / "catalog.json"),
                catalog,
            )

    def test_self_contained_theme_can_be_built_without_optional_local_archives(self):
        xml = (
            '<theme displayName="Synthetic"><background name="board.png" numbersType="twoImage">'
            + "".join(
                f'<numbers type="{direction}" name="{direction}?.png"/>'
                for direction in ["RLD", "RLU", "LRD", "LRU"]
            )
            + '</background><checker red="white?.png" green="black?.png">'
            + ",".join(str(i * 8) for i in range(15))
            + '</checker><dice type="sixImage" red="white?.png" green="black?.png"/><cube type="sixImage" name="cube?.png"/></theme>'
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "in"
            source.mkdir()
            with ZipFile(source / "synthetic.zip", "w") as z:
                z.writestr("themes/synthetic/theme.xml", xml)
                z.writestr("themes/synthetic/board.png", png(128, 96))
                for name in [
                    "white",
                    "black",
                    *[f"{side}{i}" for side in ["white", "black"] for i in range(1, 7)],
                    *[f"cube{i}" for i in [2, 4, 8, 16, 32, 64]],
                    *[f"{d}{i}" for d in ["RLD", "RLU", "LRD", "LRU"] for i in [1, 2]],
                ]:
                    z.writestr(f"themes/synthetic/{name}.png", png(4, 4))
            [theme] = prepare(source, root / "out", root / "manifest.json")
            self.assertEqual(theme["name"], "Synthetic")
            self.assertEqual(theme["background"]["width"], 128)
            self.assertEqual(
                (root / "out/synthetic/board.png").read_bytes(), png(128, 96)
            )

    def test_light_checker_set_is_white_whatever_its_bgblitz_colour(self):
        light = png(6, 6, (240, 240, 250, 255))
        dark = png(6, 6, (30, 40, 90, 255))
        # BG Moon and most BGBlitz themes name the dark set "red"; CL Green
        # names its light set "red". White must be the light set in both.
        for red, green, white, black in [
            (dark, light, "green", "red"),
            (light, dark, "red", "green"),
        ]:
            with self.subTest(white=white), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = root / "in"
                source.mkdir()
                synthetic_archive(source / "synthetic.zip", red, green)
                [theme] = prepare(source, root / "out", root / "manifest.json")
                self.assertEqual(
                    [i["src"] for i in theme["checkers"]["white"]],
                    [f"/themes/synthetic/{white}.png"],
                )
                self.assertEqual(
                    [i["src"] for i in theme["checkers"]["black"]],
                    [f"/themes/synthetic/{black}.png"],
                )
                self.assertEqual(
                    theme["dice"]["white"][0]["src"], f"/themes/synthetic/{white}Die1.png"
                )
                self.assertEqual(
                    theme["dice"]["black"][0]["src"], f"/themes/synthetic/{black}Die1.png"
                )

    def test_checker_luminance_ignores_transparency_and_png_encoding(self):
        white = 255.0
        self.assertAlmostEqual(png_luminance(png(9, 7, (255, 255, 255, 255))), white)
        self.assertAlmostEqual(
            png_luminance(png(9, 7, (255, 255, 255, 255), interlace=True)), white
        )
        self.assertAlmostEqual(png_luminance(png(9, 7, (255,), color=0)), white)
        self.assertAlmostEqual(png_luminance(png(9, 7, (255, 255, 255), color=2)), white)
        self.assertAlmostEqual(png_luminance(png(3, 3, (0, 0, 255, 255))), 0.0722 * 255)
        self.assertAlmostEqual(
            png_luminance(png(5, 5, (1,), color=3, palette=(b"\0\0\0\xff\xff\xff", b"\0\xff"))),
            white,
        )
        with self.assertRaisesRegex(ValueError, "no opaque pixels"):
            png_luminance(png(4, 4, (255, 255, 255, 0)))
        with self.assertRaisesRegex(ValueError, "must be PNG"):
            png_luminance(b"\xff\xd8\xff")

    @unittest.skipUnless(
        (ROOT / "themes/bg-earth.zip").exists(), "optional local BG Earth artwork"
    )
    def test_changing_archive_filename_case_keeps_urls_and_assets_stable(self):
        import shutil

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "in"
            source.mkdir()
            archive = source / "Earth.zip"
            shutil.copy(ROOT / "themes/bg-earth.zip", archive)
            (root / "out/EARTH").mkdir(parents=True)
            (root / "out/EARTH/old.png").write_bytes(b"old")
            first = prepare(source, root / "out", root / "manifest.json")
            archive.rename(source / "earth.zip")
            second = prepare(source, root / "out", root / "manifest.json")
            self.assertEqual(first, second)
            for image in [second[0]["background"], *second[0]["checkers"]["white"]]:
                self.assertTrue(
                    (root / "out" / image["src"].removeprefix("/themes/")).exists()
                )

    def test_empty_directory_keeps_builtin_client_usable(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertEqual(
                prepare(root / "absent", root / "out", root / "manifest.json"), []
            )

    def test_bad_archives_and_unsafe_paths_fail_clearly(self):
        for entry, data in [
            ("theme.xml", b"<theme"),
            ("../escape.png", b"bad"),
            ("theme.xml", b'<!DOCTYPE theme [<!ENTITY x "a">]><theme/>'),
        ]:
            with self.subTest(entry=entry), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = root / "in"
                source.mkdir()
                with ZipFile(source / "broken.zip", "w") as z:
                    z.writestr(entry, data)
                with self.assertRaisesRegex(ValueError, "broken.zip"):
                    prepare(source, root / "out", root / "manifest.json")
                self.assertFalse((root / "escape.png").exists())

    @unittest.skipUnless(
        (ROOT / "themes/bg-earth.zip").exists(), "optional local BG Earth artwork"
    )
    def test_removed_archive_does_not_leave_bundled_assets_or_catalog_entries(self):
        import shutil

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "in"
            source.mkdir()
            shutil.copy(ROOT / "themes/bg-earth.zip", source)
            prepare(source, root / "out", root / "manifest.json")
            (source / "bg-earth.zip").unlink()
            self.assertEqual(prepare(source, root / "out", root / "manifest.json"), [])
            self.assertFalse(list((root / "out").rglob("*.png")))


if __name__ == "__main__":
    unittest.main()
