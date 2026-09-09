"""Regression checks for the launcher's fixed-width WebKit relocation."""

import pathlib
import subprocess
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).with_name("relocate-webkit.sh")


class RelocationTests(unittest.TestCase):
    def relocate(self, original, source_path, target_path):
        with tempfile.TemporaryDirectory() as directory:
            source = pathlib.Path(directory) / "source.so"
            target = pathlib.Path(directory) / "target.so"
            source.write_bytes(original)
            result = subprocess.run(
                ["bash", str(SCRIPT), str(source), str(target), source_path, target_path],
                capture_output=True,
            )
            self.assertEqual(source.read_bytes(), original)
            return result, target.read_bytes() if target.exists() else None

    def test_relocation_preserves_binary_size_and_unrelated_bytes(self):
        for path in ("/usr/lib/x86_64-linux-gnu/webkitgtk-6.0", "/usr/lib/webkitgtk-6.0"):
            with self.subTest(path=path):
                original = b"\x7fELF\x00\xff\n" + path.encode() + b"\x00\n" + path.encode() + b"\x00tail"
                result, output = self.relocate(original, path, "/tmp/cv.12345678/w")
                self.assertEqual(result.returncode, 0, result.stderr)
                replacement = "/tmp/cv.12345678/w".ljust(len(path), "/").encode()
                self.assertEqual(output, original.replace(path.encode(), replacement))
                self.assertEqual(len(output), len(original))

    def test_missing_helper_path_fails_without_creating_output(self):
        result, output = self.relocate(b"unrelated", "/usr/lib/webkitgtk-6.0", "/tmp/cv.12345678/w")
        self.assertNotEqual(result.returncode, 0)
        self.assertIsNone(output)

    def test_unsupported_or_overlong_paths_fail_without_creating_output(self):
        for target in ("relative/path", "/tmp/shared/w", "/tmp/cv." + "x" * 60 + "/w"):
            with self.subTest(target=target):
                result, output = self.relocate(b"/usr/lib/webkitgtk-6.0", "/usr/lib/webkitgtk-6.0", target)
                self.assertNotEqual(result.returncode, 0)
                self.assertIsNone(output)


if __name__ == "__main__":
    unittest.main()
