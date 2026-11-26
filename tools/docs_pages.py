"""Keep links in the published manual compatible with the GitHub Markdown sources."""

import re
from pathlib import Path
from urllib.parse import quote

from mkdocs.structure.files import File


def heading_id(title, separator):
    # GitHub drops punctuation and gives each space its own hyphen.
    return re.sub(r"[^\w -]", "", title.lower()).replace(" ", separator)


def on_config(config):
    config.mdx_configs.setdefault("toc", {})["slugify"] = heading_id
    return config


def on_files(files, config):
    logo = Path(config.docs_dir).parent / "web/public/neondojo-logo.svg"
    files.append(File.generated(config, "assets/neondojo-logo.svg", content=logo.read_bytes()))
    return files


def on_page_markdown(markdown, page, config, files):
    docs = Path(config.docs_dir)
    root = docs.parent
    source = Path(page.file.abs_src_path)

    def repository_link(match):
        path, fragment = match.groups()
        target = (source.parent / path).resolve()
        if target.is_relative_to(docs):
            entry = files.get_file_from_path(target.relative_to(docs).as_posix())
            if entry and entry.inclusion.is_included():
                return match.group()
        if not target.is_relative_to(root) or not target.exists():
            return match.group()  # Let the strict build report a missing link.
        kind = "tree" if target.is_dir() else "blob"
        url = f"{config.repo_url}/{kind}/main/{quote(target.relative_to(root).as_posix())}"
        return f"]({url}{fragment or ''})"

    # Source files, historical plans, and directory listings live on GitHub.
    return re.sub(r"\]\((\.\./[^\s)#]+)(#[^\s)]*)?\)", repository_link, markdown)
