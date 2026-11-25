"""Build and publish the Linux x86_64 tank contest kit."""

import argparse
import ctypes
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[1]
TEMPLATES = ROOT / "packaging/contest"
TARGET = "x86_64-unknown-linux-gnu"
TEMPLATE_FILES = (
    "README.md", "AGENTS.md", "docs/rules.md", "docs/api.md", "docs/testing.md",
    "scripts/selftest.py", "starter/bot.c",
)
BUILD_FILES = ("arena-runner", "libarena_baseline_bot.so", "fff-timer-query", "fff-timer")


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def build(offline):
    if sys.platform != "linux" or platform.machine() != "x86_64" or platform.libc_ver()[0] != "glibc":
        raise ValueError("打包需要 Linux x86_64 GNU/glibc")
    target_dir = ROOT / "target/contest-build"
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(("-C", "opt-level=3", "-C", "debuginfo=0", "-C", "strip=symbols"))
    command = [
        "cargo", "build", "--locked", "--release", "--target", TARGET,
        "-p", "arena-runner", "-p", "arena-baseline-bot", "-p", "fff-session-timer",
    ]
    if offline:
        command.append("--offline")
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    artifacts = target_dir / TARGET / "release"
    versions = set()
    for name in BUILD_FILES:
        artifact = artifacts / name
        sections = subprocess.check_output(["readelf", "--wide", "--sections", artifact], text=True)
        if re.search(r"\.(?:z?debug_\w*|symtab|gnu_debuglink)\s", sections):
            raise ValueError(f"构建产物仍带有调试节或普通符号表：{name}")
        info = subprocess.check_output(["readelf", "--version-info", artifact], text=True)
        versions.update(re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", info))
    return artifacts, max(versions, key=lambda value: tuple(map(int, value.split("."))))


def copy(source, destination, executable=False):
    if not source.is_file() or source.resolve() != source:
        raise ValueError(f"打包文件必须是普通文件：{source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def publish(stage, output):
    # An exclusive rename prevents a late writer from replacing another kit.
    libc = ctypes.CDLL(None, use_errno=True)
    rename = libc.renameat2
    rename.argtypes = (ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint)
    rename.restype = ctypes.c_int
    at_fdcwd, rename_noreplace = -100, 1
    if rename(at_fdcwd, os.fsencode(stage), at_fdcwd, os.fsencode(output), rename_noreplace) != 0:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), output)


def assemble(output, artifacts, glibc_min):
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".contest-kit-", dir=output.parent) as temporary:
        stage = Path(temporary)
        participant = stage / "arena-contest"
        deliverable = stage / "kit"
        for name in TEMPLATE_FILES:
            copy(TEMPLATES / "participant" / name, participant / name)
        for name in ("include/arena_bot.h", "viewer.html", "scripts/league.py", "LICENSE"):
            copy(ROOT / name, participant / name)
        for name in ("arena-runner", "fff-timer-query"):
            copy(artifacts / name, participant / "bin" / name, executable=True)
        copy(artifacts / "libarena_baseline_bot.so", participant / "lib/libarena_baseline_bot.so")
        copy(artifacts / "fff-timer", deliverable / "fff-timer", executable=True)
        copy(ROOT / "LICENSE", deliverable / "LICENSE")
        files = {
            path.relative_to(participant).as_posix(): {
                "sha256": sha256(path),
                "mode": path.stat().st_mode & 0o777,
            }
            for path in sorted(participant.rglob("*")) if path.is_file()
        }
        manifest = {
            "kit_format": "arena-contest",
            "kit_version": 1,
            "abi_version": 1,
            "target": TARGET,
            "glibc_min": glibc_min,
            "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "source_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
            "files": files,
        }
        (participant / "manifest.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8",
        )
        archive = deliverable / "participant.tar.gz"
        with archive.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as zipped:
            with tarfile.open(fileobj=zipped, mode="w") as tar:
                for name in sorted((*files, "manifest.json")):
                    file = participant / name
                    info = tar.gettarinfo(str(file), arcname=f"arena-contest/{name}")
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ""
                    info.mode = 0o644 if name == "manifest.json" else files[name]["mode"]
                    with file.open("rb") as stream:
                        tar.addfile(info, stream)
        organizer = (TEMPLATES / "organizer/README.md").read_text(encoding="utf-8")
        organizer = organizer.replace("@ARCHIVE_SHA256@", sha256(archive))
        organizer = organizer.replace("@TIMER_SHA256@", sha256(deliverable / "fff-timer"))
        (deliverable / "README.md").write_text(organizer, encoding="utf-8")
        publish(deliverable, output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "dist/contest-kit")
    parser.add_argument("--offline", action="store_true", help="build using Cargo's local cache")
    args = parser.parse_args()
    output = args.out.absolute()
    try:
        if os.path.lexists(output):
            raise ValueError(f"输出目录已存在：{output}")
        artifacts, glibc_min = build(args.offline)
        assemble(output, artifacts, glibc_min)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"题包生成失败：{error}\n")
    print(output / "participant.tar.gz")


if __name__ == "__main__":
    main()
