"""Check a bot or play two games against the baseline with swapped seats."""

import argparse
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "bin/arena-runner"
BASELINE = ROOT / "lib/libarena_baseline_bot.so"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("check", "run"))
    parser.add_argument("--out", type=Path, required=True, help="new output directory")
    parser.add_argument("--bot", type=Path, default=ROOT / "submission/libteam.so")
    parser.add_argument("--seed", type=int, default=407)
    args = parser.parse_args()
    try:
        bot = args.bot.resolve(strict=True)
        if args.mode == "run":
            return subprocess.call([
                sys.executable, ROOT / "scripts/league.py", "--runner", RUNNER,
                "--out", args.out, "--seed", str(args.seed), "--matches", "2",
                "--players", "2", "--jobs", "1", bot, BASELINE,
            ])

        result = subprocess.run([
            RUNNER, "--out", args.out, "--seed", str(args.seed),
            "--max-ticks", "2", bot, BASELINE,
        ], capture_output=True, text=True, timeout=10)
        if result.returncode:
            print(result.stderr.rstrip() or result.stdout.rstrip(), file=sys.stderr)
            return 1
        replay_path = args.out / "replay.json"
        replay = json.loads(replay_path.read_text(encoding="utf-8"))
        for frame in replay["frames"][1:]:
            for label, decision in zip(("提交的 Bot", "基准 Bot"), frame["decisions"], strict=True):
                if decision["status"] in ("error", "timeout"):
                    print(f"{label}第 {frame['tick']} 步调用失败（{decision['status']}），"
                          f"详情见 {replay_path}", file=sys.stderr)
                    return 1
        print(f"接入检查通过，裁判已加载并调用两个 Bot。记录见 {replay_path}")
        return 0
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"自测失败：{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
