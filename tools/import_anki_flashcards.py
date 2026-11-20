#!/usr/bin/env python3
"""Render and import APKG files into NeonDojo using Anki's package reader."""
import argparse
from collections import Counter
from hashlib import file_digest
from html import escape
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory

try:
    from anki.collection import Collection, ImportAnkiPackageRequest
    from anki.lang import set_lang
    from anki.utils import strip_html
except ImportError as error:
    print(f"Anki Python renderer unavailable: {error}", file=sys.stderr)
    raise SystemExit(3) from error

TYPED_ANSWER = re.compile(r"\[\[type:(.+?)\]\]")


def render_typed_answer(html, collection, card, note, *, show_answer):
    def replace(match):
        field = match[1]
        prefix, separator, name = field.partition(":")
        if separator and prefix in {"cloze", "nc"}:
            field = name
        value = note[field] if field in note else ""
        if separator and prefix == "cloze" and value:
            value = collection.extract_cloze_for_typing(value, card.ord + 1)
        if not value:
            return ""
        if show_answer:
            return f'<div class="typeGood">{escape(strip_html(value))}</div>'
        return '<input type="text" id="typeans" autocomplete="off">'

    return TYPED_ANSWER.sub(replace, html)


def render_package(path: Path, output: Path) -> int:
    with path.open("rb") as source:
        package = file_digest(source, "sha256").hexdigest()
    collection = Collection(str(output / "collection.anki2"))
    decks: Counter[str] = Counter()
    try:
        request = ImportAnkiPackageRequest(package_path=str(path))
        request.options.with_scheduling = False
        request.options.with_deck_configs = False
        collection.import_anki_package(request)
        deck_names = {deck.id: deck.name for deck in collection.decks.all_names_and_ids()}
        with (output / "cards.jsonl").open("w", encoding="utf-8") as target:
            for card_id in sorted(collection.find_cards("")):
                card = collection.get_card(card_id)
                note = card.note()
                deck = deck_names[card.did]
                rendered = card.render_output()
                model = note.note_type()
                prompt = note["Title"].strip() if "Title" in note else ""
                if not prompt or "<" in prompt or len(prompt.encode()) > 500:
                    prompt = model["name"] if model else deck
                row = {
                    "guid": note.guid,
                    "template_number": card.ord + 1,
                    "deck": deck,
                    "prompt": prompt,
                    "tags": note.tags,
                    "css": rendered.css,
                    "front": render_typed_answer(
                        rendered.question_text, collection, card, note, show_answer=False
                    ),
                    "back": render_typed_answer(
                        rendered.answer_text, collection, card, note, show_answer=True
                    ),
                }
                target.write(json.dumps(row, ensure_ascii=False) + "\n")
                decks[deck] += 1
        media = Path(collection.media.dir())
    finally:
        collection.close()
    shutil.move(media, output / "media")
    (output / "manifest.json").write_text(
        json.dumps({
            "package": package,
            "decks": [{"name": name, "cards": count} for name, count in sorted(decks.items())],
        }),
        encoding="utf-8",
    )
    return decks.total()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="APKG file or directory to search recursively")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--importer", type=Path, help="built import_anki_flashcards Rust example")
    mode.add_argument("--render-to", type=Path, help="render one package into an existing empty directory")
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    files = [source] if source.is_file() else sorted(
        path for path in source.rglob("*") if path.is_file() and path.suffix.lower() == ".apkg"
    )
    if not files or any(path.suffix.lower() != ".apkg" for path in files):
        parser.error("Select at least one .apkg file")
    set_lang("en")
    if args.render_to:
        if len(files) != 1 or not args.render_to.is_dir() or any(args.render_to.iterdir()):
            parser.error("Render mode requires one .apkg and an empty output directory")
        render_package(files[0], args.render_to)
        return
    importer = args.importer.resolve(strict=True)
    root = Path(__file__).resolve().parent.parent
    for path in files:
        with TemporaryDirectory(prefix="neondojo-anki-") as temporary:
            directory = Path(temporary)
            count = render_package(path, directory)
            print(f"{path.name} — {count} cards", flush=True)
            subprocess.run([importer, directory], cwd=root, check=True)


if __name__ == "__main__":
    main()
