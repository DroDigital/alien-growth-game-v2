#!/usr/bin/env python3
"""Fetch CC0/permissive assets, convert them, and record them in the manifest.

Reads   assets/sources.json   (hand-edited list of what to fetch)
Writes  assets/manifest.json  (resolved URLs, authors, licenses, SHA-256s)
        assets/<output>       (converted files, e.g. KTX2 textures)
        CREDITS.md            (via `agtool credits`)

Assets are imported at build time only; the game never downloads anything.
A failed download is a warning, not an error: the game uses the asset's
procedural fallback. Use --strict to make failures fatal (release builds).

Usage:
  tools/fetch_assets.py                 resolve via provider APIs, record hashes
  tools/fetch_assets.py --locked        use URLs/hashes from manifest.json only (CI)
  tools/fetch_assets.py --ultra         also fetch the optional 4K Ultra pack
  tools/fetch_assets.py --only ID ...   fetch specific ids
  tools/fetch_assets.py --dry-run       print what would happen

Environment:
  FREESOUND_API_KEY   token for freesound.org sources (CC0 filter enforced)
  AGTOOL              path to a prebuilt agtool binary
  AGG_HTTP_TIMEOUT    seconds per request (default 60)
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import urllib.parse
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
SOURCES = ASSETS / "sources.json"
MANIFEST = ASSETS / "manifest.json"
CACHE = ASSETS / ".cache"
MANIFEST_VERSION = 1
USER_AGENT = "AlienGrowth-asset-fetcher/1.0 (build-time asset import)"
TIMEOUT = float(os.environ.get("AGG_HTTP_TIMEOUT", "60"))

# Provider endpoints. Overridable for tests (pointing at a local mock server).
POLYHAVEN_API = os.environ.get("AGG_POLYHAVEN_API", "https://api.polyhaven.com")
AMBIENTCG_GET = os.environ.get("AGG_AMBIENTCG_GET", "https://ambientcg.com/get")
FREESOUND_API = os.environ.get("AGG_FREESOUND_API", "https://freesound.org/apiv2")

TEXCONV_KINDS = {"color", "linear", "normal", "mask", "roughness", "hdr", "hdr-cubemap"}
CC0_URLS = {
    "http://creativecommons.org/publicdomain/zero/1.0/",
    "https://creativecommons.org/publicdomain/zero/1.0/",
}


class FetchError(Exception):
    pass


@dataclass
class Resolved:
    source_url: str
    page_url: str
    author: str
    license: str
    # Member to extract when source_url is a zip archive.
    member: str | None = None


def http_get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            return resp.read()
    except Exception as e:  # noqa: BLE001 - report any network failure uniformly
        raise FetchError(f"GET {url}: {e}") from e


def http_json(url: str) -> dict:
    try:
        return json.loads(http_get(url))
    except json.JSONDecodeError as e:
        raise FetchError(f"bad JSON from {url}: {e}") from e


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


# --- Providers -------------------------------------------------------------

def resolve_polyhaven(src: dict) -> Resolved:
    asset = src["asset"]
    files = http_json(f"{POLYHAVEN_API}/files/{urllib.parse.quote(asset)}")
    info = http_json(f"{POLYHAVEN_API}/info/{urllib.parse.quote(asset)}")
    try:
        entry = files[src["map"]][src["resolution"]][src["format"]]
    except KeyError as e:
        raise FetchError(f"polyhaven {asset}: no {src['map']}/{src['resolution']}/{src['format']}") from e
    authors = ", ".join(sorted(info.get("authors", {}).keys())) or "Poly Haven"
    return Resolved(
        source_url=entry["url"],
        page_url=f"https://polyhaven.com/a/{asset}",
        author=authors,
        license="CC0-1.0",
    )


def resolve_ambientcg(src: dict) -> Resolved:
    asset, res, fmt = src["asset"], src["resolution"], src["format"]
    archive = f"{asset}_{res}-{fmt}"
    return Resolved(
        source_url=f"{AMBIENTCG_GET}?file={archive}.zip",
        page_url=f"https://ambientcg.com/view?id={asset}",
        author="ambientCG (Lennart Demes)",
        license="CC0-1.0",
        member=f"{archive}_{src['map']}.{fmt.lower()}",
    )


def resolve_freesound(src: dict) -> Resolved:
    key = os.environ.get("FREESOUND_API_KEY")
    if not key:
        raise FetchError("FREESOUND_API_KEY not set; skipping freesound source")
    sound = http_json(
        f"{FREESOUND_API}/sounds/{int(src['asset'])}/"
        f"?fields=id,url,username,license,previews&token={urllib.parse.quote(key)}"
    )
    if sound.get("license") not in CC0_URLS:
        raise FetchError(f"freesound {src['asset']} is not CC0 ({sound.get('license')})")
    return Resolved(
        source_url=sound["previews"]["preview-hq-ogg"],
        page_url=sound["url"],
        author=sound["username"],
        license="CC0-1.0",
    )


def resolve_direct(src: dict) -> Resolved:
    for field in ("url", "page_url", "author", "license"):
        if not src.get(field):
            raise FetchError(f"direct source {src['id']} needs '{field}'")
    return Resolved(src["url"], src["page_url"], src["author"], src["license"], src.get("member"))


PROVIDERS = {
    "polyhaven": resolve_polyhaven,
    "ambientcg": resolve_ambientcg,
    "freesound": resolve_freesound,
    "direct": resolve_direct,
}


def resolve_locked(entry: dict) -> Resolved:
    url = entry["source_url"]
    member = None
    if "#" in url:
        url, member = url.split("#", 1)
    return Resolved(url, entry["page_url"], entry["author"], entry["license"], member)


# --- Download, extract, convert --------------------------------------------

def download(url: str, expected_sha: str | None, locked: bool) -> bytes:
    CACHE.mkdir(parents=True, exist_ok=True)
    cached = CACHE / hashlib.sha1(url.encode()).hexdigest()
    if cached.exists():
        data = cached.read_bytes()
        if expected_sha is None or sha256(data) == expected_sha:
            return data
    data = http_get(url)
    cached.write_bytes(data)
    return data


def extract(data: bytes, member: str) -> bytes:
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as zf:
            names = zf.namelist()
            match = next((n for n in names if n == member or n.endswith("/" + member)), None)
            if match is None:
                raise FetchError(f"{member} not in archive (has {len(names)} files)")
            return zf.read(match)
    except zipfile.BadZipFile as e:
        raise FetchError(f"not a zip archive: {e}") from e


def agtool_path() -> list[str]:
    env = os.environ.get("AGTOOL")
    if env:
        return [env]
    exe = "agtool.exe" if os.name == "nt" else "agtool"
    built = ROOT / "target" / "release" / exe
    if not built.exists():
        print("building agtool (release)...", flush=True)
        subprocess.run(
            ["cargo", "build", "-p", "tools", "--release", "--quiet"], cwd=ROOT, check=True
        )
    return [str(built)]


def convert(src: dict, raw: bytes, suffix: str, out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    mode = src.get("convert", "copy")
    if mode == "copy":
        out.write_bytes(raw)
        return
    if mode not in TEXCONV_KINDS:
        raise FetchError(f"unknown convert mode '{mode}'")
    tmp = CACHE / f"convert-input{suffix}"
    tmp.write_bytes(raw)
    cmd = agtool_path() + ["texconv", str(tmp), str(out), "--kind", mode]
    if src.get("max_size"):
        cmd += ["--max-size", str(src["max_size"])]
    result = subprocess.run(cmd, capture_output=True, text=True)
    tmp.unlink(missing_ok=True)
    if result.returncode != 0:
        raise FetchError(f"texconv failed: {result.stderr.strip()}")


def source_suffix(src: dict, resolved: Resolved) -> str:
    name = resolved.member or urllib.parse.urlparse(resolved.source_url).path
    suffix = Path(name).suffix
    return suffix if suffix else "." + str(src.get("format", "bin")).lower()


# --- Main ------------------------------------------------------------------

def load_json(path: Path, default):
    if not path.exists():
        return default
    return json.loads(path.read_text(encoding="utf-8"))


def process(src: dict, old: dict | None, args) -> dict:
    """Fetches one source; returns its manifest entry."""
    if args.locked:
        if old is None:
            raise FetchError("not in manifest.json (run without --locked first)")
        resolved = resolve_locked(old)
    else:
        provider = PROVIDERS.get(src["provider"])
        if provider is None:
            raise FetchError(f"unknown provider '{src['provider']}'")
        resolved = provider(src)

    if not resolved.source_url.startswith("https://") and not args.allow_http:
        raise FetchError(f"refusing non-https source {resolved.source_url}")

    source_url = resolved.source_url + (f"#{resolved.member}" if resolved.member else "")
    expected = old.get("source_sha256") if old and old.get("source_url") == source_url else None
    if args.dry_run:
        print(f"  would fetch {source_url}")
        return old or {}

    archive = download(resolved.source_url, None if resolved.member else expected, args.locked)
    raw = extract(archive, resolved.member) if resolved.member else archive
    raw_sha = sha256(raw)
    if expected and raw_sha != expected:
        msg = f"source hash changed: expected {expected}, got {raw_sha}"
        if args.locked:
            raise FetchError(msg)
        print(f"  warning: {msg}; recording new hash")

    out = ASSETS / src["output"]
    convert(src, raw, source_suffix(src, resolved), out)
    return {
        "id": src["id"],
        "kind": src["kind"],
        "path": src["output"],
        "source_url": source_url,
        "page_url": resolved.page_url,
        "author": resolved.author,
        "license": resolved.license,
        "source_sha256": raw_sha,
        "sha256": sha256(out.read_bytes()),
        "fallback": src["fallback"],
        "ultra_pack": bool(src.get("ultra_pack", False)),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--locked", action="store_true", help="only use URLs and hashes from manifest.json")
    ap.add_argument("--ultra", action="store_true", help="include the optional 4K Ultra texture pack")
    ap.add_argument("--only", nargs="+", metavar="ID", help="fetch only these ids")
    ap.add_argument("--strict", action="store_true", help="fail if any asset cannot be fetched")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--allow-http", action="store_true", help=argparse.SUPPRESS)  # local tests only
    ap.add_argument("--no-credits", action="store_true", help="skip CREDITS.md regeneration")
    args = ap.parse_args()

    sources = load_json(SOURCES, {"assets": []})["assets"]
    manifest = load_json(MANIFEST, {"version": MANIFEST_VERSION, "assets": []})
    entries = {e["id"]: e for e in manifest.get("assets", [])}

    ok = failed = skipped = 0
    for src in sources:
        sid = src["id"]
        if args.only and sid not in args.only:
            continue
        if src.get("ultra_pack") and not args.ultra:
            skipped += 1
            continue
        print(f"[{sid}] {src['provider']}:{src.get('asset', src.get('url', ''))}", flush=True)
        try:
            entry = process(src, entries.get(sid), args)
            if entry:
                entries[sid] = entry
            ok += 1
        except (FetchError, subprocess.CalledProcessError, OSError) as e:
            failed += 1
            print(f"  FAILED: {e}  -> game will use fallback '{src['fallback']}'", flush=True)

    # Drop manifest entries whose source was removed from sources.json.
    known = {s["id"] for s in sources}
    for stale in [i for i in entries if i not in known]:
        print(f"[{stale}] removed from sources.json; dropping from manifest")
        del entries[stale]

    if not args.dry_run:
        manifest = {"version": MANIFEST_VERSION, "assets": sorted(entries.values(), key=lambda e: e["id"])}
        MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        if not args.no_credits:
            subprocess.run(agtool_path() + ["--root", str(ROOT), "credits"], check=False)

    print(f"done: {ok} fetched, {failed} failed, {skipped} skipped (Ultra pack)")
    total = sum(p.stat().st_size for p in ASSETS.rglob("*") if p.is_file() and CACHE not in p.parents)
    print(f"assets/ size (excluding download cache): {total / 1e6:.1f} MB (base budget 5000 MB)")
    if args.strict and failed:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
