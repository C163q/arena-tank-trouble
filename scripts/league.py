"""Run a seeded series of 2–4-seat tank matches and rank the submitted bots."""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from fractions import Fraction
import hashlib
import html
import json
import os
from pathlib import Path
import random
import signal
import shutil
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class Bout:
    number: int
    seed: int
    seats: tuple[int, ...]

    @property
    def directory(self):
        return f"match-{self.number:04d}"

    def record(self):
        return {"number": self.number, "seed": str(self.seed), "seats": self.seats}


def schedule(bot_count, players, matches, seed):
    rng = random.Random(seed)
    appearances = Counter()
    pairs = Counter()
    bouts = []
    while len(bouts) < matches:
        priority = list(range(bot_count))
        rng.shuffle(priority)
        order = {bot: index for index, bot in enumerate(priority)}
        group = []
        candidates = set(priority)
        for _ in range(players):
            bot = min(candidates, key=lambda item: (
                appearances[item],
                sum(pairs[tuple(sorted((item, other)))] for other in group),
                order[item],
            ))
            candidates.remove(bot)
            group.append(bot)
        map_seed = seed if not bouts else rng.getrandbits(64)
        rotations = min(players, matches - len(bouts))
        for offset in range(rotations):
            seats = tuple(group[offset:] + group[:offset])
            bouts.append(Bout(len(bouts) + 1, map_seed, seats))
        for index, bot in enumerate(group):
            appearances[bot] += rotations
            for other in group[:index]:
                pairs[tuple(sorted((bot, other)))] += rotations
    return bouts


def write_json(path, value):
    with path.open("w", encoding="utf-8") as output:
        json.dump(value, output, indent=2)
        output.write("\n")


def read_result(timeline):
    # The result record has at most four compact scores. Frames can be large.
    with timeline.open("rb") as stream:
        size = stream.seek(0, os.SEEK_END)
        stream.seek(-min(size, 4096), os.SEEK_END)
        footer = json.loads(stream.read().splitlines()[-1])
    if footer.get("type") != "result":
        raise ValueError("missing result record in timeline")
    return footer


def remove_runner_stage(output, pid):
    # A terminated Rust referee cannot run its own replay staging cleanup.
    prefix = f".arena-replay-{pid}-"
    for directory in output.glob(prefix + "*"):
        nonce = directory.name.removeprefix(prefix)
        if nonce.isascii() and nonce.isdecimal() and not directory.is_symlink():
            shutil.rmtree(directory, ignore_errors=True)


