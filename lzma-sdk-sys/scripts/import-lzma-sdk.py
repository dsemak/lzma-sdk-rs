#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys

from lzma_sdk_common import (
    RELEASES_PATH,
    import_release,
    load_manifest_entries,
    load_toml,
    version_key,
    write_manifest,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Import official LZMA SDK archives into the mirrored tree."
    )
    parser.add_argument(
        "--version",
        action="append",
        default=[],
        help="Specific SDK version to import. Can be passed more than once.",
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="Import every release listed in lzma-sdk-sys/releases.toml.",
    )
    parser.add_argument(
        "--force-download",
        action="store_true",
        help="Re-download archives even if they already exist in upstream/cache.",
    )
    return parser.parse_args()


def selected_versions(args: argparse.Namespace, releases: dict[str, dict[str, str]]) -> list[str]:
    if args.all:
        versions = list(releases)
    elif args.version:
        versions = list(dict.fromkeys(args.version))
    else:
        raise SystemExit("pass --all or at least one --version")

    unknown = [version for version in versions if version not in releases]
    if unknown:
        raise SystemExit(f"unknown versions: {', '.join(unknown)}")

    return sorted(versions, key=version_key)


def main() -> int:
    args = parse_args()
    releases = load_toml(RELEASES_PATH)["releases"]
    versions = selected_versions(args, releases)
    manifest = load_manifest_entries()

    for version in versions:
        entry = import_release(version, releases[version], force_download=args.force_download)
        manifest[version] = entry
        print(
            f"imported {version}: archive_sha256={entry['archive_sha256']} tree_sha256={entry['tree_sha256']}"
        )

    write_manifest(manifest)
    return 0


if __name__ == "__main__":
    sys.exit(main())
