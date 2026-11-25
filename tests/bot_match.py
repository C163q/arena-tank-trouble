"""Run native bot matches and optionally open an exported replay in Chromium."""

import argparse
import json
import os
from pathlib import Path
import resource
import select
import shutil
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]

C_SOURCE = r"""
#define _POSIX_C_SOURCE 200809L
#include "arena_bot.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

uint32_t arena_bot_abi_version(void) {
    return ARENA_BOT_ABI_VERSION + WRONG_ABI;
}

uint32_t arena_bot_decide(const uint8_t *observation, size_t len) {
    if (KIND == 1) {
        const struct timespec delay = {0, 60000000};
        nanosleep(&delay, NULL);
        return ARENA_FIRE;
    }
    if (KIND == 2) {
        return UINT32_MAX;
    }
    if (KIND == 3) {
        abort();
    }
    if (KIND == 7) {
        const pid_t helper = fork();
        if (helper < 0) abort();
        if (helper > 0) {
            printf("helper %ld\n", (long)helper);
            fflush(stdout);
        }
        for (;;) pause();
    }
    if (KIND == 6) {
        static unsigned calls = 0;
        return calls++ == 0 ? ARENA_FIRE : 0;
    }
    if (KIND == 5) {
        printf("%ld\n", (long)getpid());
        fflush(stdout);
        for (;;) pause();
    }
    if (KIND == 4) {
        static unsigned tick = 0;
        char prefix[100];
        snprintf(prefix, sizeof prefix,
                 "{\"tick\":%u,\"world_tick\":%u,\"seat\":0,\"decision_budget_ms\":%u,",
                 tick, tick > 12 ? tick - 12 : 0, tick == 0 ? 100 : 30);
        if (len < strlen(prefix) || memcmp(observation, prefix, strlen(prefix)) != 0) {
            return UINT32_MAX;
        }
        return tick++ == 0 ? ARENA_FIRE : 0;
    }
    return 0;
}
"""


def fixture(directory, name, kind=0, wrong_abi=0):
    library = directory / (name + ".so")
    subprocess.run(
        [
            os.environ.get("CC", "cc"), "-shared", "-fPIC", "-std=c11", "-Wall", "-Wextra",
            "-Werror", "-I", str(ROOT / "include"), f"-DKIND={kind}",
            f"-DWRONG_ABI={wrong_abi}", "-x", "c", "-", "-o", str(library),
        ],
        input=C_SOURCE, text=True, check=True,
    )
    return library


def run_match(binary, out, *bots, max_ticks=3600, seed=407, kill=None, survival=None, check=True):
    command = [
        str(binary), "--out", str(out), "--seed", str(seed), "--max-ticks", str(max_ticks),
    ]
    if kill is not None:
        command.extend(("--kill-points", str(kill)))
    if survival is not None:
        command.extend(("--survival-points", str(survival)))
    command.extend(map(str, bots))
    outcome = subprocess.run(command, text=True, capture_output=True, timeout=20)
    if check:
        assert outcome.returncode == 0, outcome.stderr
        assert (out / "replay.html").is_file()
        return json.loads((out / "replay.json").read_bytes())
    assert outcome.returncode != 0
    return outcome


