"""Verify the published tank contest kit from a clean extraction directory."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile


def run(directory, *command, expected=0):
    result = subprocess.run(command, cwd=directory, capture_output=True, text=True, timeout=60)
    assert result.returncode == expected, (command, result.stdout, result.stderr)
    return result


def verify(kit, out):
    assert {p.name for p in kit.iterdir()} == {
        "README.md", "fff-timer", "participant.tar.gz", "LICENSE",
    }
    instructions = (kit / "README.md").read_text(encoding="utf-8")
    for name in ("fff-timer", "participant.tar.gz"):
        assert hashlib.sha256((kit / name).read_bytes()).hexdigest() in instructions
    out.mkdir(parents=True)
    with tarfile.open(kit / "participant.tar.gz", "r:gz") as archive:
        for member in archive:
            path = Path(member.name)
            assert member.isfile() and path.parts[0] == "arena-contest" and ".." not in path.parts
            assert not member.name.endswith((".rs", ".rlib", "Cargo.toml", "/fff-timer"))
            destination = out / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            with archive.extractfile(member) as source, destination.open("xb") as output:
                output.write(source.read())
            destination.chmod(member.mode)
    work = out / "arena-contest"
    manifest = json.loads((work / "manifest.json").read_text())
    assert manifest["abi_version"] == 1
    assert {str(p.relative_to(work)) for p in work.rglob("*") if p.is_file()} == {
        "manifest.json", *manifest["files"],
    }
    for name, entry in manifest["files"].items():
        path = work / name
        assert hashlib.sha256(path.read_bytes()).hexdigest() == entry["sha256"]
        assert path.stat().st_mode & 0o777 == entry["mode"]
    assert (work / "README.md").exists() and (work / "AGENTS.md").exists()
    assert (work / "bin/fff-timer-query").is_file()
    assert (work / "LICENSE").read_bytes() == (kit / "LICENSE").read_bytes()

    (work / "submission").mkdir()
    run(work, "cc", "-std=c11", "-O2", "-fPIC", "-shared", "-Iinclude",
        "starter/bot.c", "-o", "submission/libteam.so")
    selftest = [sys.executable, "scripts/selftest.py"]
    run(work, *selftest, "check", "--out", "runs/check")
    for name, options in [("baseline", []), ("challenge", ["--opponent", "challenge"])]:
        run(work, *selftest, "run", *options, "--out", f"runs/{name}")
        quick = json.loads((work / f"runs/{name}/summary.json").read_text())
        assert quick["completed"] == 2 and not quick["failed"]
        assert quick["bots"][1]["name"] == f"libarena_{name}_bot"
        first, second = quick["matches"]
        assert first["seed"] == second["seed"]
        assert first["seats"] == second["seats"][::-1]
        for match in quick["matches"]:
            assert (work / f"runs/{name}" / match["replay_html"]).is_file()

    (work / "bad.c").write_text(
        (work / "starter/bot.c").read_text().replace("return 0;", "return UINT32_MAX;")
    )
    run(work, "cc", "-std=c11", "-fPIC", "-shared", "-Iinclude",
        "bad.c", "-o", "bad.so")
    run(work, *selftest, "check", "--bot", "./bad.so", "--out", "runs/invalid", expected=1)
    assert (work / "runs/invalid/replay.json").is_file()
    print(f"Contest kit: manifest, C starter, preflight, rejected action, and both benchmark seat swaps verified in {out}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kit", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    verify(args.kit.resolve(), args.out.resolve())
