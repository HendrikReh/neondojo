"""Check the generated manual's local links, GitHub source links, and logo."""

import sys
import tomllib
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urljoin, urlsplit


ROOT = Path(__file__).resolve().parents[1]


class Page(HTMLParser):
    def __init__(self, file):
        super().__init__()
        self.ids = set()
        self.links = []
        self.images = []
        self.icons = []
        self.feed(file.read_text(encoding="utf-8"))
        self.close()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        if tag == "a" and "href" in attrs:
            self.links.append(attrs["href"])
        if tag == "img" and "src" in attrs:
            self.images.append(attrs["src"])
        if tag == "link" and "icon" in attrs.get("rel", "").split():
            self.icons.append(attrs["href"])


def check():
    with (ROOT / "zensical.toml").open("rb") as config:
        project = tomllib.load(config)["project"]
    site = ROOT / project["site_dir"]
    base = project["site_url"]
    repo = project["repo_url"] + "/"
    logo = project["theme"]["logo"]
    if (site / logo).read_bytes() != (ROOT / "web/public/neondojo-logo.svg").read_bytes():
        raise ValueError("Published logo differs from the application's logo")

    pages = {file.relative_to(site): Page(file) for file in site.rglob("*.html")}
    if Path("index.html") not in pages:
        raise ValueError("Missing handbook home page")
    errors = []
    for path, page in pages.items():
        page_url = urljoin(base, path.as_posix())
        for sources in (page.images, page.icons):
            if not any(urljoin(page_url, src) == urljoin(base, logo) for src in sources):
                errors.append(f"{path}: missing logo or favicon")
                break
        for href in page.links + page.images:
            url = urljoin(page_url, href)
            if url.startswith((repo + "blob/main/", repo + "tree/main/")):
                kind, rel = urlsplit(url[len(repo):]).path.split("/main/", 1)
                target = ROOT / unquote(rel)
                exists = target.is_file() if kind == "blob" else target.is_dir()
                if not exists:
                    errors.append(f"{path}: missing repository target {href}")
            elif url.startswith(base):
                parts = urlsplit(url[len(base):])
                rel = Path(unquote(parts.path))
                if (site / rel).is_dir():
                    rel /= "index.html"
                if not (site / rel).is_file():
                    errors.append(f"{path}: missing site target {href}")
                elif (
                    rel in pages
                    and parts.fragment
                    and unquote(parts.fragment) not in pages[rel].ids
                ):
                    errors.append(f"{path}: missing anchor {href}")
            elif not urlsplit(href).scheme and not href.startswith("//"):
                errors.append(f"{path}: relative link leaves the published site: {href}")

    if errors:
        sys.exit("\n".join(sorted(set(errors))))
    print(f"Checked links, logo and favicon on {len(pages)} HTML pages.")


if __name__ == "__main__":
    check()
