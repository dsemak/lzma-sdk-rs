#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys

from lzma_sdk_common import (
    CACHE_DIR,
    MIRROR_ROOT,
    RELEASES_PATH,
    compute_tree_sha256,
    download_archive,
    load_manifest_entries,
    load_toml,
    sha256_file,
    version_key,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify mirrored LZMA SDK trees against the recorded manifest."
    )
    parser.add_argument(
        "--version",
        action="append",
        default=[],
        help="Specific SDK version to verify. Can be passed more than once.",
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="Verify every release listed in lzma-sdk-sys/manifest.toml.",
    )
    parser.add_argument(
        "--download-missing",
        action="store_true",
        help="Download missing archives into lzma-sdk-sys/cache before verifying.",
    )
    return parser.parse_args()


def selected_versions(args: argparse.Namespace, manifest: dict[str, dict[str, str]]) -> list[str]:
    if args.all:
        versions = list(manifest)
    elif args.version:
        versions = list(dict.fromkeys(args.version))
    else:
        raise SystemExit("pass --all or at least one --version")

    unknown = [version for version in versions if version not in manifest]
    if unknown:
        raise SystemExit(f"versions missing from manifest: {', '.join(unknown)}")

    return sorted(versions, key=version_key)


def verify_release(
    version: str,
    manifest_entry: dict[str, str],
    release_entry: dict[str, str],
    download_missing: bool,
) -> None:
    for key in ("published", "archive_url", "archive_file", "archive_format", "source_page"):
        if manifest_entry[key] != release_entry[key]:
            raise ValueError(
                f"{version}: manifest field {key!r} does not match lzma-sdk-sys/releases.toml"
            )

    archive_path = CACHE_DIR / manifest_entry["archive_file"]
    if archive_path.exists():
        archive_sha256 = sha256_file(archive_path)
    elif download_missing:
        download_archive(manifest_entry["archive_url"], archive_path, force=False)
        archive_sha256 = sha256_file(archive_path)
    else:
        raise FileNotFoundError(f"{version}: missing cached archive {archive_path}")

    if archive_sha256 != manifest_entry["archive_sha256"]:
        raise ValueError(f"{version}: archive sha256 mismatch")

    tree_root = MIRROR_ROOT / version
    if not tree_root.is_dir():
        raise FileNotFoundError(f"{version}: missing mirrored tree {tree_root}")

    tree_sha256 = compute_tree_sha256(tree_root)
    if tree_sha256 != manifest_entry["tree_sha256"]:
        raise ValueError(f"{version}: tree sha256 mismatch")

    print(f"verified {version}: archive_sha256={archive_sha256} tree_sha256={tree_sha256}")


def main() -> int:
    manifest = load_manifest_entries()
    if not manifest:
        raise SystemExit("lzma-sdk-sys/manifest.toml does not exist yet")

    release_index = load_toml(RELEASES_PATH)["releases"]
    args = parse_args()
    versions = selected_versions(args, manifest)

    for version in versions:
        if version not in release_index:
            raise SystemExit(f"{version}: missing from lzma-sdk-sys/releases.toml")
        verify_release(
            version,
            manifest_entry=manifest[version],
            release_entry=release_index[version],
            download_missing=args.download_missing,
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
