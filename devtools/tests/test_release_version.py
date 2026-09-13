import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("release_version", Path(__file__).resolve().parents[1] / "release_version.py")
VERSION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERSION)


class ReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write("Cargo.toml", '[workspace.package]\nversion = "1.2.3"\n\n[workspace.dependencies]\nthing = "4.5.6"\n')
        self.write("crates/core/Cargo.toml", '[package]\nname = "sn-core"\nversion.workspace = true\n')
        self.write("Cargo.lock", '[[package]]\nname = "sn-core"\nversion = "1.2.2"\n\n[[package]]\nname = "external"\nversion = "1.2.2"\nsource = "registry+https://example.com"\n')
        self.write("Makefile", 'help:\n\t@echo "make update VERSION=v1.2.2"\n')

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def read(self, name):
        return (self.root / name).read_text(encoding="utf-8")

    def run_tool(self, flag):
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            return VERSION.main([flag], root=self.root)

    def test_check_is_read_only_sync_updates_new_guides_and_is_idempotent(self):
        guide = (
            f"git clone --branch v1.2.2 --depth 1 https://github.com/{VERSION.REPO}.git /root/installer\n"
            "bash install.sh --version v1.2.2\n"
            "stealthnet update --version v1.2.2\n"
            "make update VERSION=v1.2.2\n"
            "python3 deploy/package-release.py --version v1.2.2 --output dist\n"
        )
        # All new guides are discovered, not just a fixed list of today's pages.
        self.write("docs/en/new/installation.md", guide)
        self.write("docs/ru/new/installation.md", guide)
        before = self.read("Cargo.lock")
        self.assertEqual(self.run_tool("--check"), 1)
        self.assertEqual(self.read("Cargo.lock"), before)
        self.assertEqual(self.read("docs/en/new/installation.md"), guide)
        self.assertEqual(self.run_tool("--sync"), 0)
        self.assertEqual(self.read("docs/en/new/installation.md"), guide.replace("v1.2.2", "v1.2.3"))
        self.assertEqual(self.read("docs/ru/new/installation.md"), guide.replace("v1.2.2", "v1.2.3"))
        self.assertIn('name = "external"\nversion = "1.2.2"', self.read("Cargo.lock"))
        self.assertEqual(VERSION.planned_changes(self.root), ("1.2.3", {}))
        self.assertEqual(self.run_tool("--check"), 0)

    def test_badges_urls_and_installer_pins_follow_workspace_bump(self):
        self.write("README.md", '**v1.2.2 · Description**\n<img alt="Download v1.2.2">\n'
                   f"https://github.com/{VERSION.REPO}/releases/tag/v1.2.2\n")
        self.write("README.ru.md", '**v1.2.2 · Описание**\n<img alt="Скачать v1.2.2">\n')
        self.write("docs/media/navigation/download.svg", '<svg aria-label="Скачать v1.2.2"><title>Скачать v1.2.2</title><text>Скачать v1.2.2</text></svg>')
        self.write("install.sh", "SN_RELEASE_VERSION=latest\n")
        self.write("deploy/other-install.sh", "SN_RELEASE_VERSION=v1.2.2\n")
        self.write("docs/install.md", f"curl https://raw.githubusercontent.com/{VERSION.REPO}/v1.2.2/install.sh\n")
        self.assertEqual(self.run_tool("--sync"), 0)
        for name in ("README.md", "README.ru.md", "docs/media/navigation/download.svg", "docs/install.md", "deploy/other-install.sh"):
            self.assertNotIn("v1.2.2", self.read(name))
            self.assertIn("v1.2.3", self.read(name))
        self.assertEqual(self.read("install.sh"), "SN_RELEASE_VERSION=latest\n")
        self.write("Cargo.toml", '[workspace.package]\nversion = "1.3.0"\n')
        self.assertEqual(self.run_tool("--check"), 1)
        self.assertEqual(self.run_tool("--sync"), 0)
        self.assertIn("v1.3.0", self.read("README.md"))
        self.assertIn("v1.3.0", self.read("docs/media/navigation/download.svg"))
        self.assertIn('name = "sn-core"\nversion = "1.3.0"', self.read("Cargo.lock"))
        self.assertEqual(self.run_tool("--check"), 0)

    def test_preserves_history_dependencies_and_other_projects(self):
        history = "Verified `make update VERSION=v1.2.2` on a clean VM.\n"
        self.write("docs/compatibility.md", history)
        self.write("docs/audit-local/session.md", history)
        prose = ('Fixed since **1.2.2**. Requires agent **1.2.0** or newer.\n'
                 'git clone --branch v1.2.2 https://github.com/other/project.git\n'
                 'https://github.com/other/project/releases/tag/v1.2.2\n')
        self.write("docs/en/install.md", prose)
        self.assertEqual(self.run_tool("--sync"), 0)
        self.assertEqual(self.read("docs/compatibility.md"), history)
        self.assertEqual(self.read("docs/audit-local/session.md"), history)
        self.assertEqual(self.read("docs/en/install.md"), prose)
        self.assertIn('thing = "4.5.6"', self.read("Cargo.toml"))

    def test_invalid_workspace_or_missing_lock_package_fails_before_writing(self):
        before = self.read("Makefile")
        self.write("Cargo.lock", '[[package]]\nname = "other"\nversion = "1.2.2"\n')
        self.assertEqual(self.run_tool("--sync"), 1)
        self.assertEqual(self.read("Makefile"), before)
        self.write("Cargo.toml", '[workspace.package]\nversion = "invalid"\n')
        self.assertEqual(self.run_tool("--check"), 1)


if __name__ == "__main__":
    unittest.main()
