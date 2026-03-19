from __future__ import annotations

import hashlib
import importlib
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import tomllib
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
RELEASES_PATH = ROOT / "releases.toml"
MANIFEST_PATH = ROOT / "manifest.toml"
CACHE_DIR = ROOT / "cache"
MIRROR_ROOT = ROOT / "lzma-sdk"


def version_key(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in version.split("."))


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def ensure_parent(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download_archive(url: str, destination: Path, force: bool) -> None:
    ensure_parent(destination)
    if destination.exists() and not force:
        return

    with urllib.request.urlopen(url) as response, destination.open("wb") as handle:
        shutil.copyfileobj(response, handle)


def check_relative_path(name: str) -> Path:
    path = Path(name)
    if path.is_absolute():
        raise ValueError(f"absolute path in archive: {name}")
    if any(part == ".." for part in path.parts):
        raise ValueError(f"unsafe path in archive: {name}")
    return path


def load_py7zr():
    try:
        return importlib.import_module("py7zr")
    except ModuleNotFoundError as error:
        raise RuntimeError(
            "py7zr is required to extract .7z archives; install it with "
            "`python3 -m pip install -r lzma-sdk-sys/scripts/requirements.txt`"
        ) from error


def extract_7z(archive_path: Path, destination: Path) -> None:
    py7zr = load_py7zr()
    with py7zr.SevenZipFile(archive_path, mode="r") as archive:
        for name in archive.getnames():
            check_relative_path(name)
        archive.extractall(path=destination)


def extract_tar_bz2(archive_path: Path, destination: Path) -> None:
    with tarfile.open(archive_path, mode="r:bz2") as archive:
        for member in archive.getmembers():
            path = check_relative_path(member.name)
            if member.issym() or member.islnk():
                raise ValueError(f"links are not allowed in archive: {path}")
        archive.extractall(path=destination, filter="data")


def extract_archive(archive_path: Path, archive_format: str, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    if archive_format == "7z":
        extract_7z(archive_path, destination)
        return
    if archive_format == "tar.bz2":
        extract_tar_bz2(archive_path, destination)
        return
    raise ValueError(f"unsupported archive format: {archive_format}")


def effective_root(path: Path) -> Path:
    entries = sorted(path.iterdir(), key=lambda item: item.name)
    if len(entries) == 1 and entries[0].is_dir():
        return entries[0]
    return path


def normalized_mode(path: Path) -> int:
    return 0o755 if os.access(path, os.X_OK) else 0o644


def copy_normalized_tree(source_root: Path, destination_root: Path) -> None:
    if destination_root.exists():
        shutil.rmtree(destination_root)

    destination_root.mkdir(parents=True, exist_ok=True)

    for source_path in sorted(source_root.rglob("*")):
        relative = source_path.relative_to(source_root)
        destination_path = destination_root / relative

        if source_path.is_symlink():
            raise ValueError(f"symlinks are not supported: {source_path}")

        if source_path.is_dir():
            destination_path.mkdir(parents=True, exist_ok=True)
            os.chmod(destination_path, 0o755)
            continue

        destination_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source_path, destination_path)
        os.chmod(destination_path, normalized_mode(source_path))


def compute_tree_sha256(root: Path) -> str:
    digest = hashlib.sha256()

    for file_path in sorted(path for path in root.rglob("*") if path.is_file()):
        relative = file_path.relative_to(root).as_posix()
        mode = f"{normalized_mode(file_path):o}"
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(mode.encode("ascii"))
        digest.update(b"\0")
        with file_path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
        digest.update(b"\0")

    return digest.hexdigest()


def load_manifest_entries() -> dict[str, dict[str, str]]:
    if not MANIFEST_PATH.exists():
        return {}

    data = load_toml(MANIFEST_PATH)
    releases = data.get("release", [])
    return {entry["version"]: entry for entry in releases}


def write_manifest(entries: dict[str, dict[str, str]]) -> None:
    ordered = [entries[version] for version in sorted(entries, key=version_key)]

    def quote(value: str) -> str:
        escaped = value.replace("\\", "\\\\").replace('"', '\\"')
        return f'"{escaped}"'

    lines = [
        "# Generated by lzma-sdk-sys/scripts/import-lzma-sdk.py",
        "",
    ]

    for entry in ordered:
        lines.append("[[release]]")
        lines.append(f"version = {quote(entry['version'])}")
        lines.append(f"published = {quote(entry['published'])}")
        lines.append(f"archive_url = {quote(entry['archive_url'])}")
        lines.append(f"archive_file = {quote(entry['archive_file'])}")
        lines.append(f"archive_format = {quote(entry['archive_format'])}")
        lines.append(f"archive_sha256 = {quote(entry['archive_sha256'])}")
        lines.append(f"tree_sha256 = {quote(entry['tree_sha256'])}")
        lines.append(f"source_page = {quote(entry['source_page'])}")
        lines.append("")

    ensure_parent(MANIFEST_PATH)
    MANIFEST_PATH.write_text("\n".join(lines), encoding="utf-8")


def import_release(version: str, metadata: dict[str, str], force_download: bool) -> dict[str, str]:
    archive_path = CACHE_DIR / metadata["archive_file"]
    download_archive(metadata["archive_url"], archive_path, force=force_download)
    archive_sha256 = sha256_file(archive_path)

    with tempfile.TemporaryDirectory(prefix=f"lzma-sdk-{version}-") as temp_dir:
        extract_root = Path(temp_dir) / "extract"
        extract_archive(archive_path, metadata["archive_format"], extract_root)
        source_root = effective_root(extract_root)
        destination_root = MIRROR_ROOT / version
        copy_normalized_tree(source_root, destination_root)

    tree_sha256 = compute_tree_sha256(MIRROR_ROOT / version)

    return {
        "version": version,
        "published": metadata["published"],
        "archive_url": metadata["archive_url"],
        "archive_file": metadata["archive_file"],
        "archive_format": metadata["archive_format"],
        "archive_sha256": archive_sha256,
        "tree_sha256": tree_sha256,
        "source_page": metadata["source_page"],
    }
