#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Unit tests for tools/mol-harness (stdlib unittest only).

Run: python3 tools/mol_harness_tests.py
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
HARNESS = [sys.executable, str(HERE / "mol-harness")]


def run_tool(*args):
    return subprocess.run([*HARNESS, *args], capture_output=True, text=True)


def write_json(path, obj):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj))
    return str(path)


GOOD_DEF = {
    "id": "t/self",
    "command": ["/bin/echo", "hi"],
    "platforms": ["mac", "ubuntu"],
    "timeout_s": 60,
    "artifacts": ["stdout", "stderr", "exit"],
    "normalize": ["strip-timing", "strip-paths", "sort-lines"],
    "baseline": "per-platform",
}


def artifacts(leg, **over):
    doc = {"id": "t/self", "leg": leg, "argv": ["/bin/echo", "hi"],
           "platform": "darwin" if leg == "mac" else "linux",
           "normalize": ["strip-timing", "strip-paths", "sort-lines"],
           "stdout": "hi\n", "stderr": "", "exit": 0, "timeout": False,
           "duration_ms": 5 if leg == "mac" else 42,
           "trace_backends": {"exec": "ok"}}
    doc.update(over)
    return doc


class ValidateDefTests(unittest.TestCase):
    def check(self, mutate):
        with tempfile.TemporaryDirectory() as tmp:
            d = dict(GOOD_DEF)
            mutate(d)
            p = write_json(Path(tmp) / "def.json", d)
            return run_tool("validate-def", p)

    def test_good_def_valid(self):
        r = self.check(lambda d: None)
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_rejects_string_command(self):
        r = self.check(lambda d: d.update(command="/bin/echo hi"))
        self.assertNotEqual(r.returncode, 0)

    def test_rejects_bad_platform(self):
        r = self.check(lambda d: d.update(platforms=["windows"]))
        self.assertNotEqual(r.returncode, 0)

    def test_rejects_bad_timeout(self):
        r = self.check(lambda d: d.update(timeout_s=0))
        self.assertNotEqual(r.returncode, 0)

    def test_rejects_unknown_artifact(self):
        r = self.check(lambda d: d.update(artifacts=["stdout", "video"]))
        self.assertNotEqual(r.returncode, 0)


class CompareTests(unittest.TestCase):
    def compare(self, mac_doc, linux_doc, prior=None):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "mac").mkdir()
            (tmp / "linux").mkdir()
            (tmp / "out").mkdir()
            write_json(tmp / "mac" / "artifacts.json", mac_doc)
            write_json(tmp / "linux" / "artifacts.json", linux_doc)
            args = ["compare", str(tmp / "mac"), str(tmp / "linux"),
                    "--out", str(tmp / "out")]
            if prior is not None:
                pp = write_json(tmp / "prior.json", prior)
                args += ["--prior", pp]
            r = run_tool(*args)
            verdict = json.loads((tmp / "out" / "verdict.json").read_text())
            return r.returncode, verdict

    def test_identical_match(self):
        rc, v = self.compare(artifacts("mac"), artifacts("linux"))
        self.assertEqual(rc, 0)
        self.assertEqual(v["verdict"], "MATCH")

    def test_timing_only_diff_still_match(self):
        a = artifacts("mac", duration_ms=1)
        b = artifacts("linux", duration_ms=9999)
        rc, v = self.compare(a, b)
        self.assertEqual(rc, 0)
        self.assertEqual(v["verdict"], "MATCH")

    def test_stdout_diff_diverges(self):
        rc, v = self.compare(artifacts("mac"),
                             artifacts("linux", stdout="other\n"))
        self.assertEqual(rc, 1)
        self.assertEqual(v["verdict"], "DIVERGE")
        self.assertIn("other", v["diff"])

    def test_exit_diff_diverges(self):
        rc, v = self.compare(artifacts("mac"), artifacts("linux", exit=3))
        self.assertEqual(rc, 1)
        self.assertEqual(v["verdict"], "DIVERGE")

    def test_flip_with_prior_is_flake(self):
        rc, v = self.compare(artifacts("mac"), artifacts("linux"),
                             prior={"verdict": "DIVERGE"})
        self.assertEqual(rc, 0)
        self.assertEqual(v["verdict"], "FLAKE")


class RunTests(unittest.TestCase):
    def test_run_trivial_def(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            p = write_json(tmp / "def.json", GOOD_DEF)
            r = run_tool("run", p, "--leg", "mac", "--out", str(tmp / "res"))
            self.assertEqual(r.returncode, 0, r.stderr)
            arts = json.loads((tmp / "res" / "artifacts.json").read_text())
            self.assertEqual(arts["stdout"], "hi\n")
            self.assertEqual(arts["exit"], 0)
            self.assertTrue((tmp / "res" / "results.xml").exists())

    def test_run_rejects_wrong_leg(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            d = dict(GOOD_DEF)
            d["platforms"] = ["mac"]
            p = write_json(tmp / "def.json", d)
            r = run_tool("run", p, "--leg", "ubuntu", "--out", str(tmp))
            self.assertNotEqual(r.returncode, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
