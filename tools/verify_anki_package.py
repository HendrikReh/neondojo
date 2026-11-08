"""Optional integration check against Anki's own import library in a fresh profile.

Run with a Python environment containing the official `anki` package. This is a
development check only; BG Tutor's export and learning implementation is Rust.
"""

import argparse
import json
from pathlib import Path
import tempfile


def verify(package: Path, expected: int | None = None, updated: Path | None = None):
    from anki.collection import Collection, ImportAnkiPackageRequest

    with tempfile.TemporaryDirectory(prefix="bgtutor-anki-import-") as directory:
        collection = Collection(str(Path(directory) / "collection.anki2"))
        try:
            collection.import_anki_package(
                ImportAnkiPackageRequest(package_path=str(package.resolve()))
            )
            initial_cards = set(collection.find_cards(""))
            initial_notes = set(collection.find_notes(""))
            if expected is not None:
                assert len(initial_cards) == expected, "Unexpected imported card count"
            initial_fields = {
                nid: list(collection.get_note(nid).fields) for nid in initial_notes
            }
            for fields in initial_fields.values():
                assert not any(
                    "<script" in field.lower() for field in fields
                ), "Raw script in exported note fields"
            collection.import_anki_package(
                ImportAnkiPackageRequest(package_path=str(package.resolve()))
            )
            assert (
                set(collection.find_cards("")) == initial_cards
            ), "Reimport added cards"
            assert (
                set(collection.find_notes("")) == initial_notes
            ), "Reimport added notes"
            if updated:
                collection.import_anki_package(
                    ImportAnkiPackageRequest(package_path=str(updated.resolve()))
                )
                assert set(collection.find_cards("")) == initial_cards
                assert set(collection.find_notes("")) == initial_notes
                assert any(
                    list(collection.get_note(nid).fields) != initial_fields[nid]
                    for nid in initial_notes
                ), "Updated package did not update note contents"
            return {
                "cards": len(initial_cards),
                "notes": len(initial_notes),
                "reimport_deduplicated": True,
                "content_update_checked": bool(updated),
            }
        finally:
            collection.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("--expected-cards", type=int)
    parser.add_argument("--updated", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.package, args.expected_cards, args.updated)))
