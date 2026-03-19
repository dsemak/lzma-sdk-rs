from __future__ import annotations

import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock

SCRIPT_DIR = Path(__file__).resolve().parents[1]
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import lzma_sdk_common


class FakeSevenZipFile:
    opened_archive: Path | None = None
    extracted_destination: Path | None = None
    names: list[str] = []

    def __init__(self, archive_path: Path, mode: str) -> None:
        self.archive_path = archive_path
        self.mode = mode

    def __enter__(self) -> "FakeSevenZipFile":
        FakeSevenZipFile.opened_archive = self.archive_path
        if self.mode != "r":
            raise AssertionError(f"unexpected mode: {self.mode}")
        return self

    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return False

    def getnames(self) -> list[str]:
        return list(self.names)

    def extractall(self, path: Path) -> None:
        FakeSevenZipFile.extracted_destination = Path(path)


class LzmaSdkCommonTests(unittest.TestCase):
    def test_load_py7zr_reports_actionable_error(self) -> None:
        with mock.patch.object(lzma_sdk_common.importlib, "import_module", side_effect=ModuleNotFoundError):
            with self.assertRaisesRegex(RuntimeError, "py7zr is required"):
                lzma_sdk_common.load_py7zr()

    def test_extract_7z_uses_py7zr(self) -> None:
        fake_module = types.SimpleNamespace(SevenZipFile=FakeSevenZipFile)
        FakeSevenZipFile.names = ["sdk/C/Alloc.c"]
        FakeSevenZipFile.opened_archive = None
        FakeSevenZipFile.extracted_destination = None

        with tempfile.TemporaryDirectory() as temp_dir:
            archive_path = Path(temp_dir) / "sdk.7z"
            destination = Path(temp_dir) / "extract"

            with mock.patch.object(lzma_sdk_common, "load_py7zr", return_value=fake_module):
                lzma_sdk_common.extract_7z(archive_path, destination)

        self.assertEqual(FakeSevenZipFile.opened_archive, archive_path)
        self.assertEqual(FakeSevenZipFile.extracted_destination, destination)

    def test_extract_7z_rejects_unsafe_paths(self) -> None:
        fake_module = types.SimpleNamespace(SevenZipFile=FakeSevenZipFile)
        FakeSevenZipFile.names = ["../escape"]

        with tempfile.TemporaryDirectory() as temp_dir:
            archive_path = Path(temp_dir) / "sdk.7z"
            destination = Path(temp_dir) / "extract"

            with mock.patch.object(lzma_sdk_common, "load_py7zr", return_value=fake_module):
                with self.assertRaisesRegex(ValueError, "unsafe path"):
                    lzma_sdk_common.extract_7z(archive_path, destination)


if __name__ == "__main__":
    unittest.main()
