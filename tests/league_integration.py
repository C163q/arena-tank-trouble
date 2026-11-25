"""Verify the league schedule and run real shared libraries through the CLI."""

import argparse
import collections
import json
from pathlib import Path
import subprocess
import time
import sys
import tempfile

from bot_match import ROOT, fixture

sys.path.insert(0, str(ROOT / "scripts"))
from league import schedule


def verify_schedule():
    bouts = schedule(bot_count=4, players=4, matches=9, seed=407)
    assert bouts == schedule(bot_count=4, players=4, matches=9, seed=407)
    assert [bout.number for bout in bouts] == list(range(1, 10))
    assert bouts[0].seed == 407
    for start in (0, 4):
        group = bouts[start:start + 4]
        assert len({bout.seed for bout in group}) == 1
        for seat in range(4):
            assert {bout.seats[seat] for bout in group} == set(range(4))
    larger = schedule(bot_count=5, players=2, matches=10, seed=407)
    played = collections.Counter(bot for bout in larger for bot in bout.seats)
    assert played == {bot: 4 for bot in range(5)}


def league_cli(binary, out, bots, matches=4):
    return [
        sys.executable, str(ROOT / "scripts/league.py"), "--runner", str(binary),
        "--out", str(out), "--matches", str(matches), "--jobs", "2", "--max-ticks", "100",
        "--seed", "407", *map(str, bots),
    ]


def verify_native_batch(binary, out, work):
    (work / "one").mkdir()
    (work / "two").mkdir()
    shooter = fixture(work / "one", "candidate", kind=6)
    passive = fixture(work / "two", "candidate")
    outcome = subprocess.run(
        league_cli(binary, out, [shooter, passive]), capture_output=True, text=True, timeout=30,
    )
    assert outcome.returncode == 0, (outcome.stdout, outcome.stderr)
    report = json.loads((out / "summary.json").read_text())
    assert report["completed"] == 4 and report["failed"] == 0
    assert report["jobs"] == 2 and report["players"] == 2
    assert (out / "viewer.html").is_file()
    totals = collections.Counter()
    wins = collections.Counter()
    for match in report["matches"]:
        assert (out / match["replay_html"]).is_file()
        assert (out / match["timeline"]).is_file()
        replay = json.loads((out / match["replay_json"]).read_text())
        assert replay["seed"] == match["seed"]
        assert match["scores"] == replay["scores"]
        assert match["result"] == replay["result"]
        for bot, score in zip(match["seats"], match["scores"], strict=True):
            totals[bot] += score["kill_points"] + score["survival_points"]
        if replay["result"]["reason"] == "winner":
            wins[match["seats"][replay["result"]["seat"]]] += 1
    assert wins[0] > 0 and totals[0] >= 3
    for row in report["standings"]:
        bot = row["bot"]
        assert row["played"] == 4 and row["wins"] == wins[bot]
        assert row["total_points"] == totals[bot]
    assert len({bot["source"] for bot in report["bots"]}) == 2
    assert "timeline.jsonl" in (out / "summary.md").read_text()

    broken = work / "broken.so"
    broken.write_bytes(b"not a shared library")
    failed_out = work / "failed"
    result = subprocess.run(
        league_cli(binary, failed_out, [shooter, broken], matches=2),
        capture_output=True, text=True, timeout=30,
    )
    assert result.returncode != 0
    report = json.loads((failed_out / "summary.json").read_text())
    assert report["completed"] == 0 and report["failed"] == 2
    assert all(row["rank"] is None for row in report["standings"])
    assert all((failed_out / match["log"]).is_file() for match in report["matches"])


def verify_interruption(binary, work):
    slow = fixture(work, "slow", kind=1)
    passive = fixture(work, "passive")
    out = work / "interrupted"
    with subprocess.Popen(
        league_cli(binary, out, [slow, passive], matches=7),
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ) as process:
        try:
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if len(list((out / "logs").glob("*.log"))) >= 2:
                    break
                assert process.poll() is None
                time.sleep(0.01)
            else:
                raise AssertionError("parallel games did not start")
            process.terminate()
            assert process.wait(timeout=5) == 130
        finally:
            if process.poll() is None:
                process.kill()
    report = json.loads((out / "summary.json").read_text())
    assert len(report["matches"]) == 7
    assert report["interrupted"] > 0
    assert report["completed"] + report["failed"] + report["interrupted"] == 7


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/arena-runner")
    parser.add_argument("--artifacts", type=Path, required=True)
    args = parser.parse_args()
    verify_schedule()
    with tempfile.TemporaryDirectory(prefix="arena-league-native-") as directory:
        verify_native_batch(args.binary.resolve(), args.artifacts.resolve(), Path(directory))
        verify_interruption(args.binary.resolve(), Path(directory))
    print("League: deterministic rotations, parallel native matches, standings and failures passed")


if __name__ == "__main__":
    main()