def verify_referee_exit(binary, path, idle):
    blocked = fixture(path, "blocked", kind=5)
    command = [str(binary), "--out", str(path / "interrupted"), str(blocked), str(idle)]
    with subprocess.Popen(command, text=True, stdout=subprocess.PIPE) as referee:
        worker_handle = None
        try:
            assert select.select([referee.stdout], [], [], 3)[0], "worker did not enter its call"
            worker_handle = os.pidfd_open(int(referee.stdout.readline()))
            referee.kill()
            referee.wait(timeout=3)
            assert select.select([worker_handle], [], [], 2)[0], "worker survived referee shutdown"
        finally:
            if referee.poll() is None:
                referee.kill()
            if worker_handle is not None:
                try:
                    signal.pidfd_send_signal(worker_handle, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                finally:
                    os.close(worker_handle)


def verify_native(binary, path):
    idle = fixture(path, "idle")
    shooter = fixture(path, "observes", kind=4)
    bout = run_match(binary, path / "two", shooter, idle, max_ticks=100, kill=2, survival=3)
    assert bout["result"] == {"reason": "winner", "seat": 0}
    assert bout["scores"] == [
        {"kill_points": 2, "survival_points": 3},
        {"kill_points": 0, "survival_points": 0},
    ]
    [death] = [frame for frame in bout["frames"] if frame["eliminations"]]
    assert death["eliminations"] == [{"tank": 1, "owner": 0}]
    assert death["victory_at"] == death["tick"] + 120 == bout["frames"][-1]["tick"]
    assert all(frame["decisions"][0]["status"] == "ok" for frame in bout["frames"][1:])
    assert bout["frames"][1]["decisions"][0]["action"]["fire"]

    slow = fixture(path, "slow", kind=1)
    invalid = fixture(path, "invalid", kind=2)
    crash = fixture(path, "crash", kind=3)
    bout = run_match(binary, path / "faults", slow, invalid, crash, idle, max_ticks=3)
    for index, frame in enumerate(bout["frames"][1:]):
        actions = frame["decisions"]
        assert actions[0]["status"] == ("ok" if index == 0 else "timeout"), actions[0]
        assert actions[0]["action"]["fire"] == (index == 0)
        assert [seat["status"] for seat in actions[1:]] == ["error", "error", "ok"], actions
        assert all(not any(seat["action"].values()) for seat in actions[1:])
    assert bout["result"] == {"reason": "time_limit"}
    assert all(not score["survival_points"] for score in bout["scores"])

    wrong_abi = fixture(path, "wrong_abi", wrong_abi=1)
    bad = run_match(binary, path / "bad", wrong_abi, idle, check=False)
    assert "unsupported bot ABI version" in bad.stderr
    assert not (path / "bad").exists()
    verify_referee_exit(binary, path, idle)


def verify_failed_export(binary, path):
    def limit_output():
        signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
        resource.setrlimit(resource.RLIMIT_FSIZE, (512, 512))

    out = path / "failed-export"
    command = [str(binary), "--out", str(out), "--max-ticks", "1", "demo", "demo"]
    result = subprocess.run(command, capture_output=True, timeout=5, preexec_fn=limit_output)
    assert result.returncode != 0
    assert not out.exists(), "failed export published a partial file"
    assert not list(path.glob(".arena-replay-*"))


def verify_timeline(path, data):
    with path.open(encoding="utf-8") as text:
        records = [json.loads(line) for line in text]
    header, *frames, result = records
    assert header["type"] == "arena_timeline"
    assert header["map"] == data["map"]
    assert header["seats"] == data["seats"]
    assert result["type"] == "result"
    assert result["result"] == data["result"] and result["scores"] == data["scores"]
    assert len(frames) == len(data["frames"]) == result["tick"] + 1
    for index, (compact, exact) in enumerate(zip(frames, data["frames"])):
        assert compact["tick"] == exact["tick"] == index
        assert compact["victory_at"] == exact["victory_at"]
        assert compact["eliminations"] == [[e["tank"], e["owner"]] for e in exact["eliminations"]]
        assert len(compact["tanks"]) == len(exact["tanks"])
        assert len(compact["projectiles"]) == len(exact["projectiles"])
        assert compact["pickups"] == [[p["kind"], p["x"], p["y"]] for p in exact["pickups"]]
        assert compact["laser_trails"] == [
            [t["owner"], t["start"]["x"], t["start"]["y"], t["end"]["x"], t["end"]["y"], t["expires_at"]]
            for t in exact["laser_trails"]
        ]
        for tank, source in zip(compact["tanks"], exact["tanks"]):
            pose = source["pose"]
            assert tank == [pose["position"]["x"], pose["position"]["y"], pose["angle"],
                            source["alive"], source["ammo"], source["refill_at"], source["next_shot"]]
        for shot, source in zip(compact["projectiles"], exact["projectiles"]):
            assert shot == [source["owner"], source["kind"], source["position"]["x"],
                            source["position"]["y"], source["velocity"]["x"],
                            source["velocity"]["y"], source["wall_contacts"]]
        assert compact["decisions"] == [
            ["".join(key for name, key in zip(
                ("forward", "reverse", "turn_left", "turn_right", "fire"), "WSADF"
            ) if decision["action"][name]) or "-", decision["status"], decision.get("diagnostic")]
            for decision in exact["decisions"]
        ]


def verify_browser(replay_path, screenshot):
    from playwright.sync_api import expect, sync_playwright

    with sync_playwright() as playwright:
        browser_path = os.environ.get("ARENA_CHROMIUM") or shutil.which("chromium")
        browser = playwright.chromium.launch(executable_path=browser_path)
        context = browser.new_context(offline=True, viewport={"width": 1100, "height": 1060})
        page = context.new_page()
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(replay_path.resolve().as_uri())
        seek = page.locator("#seek")
        expect(page.locator("#scoreboard tr")).to_have_count(4)
        expect(seek).to_have_value("0")
        page.wait_for_function(
            """() => {
              const canvas = document.querySelector("canvas");
              const ctx = canvas.getContext("2d");
              return ctx.getImageData(1, 1, 1, 1).data[3] === 255;
            }"""
        )
        page.locator("#speed").select_option("2")
        page.get_by_role("button", name="播放").click()
        page.wait_for_function("() => Number(document.querySelector('#seek').value) >= 6")
        page.get_by_role("button", name="暂停").click()
        frame = seek.input_value()
        page.wait_for_timeout(150)
        expect(seek).to_have_value(frame)
        seek.focus()
        page.keyboard.press("End")
        expect(seek).to_have_value(seek.get_attribute("max"))
        page.keyboard.press("Home")
        expect(seek).to_have_value("0")
        expect(page.locator("#result")).to_contain_text("获胜")
        page.screenshot(path=screenshot, full_page=True)
        viewer_file = ROOT / "viewer.html"
        template = context.new_page()
        template.goto((ROOT / "crates/arena-runner/web/replay.html").as_uri())
        expect(template.locator("#file-status")).to_contain_text("源码模板")
        template.get_by_role("link", name="项目根目录的通用播放器").click()
        expect(template).to_have_url(viewer_file.as_uri())
        template.close()
        blocked = browser.new_context(java_script_enabled=False).new_page()
        blocked.goto(viewer_file.resolve().as_uri())
        expect(blocked.locator("#file-status")).to_contain_text("正在启动播放器")
        blocked.context.close()
        viewer = context.new_page()
        viewer.on("pageerror", lambda error: errors.append(str(error)))
        viewer.goto(viewer_file.resolve().as_uri())
        expect(viewer.locator("#match")).to_be_hidden()
        open_replay = viewer.locator("#open-replay")
        status = viewer.locator("#file-status")
        slider = viewer.locator("#seek")
        viewer.evaluate(
            """() => {
              const dataTransfer = new DataTransfer();
              dataTransfer.setData("text/uri-list", "file:///replay.json");
              document.body.dispatchEvent(new DragEvent("drop", {bubbles: true, cancelable: true, dataTransfer}));
            }"""
        )
        expect(status).to_contain_text("从文件管理器拖入")
        zone = viewer.locator(".drop-zone")
        box = zone.bounding_box()
        assert box["height"] >= 160
        expect(viewer.get_by_text("选择文件", exact=True)).to_be_visible()
        drag = context.new_cdp_session(viewer)

        def drop_file(name, x, y):
            data = {"files": [str(replay_path.with_name(name).resolve())],
                    "items": [], "dragOperationsMask": 1}
            location = {"x": x, "y": y, "data": data}
            for event in ("dragEnter", "dragOver"):
                drag.send("Input.dispatchDragEvent", {"type": event, **location})
            expect(viewer.locator("#drop-overlay")).to_be_visible()
            drag.send("Input.dispatchDragEvent", {"type": "drop", **location})
            expect(viewer.locator("#drop-overlay")).to_be_hidden()

        drop_file("replay.json", box["x"] + 40, box["y"] + 40)
        expect(status).to_have_text("已载入 replay.json")
        expect(viewer.locator("#scoreboard tr")).to_have_count(4)
        starting_canvas = viewer.locator("canvas").evaluate("(canvas) => canvas.toDataURL()")
        slider.focus()
        viewer.keyboard.press("End")
        expect(slider).to_have_value(slider.get_attribute("max"))

        canvas_box = viewer.locator("canvas").bounding_box()
        drop_file("timeline.jsonl", canvas_box["x"] + 40, canvas_box["y"] + 40)
        expect(status).to_have_text("已载入 timeline.jsonl")
        expect(slider).to_have_value("0")
        assert viewer.locator("canvas").evaluate("(canvas) => canvas.toDataURL()") == starting_canvas
        with viewer.expect_file_chooser() as chooser:
            viewer.get_by_text("选择文件", exact=True).click()
        chooser.value.set_files(str(replay_path.with_name("replay.json")))
        expect(status).to_have_text("已载入 replay.json")
        extended = json.loads(replay_path.with_name("replay.json").read_bytes())
        extended["frames"][0]["walls"] = None
        open_replay.set_input_files({
            "name": "extended.json", "mimeType": "application/json",
            "buffer": json.dumps(extended).encode(),
        })
        expect(status).to_have_text("已载入 extended.json")
        assert viewer.locator("canvas").evaluate("(canvas) => canvas.toDataURL()") == starting_canvas
        open_replay.set_input_files({
            "name": "broken.json", "mimeType": "application/json", "buffer": b"{}",
        })
        expect(status).to_contain_text("无法载入 broken.json")
        expect(viewer.locator("#scoreboard tr")).to_have_count(4)
        expect(slider).to_have_value("0")
        assert not errors, errors
        browser.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/arena-runner")
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--browser", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    with tempfile.TemporaryDirectory(prefix="arena-native-") as work:
        verify_native(binary, Path(work))
        verify_failed_export(binary, Path(work))
    data = run_match(binary, args.artifacts, *["demo"] * 4, seed=DEMO_SEED)
    assert data["result"]["reason"] == "winner"
    assert len(data["seats"]) == 4 and data["observation_delay_ticks"] == 12
    assert data["scores"][data["result"]["seat"]]["survival_points"] == 2
    verify_timeline(args.artifacts / "timeline.jsonl", data)
    if args.browser:
        verify_browser(args.artifacts / "replay.html", args.artifacts / "replay-preview.png")
    print("Bot match: real C ABI, timed decisions, kill and survival scoring, four-seat replay passed")


DEMO_SEED = 407


if __name__ == "__main__":
    main()
