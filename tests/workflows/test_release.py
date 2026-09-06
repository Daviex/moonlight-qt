"""Run the actual release workflow scripts against local artifact/CLI fixtures."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile

import yaml


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = yaml.safe_load((ROOT / ".github/workflows/prerelease-builds.yml").read_text())
STEPS = {step["name"]: step for step in WORKFLOW["jobs"]["publish-prerelease"]["steps"]}


class ReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="moonlight-release-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = os.environ.copy()
        self.env.update(CI_VERSION="abc123", BRANCH_NAME="codex/per-game-streaming-settings",
                        GITHUB_SHA="abc123" + "0" * 34, GITHUB_REPOSITORY="test/moonlight",
                        GITHUB_SERVER_URL="https://github.com", GITHUB_STEP_SUMMARY="summary.md")
        self.artifacts = {
            "Moonlight-Windows-x64": ["MoonlightPortable-x64-abc123.zip"],
            "Moonlight-Windows-arm64": ["MoonlightPortable-arm64-abc123.zip"],
            "Moonlight-Windows-installer": ["MoonlightSetup-abc123.exe"],
            "Moonlight-macOS": ["Moonlight-abc123.dmg"],
            "Moonlight-LinuxAppImage": ["Moonlight-abc123-x86_64.AppImage"],
            "Moonlight-SteamLink": ["steamlink/apps/moonlight/appinfo.json", "steamlink/apps/moonlight/bin/moonlight"],
            "zDevDbgSyms-Windows": ["Moonlight-x64.pdb", "Moonlight-arm64.pdb"],
            "zDevDbgSyms-macOS": ["Moonlight.dsym/Contents/Resources/DWARF/Moonlight"],
        }
        for artifact, files in self.artifacts.items():
            for filename in files:
                path = self.artifact_dir(artifact) / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((artifact + "/" + filename).encode())

        bin_path = self.root / "bin"
        bin_path.mkdir()
        shim = bin_path / "gh"
        shim.write_text("""#!/usr/bin/env python
import json, os, pathlib, sys
args = sys.argv[1:]
with open('gh-calls.jsonl', 'a') as log:
    log.write(json.dumps(args) + '\\n')
state = pathlib.Path('release-state')
if args[:2] == ['release', 'view']:
    if not state.exists(): sys.exit(1)
    print('https://github.com/test/moonlight/releases/tag/' + args[2])
elif args[:2] == ['release', 'create']:
    state.write_text('draft')
    if os.environ.get('FAIL_UPLOAD'): sys.exit(1)
elif args[:2] == ['release', 'upload']:
    if os.environ.get('FAIL_UPLOAD'): sys.exit(1)
elif args[:2] == ['release', 'edit']:
    state.write_text('published')
else:
    sys.exit(2)
""", encoding="utf-8", newline="\n")
        shim.chmod(0o755)
        self.env["PATH"] = str(bin_path) + os.pathsep + self.env["PATH"]

    def artifact_dir(self, name):
        return self.root / "dist" / (name + "-abc123")

    def run_step(self, name):
        bash = os.environ.get("BASH", shutil.which("bash"))
        return subprocess.run([bash, "-c", STEPS[name]["run"]], cwd=self.root,
                              env=self.env, capture_output=True, text=True)

    def prepare(self):
        result = self.run_step("Prepare release assets")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def calls(self):
        return [json.loads(line) for line in (self.root / "gh-calls.jsonl").read_text().splitlines()]

    def test_packages_preserve_binaries_archives_and_checksums(self):
        self.prepare()
        assets = self.root / "release-assets"
        self.assertEqual(len(list(assets.iterdir())), 9)
        for name in list(self.artifacts)[:5]:
            filename = self.artifacts[name][0]
            self.assertEqual((assets / filename).read_bytes(), (self.artifact_dir(name) / filename).read_bytes())
        for name in list(self.artifacts)[5:]:
            with zipfile.ZipFile(assets / (name + "-abc123.zip")) as archive:
                for filename in self.artifacts[name]:
                    self.assertEqual(archive.read(filename), (self.artifact_dir(name) / filename).read_bytes())
        checksums = (assets / "SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(checksums), 8)
        for line in checksums:
            expected, filename = line.split(maxsplit=1)
            filename = filename.removeprefix("*")
            self.assertEqual(expected, hashlib.sha256((assets / filename).read_bytes()).hexdigest())

    def test_missing_windows_installer_prevents_release_preparation(self):
        shutil.rmtree(self.artifact_dir("Moonlight-Windows-installer"))
        result = self.run_step("Prepare release assets")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing required artifact: Moonlight-Windows-installer", result.stderr)

    def test_empty_artifact_prevents_release_preparation(self):
        (self.artifact_dir("Moonlight-Windows-x64") / self.artifacts["Moonlight-Windows-x64"][0]).unlink()
        result = self.run_step("Prepare release assets")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Artifact is empty", result.stderr)

    def test_new_release_includes_all_assets_and_is_published_after_upload(self):
        self.prepare()
        result = self.run_step("Publish GitHub prerelease")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        create = next(args for args in calls if args[1] == "create")
        edit = next(args for args in calls if args[1] == "edit")
        self.assertEqual(create[2], "prerelease-codex-per-game-streaming-settings-abc123")
        self.assertIn("Moonlight prerelease codex/per-game-streaming-settings abc123", create)
        self.assertIn("--draft", create)
        self.assertIn("--draft=false", edit)
        self.assertIn("--prerelease", edit)
        self.assertLess(calls.index(create), calls.index(edit))
        self.assertEqual(sum(arg.startswith("release-assets/") for arg in create), 9)
        self.assertEqual((self.root / "release-state").read_text(), "published")
        self.assertIn("https://github.com/test/moonlight/releases/", (self.root / "summary.md").read_text())

    def test_rerun_updates_existing_release_without_creating_another(self):
        self.prepare()
        (self.root / "release-state").write_text("published")
        result = self.run_step("Publish GitHub prerelease")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        self.assertFalse(any(args[1] == "create" for args in calls))
        self.assertTrue(any(args[1] == "upload" and "--clobber" in args for args in calls))

    def test_upload_failure_keeps_new_release_unpublished(self):
        self.prepare()
        self.env["FAIL_UPLOAD"] = "1"
        result = self.run_step("Publish GitHub prerelease")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.root / "release-state").read_text(), "draft")
        self.assertFalse(any(args[1] == "edit" for args in self.calls()))


if __name__ == "__main__":
    unittest.main()
