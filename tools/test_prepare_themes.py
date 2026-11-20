import json
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile
from prepare_themes import prepare

ROOT = Path(__file__).resolve().parents[1]


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

    def test_missing_listed_archive_fails_without_changing_outputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            source = target / "source"
            source.mkdir()
            options = target / "configuration-options.json"
            listed = {"board_themes": ["forest", "walnut", "slate", "bgblitz:bg-quantum"]}
            options.write_text(json.dumps(listed))
            catalog = target / "catalog.json"
            catalog.write_text('[{"id": "bgblitz:bg-quantum"}]\n')
            asset = target / "public/bg-quantum/background.jpg"
            asset.parent.mkdir(parents=True)
            asset.write_bytes(b"artwork")
            with self.assertRaisesRegex(
                ValueError, r"bg-quantum\.zip.*NEONDOJO_ALLOW_MISSING_THEMES=1"
            ):
                prepare(source, target / "public", catalog, options)
            self.assertEqual(json.loads(options.read_text()), listed)
            self.assertEqual(catalog.read_text(), '[{"id": "bgblitz:bg-quantum"}]\n')
            self.assertEqual(asset.read_bytes(), b"artwork")

    def test_allowed_missing_archive_removes_listed_theme(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            source = target / "source"
            source.mkdir()
            options = target / "configuration-options.json"
            options.write_text(
                json.dumps({"board_themes": ["forest", "walnut", "slate", "bgblitz:bg-quantum"]})
            )
            prepare(
                source,
                target / "public",
                target / "catalog.json",
                options,
                allow_missing=True,
            )
            self.assertEqual(
                json.loads(options.read_text()),
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
            rain = next((t for t in catalog if t["name"] == "CL Green (Rain)"), None)
            if rain:
                self.assertIn("cube64_L.png", rain["cubes"]["64_l"]["src"])
            self.assertEqual(
                prepare(ROOT / "themes", target / "public", target / "catalog.json"),
                catalog,
            )

    def test_self_contained_theme_can_be_built_without_optional_local_archives(self):
        import struct, zlib

        def png(width, height):
            def chunk(kind, data):
                return (
                    struct.pack(">I", len(data))
                    + kind
                    + data
                    + struct.pack(">I", zlib.crc32(kind + data))
                )

            return (
                b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
                + chunk(
                    b"IDAT", zlib.compress((b"\0" + b"\0\xff\0\xff" * width) * height)
                )
                + chunk(b"IEND", b"")
            )

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
            options = root / "configuration-options.json"
            options.write_text(json.dumps({"board_themes": ["forest", "walnut", "slate"]}))
            [theme] = prepare(source, root / "out", root / "manifest.json", options)
            self.assertEqual(
                json.loads(options.read_text())["board_themes"],
                ["forest", "walnut", "slate", "bgblitz:synthetic"],
            )
            self.assertEqual(theme["name"], "Synthetic")
            self.assertEqual(theme["background"]["width"], 128)
            self.assertEqual(
                (root / "out/synthetic/board.png").read_bytes(), png(128, 96)
            )

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
