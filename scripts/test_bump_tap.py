"""Tests for bump-tap.py.

The script is assertion-driven, and these are the assertions. Each test names a
way the release could ship a formula `brew install` refuses — or, worse, one it
accepts while serving the wrong bytes.

Run as a subprocess rather than imported: the script calls `main()` at import
time, and the contract being tested is "what the release workflow invokes",
which is the command line.
"""

import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).with_name("bump-tap.py")

TARGETS = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
]


def sha_for(i: int) -> str:
    return f"{i:064x}"


def sums(version: str, targets=TARGETS, extra: str = "") -> str:
    lines = [
        f"{sha_for(i)}  aval-{version}-{t}.tar.gz" for i, t in enumerate(targets, 1)
    ]
    if extra:
        lines.append(extra)
    return "\n".join(lines) + "\n"


def formula(version: str, targets=TARGETS) -> str:
    body = [
        "class Aval < Formula",
        '  desc "Ask what the current architecture decision is"',
        '  homepage "https://github.com/fredericrous/aval"',
        f'  version "{version}"',
        '  license "MIT"',
        "",
    ]
    for i, t in enumerate(targets, 1):
        body += [
            "  on_macos do",
            f'    url "https://github.com/fredericrous/aval/releases/download/'
            f'v{version}/aval-{version}-{t}.tar.gz"',
            f'    sha256 "{sha_for(100 + i)}"',
            "  end",
        ]
    body += ["end", ""]
    return "\n".join(body)


def run(tmp_path, version, sums_text, formula_text):
    s = tmp_path / "SHA256SUMS"
    f = tmp_path / "aval.rb"
    s.write_text(sums_text)
    f.write_text(formula_text)
    proc = subprocess.run(
        [sys.executable, str(SCRIPT), version, str(s), str(f)],
        capture_output=True,
        text=True,
    )
    return proc, f


def test_rewrites_the_version_and_every_pair(tmp_path):
    proc, f = run(tmp_path, "0.2.0", sums("0.2.0"), formula("0.1.0"))
    assert proc.returncode == 0, proc.stderr
    out = f.read_text()
    assert 'version "0.2.0"' in out
    assert "0.1.0" not in out, "a stale version survived the rewrite"
    for i, t in enumerate(TARGETS, 1):
        assert f"aval-0.2.0-{t}.tar.gz" in out
        assert sha_for(i) in out, f"{t} kept its old checksum"


def test_a_leading_v_on_the_version_is_tolerated(tmp_path):
    proc, f = run(tmp_path, "v0.2.0", sums("0.2.0"), formula("0.1.0"))
    assert proc.returncode == 0, proc.stderr
    assert 'version "0.2.0"' in f.read_text()


def test_is_idempotent(tmp_path):
    """The workflow's 'nothing to commit' path is how a resumed run no-ops."""
    proc, f = run(tmp_path, "0.2.0", sums("0.2.0"), formula("0.1.0"))
    assert proc.returncode == 0
    once = f.read_text()
    proc2, f2 = run(tmp_path, "0.2.0", sums("0.2.0"), once)
    assert proc2.returncode == 0
    assert f2.read_text() == once


def test_refuses_checksums_for_a_different_release(tmp_path):
    """The transcription error that ships the wrong bytes under a version."""
    proc, _ = run(tmp_path, "0.2.0", sums("0.3.0"), formula("0.1.0"))
    assert proc.returncode != 0
    assert "wrong release" in proc.stderr


def test_refuses_a_formula_with_no_version_line(tmp_path):
    broken = formula("0.1.0").replace('  version "0.1.0"\n', "")
    proc, _ = run(tmp_path, "0.2.0", sums("0.2.0"), broken)
    assert proc.returncode != 0
    assert "no version line" in proc.stderr


def test_refuses_a_target_the_release_did_not_publish(tmp_path):
    """A formula asking for a platform the build matrix dropped."""
    short = [t for t in TARGETS if t != "x86_64-unknown-linux-gnu"]
    proc, _ = run(tmp_path, "0.2.0", sums("0.2.0", targets=short), formula("0.1.0"))
    assert proc.returncode != 0
    assert "did not publish it" in proc.stderr


def test_refuses_a_formula_that_does_not_carry_four_pairs(tmp_path):
    """A sed-shaped rewrite would silently do three; this counts."""
    three = formula("0.1.0", targets=TARGETS[:3])
    proc, _ = run(tmp_path, "0.2.0", sums("0.2.0"), three)
    assert proc.returncode != 0
    assert "expected 4 url/sha pairs" in proc.stderr


def test_refuses_a_checksum_file_with_nothing_it_recognises(tmp_path):
    proc, _ = run(tmp_path, "0.2.0", "deadbeef  something-else.tar.gz\n", formula("0.1.0"))
    assert proc.returncode != 0
    assert "no aval checksums parsed" in proc.stderr