def execute(args, bots, bouts):
    output = args.out
    active = set()
    lock = threading.Lock()
    stopped = threading.Event()

    def run(bout):
        name = bout.directory
        log_path = f"logs/{name}.log"
        record = {**bout.record(), "log": log_path}
        command = [
            str(args.runner), "--out", str(output / name), "--seed", str(bout.seed),
        ]
        for option in ("max_ticks", "kill_points", "survival_points"):
            value = getattr(args, option)
            if value is not None:
                command.extend(("--" + option.replace("_", "-"), str(value)))
        command.extend(bots[bot]["source"] for bot in bout.seats)

        try:
            with (output / log_path).open("wb") as log:
                with lock:
                    if stopped.is_set():
                        return {**record, "status": "interrupted"}
                    process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
                    active.add(process)
                try:
                    code = process.wait()
                finally:
                    with lock:
                        active.discard(process)
            if code != 0:
                remove_runner_stage(output, process.pid)
                cancelled = stopped.is_set() or code in (-signal.SIGINT, -signal.SIGTERM)
                status = "interrupted" if cancelled else "failed"
                return {**record, "status": status, "exit_code": code}
            timeline = f"{name}/timeline.jsonl"
            footer = read_result(output / timeline)
            return {
                **record, "status": "completed", "tick": footer["tick"],
                "scores": footer["scores"], "result": footer["result"],
                "replay_html": f"{name}/replay.html", "replay_json": f"{name}/replay.json",
                "timeline": timeline,
            }
        except (OSError, ValueError, KeyError, IndexError) as error:
            return {**record, "status": "failed", "error": str(error)}

    pool = ThreadPoolExecutor(max_workers=min(args.jobs, len(bouts)))
    futures = []
    try:
        for bout in bouts:
            futures.append(pool.submit(run, bout))
        for future in as_completed(futures):
            result = future.result()
            print(f"{result['number']}/{len(bouts)} {result['status']}", flush=True)
    except KeyboardInterrupt:
        with lock:
            stopped.set()
            processes = list(active)
        for process in processes:
            process.terminate()
        deadline = time.monotonic() + 2
        for process in processes:
            try:
                process.wait(timeout=max(0, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                process.kill()
    finally:
        pool.shutdown(wait=True, cancel_futures=stopped.is_set())
    results = []
    for index, bout in enumerate(bouts):
        if index >= len(futures) or futures[index].cancelled():
            results.append({**bout.record(), "status": "interrupted"})
        else:
            results.append(futures[index].result())
    return results


def rank(bots, matches):
    rows = [
        {"bot": bot["id"], "played": 0, "wins": 0, "kill_points": 0, "survival_points": 0}
        for bot in bots
    ]
    for match in matches:
        if match["status"] != "completed":
            continue
        for bot, score in zip(match["seats"], match["scores"], strict=True):
            row = rows[bot]
            row["played"] += 1
            row["kill_points"] += score["kill_points"]
            row["survival_points"] += score["survival_points"]
        result = match["result"]
        if result["reason"] == "winner":
            rows[match["seats"][result["seat"]]]["wins"] += 1

    def average(row):
        return Fraction(row["total_points"], row["played"] or 1)

    for row in rows:
        row["total_points"] = row["kill_points"] + row["survival_points"]
        row["points_per_match"] = float(average(row)) if row["played"] else None
    rows.sort(key=lambda row: (not row["played"], -average(row), row["bot"]))
    previous = None
    for index, row in enumerate(rows, 1):
        row["rank"] = None
        if row["played"]:
            if previous is None or average(row) != average(previous):
                row["rank"] = index
            else:
                row["rank"] = previous["rank"]
            previous = row
    return rows


def write_markdown(path, report):
    def label(bot):
        name = " ".join(report["bots"][bot]["name"].split())
        name = html.escape(name).replace("\\", "\\\\")
        name = name.replace("|", r"\|").replace("[", r"\[").replace("]", r"\]")
        return f'{bot + 1} 号 {name}'

    with path.open("w", encoding="utf-8") as f:
        print("# 坦克 bot 测评\n", file=f)
        print(
            f'完成 {report["completed"]} / {len(report["matches"])} 局，失败 {report["failed"]} 局，'
            f'中断 {report["interrupted"]} 局。并行数 {report["jobs"]}。', file=f,
        )
        print("\n[机器可读汇总](summary.json) · [赛程](plan.json)\n", file=f)
        print("| 名次 | Bot | 出场 | 胜场 | 击杀分 | 存活分 | 总分 | 场均 |", file=f)
        print("| --- | --- | --- | --- | --- | --- | --- | --- |", file=f)
        for row in report["standings"]:
            average = f'{row["points_per_match"]:.3f}' if row["played"] else "—"
            print(
                f'| {row["rank"] or "—"} | {label(row["bot"])} | {row["played"]} | {row["wins"]} '
                f'| {row["kill_points"]} | {row["survival_points"]} | {row["total_points"]} | {average} |',
                file=f,
            )
        print("\n## 各局回放\n", file=f)
        print("| 局 | 种子 | 出生位顺序 | 结果 | 文件 |", file=f)
        print("| --- | --- | --- | --- | --- |", file=f)
        for bout in report["matches"]:
            lineup = "、".join(label(bot) for bot in bout["seats"])
            if bout["status"] == "completed":
                result = bout["result"]
                if result["reason"] == "winner":
                    status = label(bout["seats"][result["seat"]]) + " 获胜"
                else:
                    status = "时限平局" if result["reason"] == "time_limit" else "全灭平局"
                links = f'[回放]({bout["replay_html"]}) · [逐步文本]({bout["timeline"]})'
            else:
                status = "中断" if bout["status"] == "interrupted" else "失败"
                links = f'[日志]({bout["log"]})' if "log" in bout else "—"
            print(f'| {bout["number"]} | {bout["seed"]} | {lineup} | {status} | {links} |', file=f)


def bot_specs(specs):
    bots = []
    for index, argument in enumerate(specs):
        if argument == "demo":
            bots.append({"id": index, "name": "示例", "source": argument})
        else:
            path = Path(argument).resolve(strict=True)
            if not path.is_file():
                raise ValueError(f"bot path is not a regular file: {path}")
            with path.open("rb") as file:
                digest = hashlib.file_digest(file, "sha256").hexdigest()
            name = os.fsencode(path.stem).decode("utf-8", errors="replace")
            bots.append({"id": index, "name": name, "source": str(path), "sha256": digest})
    return bots


def bounded(low, high=None):
    def parse(text):
        value = int(text)
        if value < low or (high is not None and value > high):
            bounds = f"{low}–{high}" if high is not None else f"at least {low}"
            raise argparse.ArgumentTypeError(f"expected {bounds}")
        return value
    return parse


def interrupt(_signal, _frame):
    raise KeyboardInterrupt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bots", nargs="+", help="bot shared libraries or demo")
    parser.add_argument("--out", required=True, type=Path, help="new result directory")
    parser.add_argument("--runner", type=Path, default=ROOT / "target/debug/arena-runner")
    parser.add_argument("--matches", type=bounded(1), help="total games; defaults to the seat count")
    parser.add_argument("--jobs", type=bounded(1), default=1, help="parallel matches")
    parser.add_argument("--players", type=int, choices=(2, 3, 4), help="seats; defaults to at most 4")
    parser.add_argument("--seed", type=bounded(0, (1 << 64) - 1), default=407)
    parser.add_argument("--max-ticks", type=bounded(1, 36000))
    parser.add_argument("--kill-points", type=bounded(0, (1 << 32) - 1))
    parser.add_argument("--survival-points", type=bounded(0, (1 << 32) - 1))
    args = parser.parse_args()
    players = args.players or min(len(args.bots), 4)
    if not 2 <= players <= len(args.bots):
        parser.error("provide at least two bots and enough bots for the chosen seats")
    args.out = args.out.resolve()
    try:
        args.runner = args.runner.resolve(strict=True)
        if not args.runner.is_file() or not os.access(args.runner, os.X_OK):
            parser.error("build arena-runner or provide an executable with --runner")
        bots = bot_specs(args.bots)
        bouts = schedule(len(bots), players, args.matches or players, args.seed)
        args.out.mkdir(parents=True)
        (args.out / "logs").mkdir()
        plan = {
            "format_version": 1, "seed": str(args.seed), "players": players,
            "jobs": min(args.jobs, len(bouts)), "runner": str(args.runner), "bots": bots,
            "options": {name: getattr(args, name) for name in ("max_ticks", "kill_points", "survival_points")},
            "matches": [bout.record() for bout in bouts],
        }
        write_json(args.out / "plan.json", plan)
        signal.signal(signal.SIGTERM, interrupt)
        results = execute(args, bots, bouts)
        counts = Counter(result["status"] for result in results)
        report = {
            **plan, "matches": results, "standings": rank(bots, results),
            "completed": counts["completed"], "failed": counts["failed"],
            "interrupted": counts["interrupted"],
        }
        write_json(args.out / "summary.json", report)
        write_markdown(args.out / "summary.md", report)
        print(args.out / "summary.md")
        if counts["interrupted"]:
            return 130
        return int(bool(counts["failed"]))
    except (OSError, ValueError) as error:
        parser.exit(1, f"{parser.prog}: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
