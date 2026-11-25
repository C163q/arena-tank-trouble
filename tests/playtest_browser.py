"""Run the browser against the locally built arena-playtest executable."""

import argparse
import json
import os
import select
import shutil
import subprocess
import time
from collections import deque
from contextlib import contextmanager
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SEED = "407"  # The default facing has a clear shot from spawn to the target.


@contextmanager
def running_server(binary):
    with subprocess.Popen(
        [str(binary), "--port", "0", "--seed", SEED],
        stdout=subprocess.PIPE,
        text=True,
    ) as server:
        try:
            ready, _, _ = select.select([server.stdout], [], [], 5)
            assert ready, "local server did not start"
            url = server.stdout.readline().strip()
            assert url.startswith("http://127.0.0.1:"), f"invalid server URL {url!r}"
            yield server, url
        finally:
            if server.poll() is None:
                server.terminate()
                try:
                    server.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    server.kill()


class Wire:
    def __init__(self, page):
        self.page = page
        self.states = deque(maxlen=180)
        self.commands = []
        page.on("websocket", self.observe)

    def observe(self, ws):
        ws.on("framereceived", self.receive)
        ws.on("framesent", lambda payload: self.commands.append(json.loads(payload)))

    def receive(self, payload):
        message = json.loads(payload)
        if message["type"] == "state":
            self.states.append(message)

    def wait_for(self, condition, label):
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            if self.states and condition(self.states[-1]):
                return self.states[-1]
            self.page.wait_for_timeout(20)
        raise AssertionError(f"did not observe {label}")


def player_pose(state):
    return state["tanks"][0]["pose"]


def ammo(state):
    return state["tanks"][0]["ammo"]


def open_playtest(page, url):
    wire = Wire(page)
    page.goto(url)
    expect(page.locator("canvas")).to_be_visible(timeout=2500)
    expect(page.locator("#connection-status")).to_have_text("已连接")
    wire.wait_for(lambda s: s["seed"] == SEED, "seeded start")
    page.wait_for_function(
        """() => {
          const canvas = document.querySelector("canvas");
          const pixel = canvas.getContext("2d").getImageData(canvas.width >> 1, canvas.height >> 1, 1, 1);
          return pixel.data[3] === 255;
        }"""
    )
    return wire


def test_keyboard_hit_and_restart(page, url):
    wire = open_playtest(page, url)
    initial = wire.states[-1]
    assert initial["params"] == {
        "forward_secs": 0.45,
        "reverse_secs": 0.45,
        "turn_secs": 1.5,
        "reload_secs": 5.0,
        "wall_contacts": 10,
    }
    assert initial["geometry"]["tank_length"] == 0.42
    assert initial["geometry"]["tank_width"] == 0.34
    assert initial["geometry"]["projectile_radius"] == 0.042
    assert initial["geometry"]["wall_thickness"] == 0.08
    assert page.evaluate(
        """async (state) => {
          const { render } = await import("/canvas.js");
          const canvas = document.createElement("canvas");
          canvas.style.cssText = "position:fixed;left:-2000px;width:1000px;height:1000px";
          document.body.append(canvas);
          render(canvas, {
            ...state, width: 3, height: 3,
            walls: [{a: {x: 1, y: 1}, b: {x: 2, y: 1}}],
            tanks: state.tanks.map((tank) => ({...tank, pose: {position: {x: 0.4, y: 0.4}, angle: 0}})),
          });
          const ctx = canvas.getContext("2d");
          const transform = ctx.getTransform();
          const colorAt = (worldX, worldY) => {
            const {x, y} = new DOMPoint(worldX, worldY).matrixTransform(transform);
            return [...ctx.getImageData(Math.floor(x), Math.floor(y), 1, 1).data].join(",");
          };
          // This point is inside the square cap, outside the corresponding round cap.
          const corner = colorAt(2.034, 1.034);
          const isWall = corner === colorAt(2, 1) && corner !== colorAt(2.07, 1.07);
          canvas.remove();
          return isWall;
        }""",
        initial,
    )
    spawn = player_pose(initial)
    page.keyboard.down("w")
    moved = wire.wait_for(lambda s: player_pose(s)["position"] != spawn["position"], "forward movement")
    page.keyboard.up("w")
    page.keyboard.down("a")
    left = wire.wait_for(lambda s: player_pose(s)["angle"] < 0, "left turn")
    page.keyboard.up("a")
    page.keyboard.down("d")
    wire.wait_for(lambda s: player_pose(s)["angle"] > left["tanks"][0]["pose"]["angle"], "right turn")
    page.keyboard.up("d")
    page.keyboard.down("s")
    wire.wait_for(
        lambda s: player_pose(s)["position"]["x"] < player_pose(moved)["position"]["x"],
        "reverse movement",
    )
    page.keyboard.up("s")

    page.locator("#reload_secs").fill("0.25")
    page.locator("canvas").focus()
    tick_before_key = wire.states[-1]["tick"]
    page.keyboard.down("w")
    wire.wait_for(lambda s: s["tick"] >= tick_before_key + 3, "held input before restart")
    assert wire.commands[-1]["type"] == "input" and wire.commands[-1]["forward"]
    page.get_by_role("button", name="同图重开").click()
    restarted = wire.wait_for(lambda s: s["round"] == 1 and s["tick"] >= 4, "same-map restart")
    assert restarted["seed"] == SEED
    assert restarted["walls"] == initial["walls"]
    assert restarted["tanks"] == initial["tanks"]
    assert restarted["params"]["reload_secs"] == 0.25
    page.keyboard.down("w")  # A repeat for the key held before restart.
    settled = wire.wait_for(lambda s: s["tick"] >= restarted["tick"] + 4, "steps after repeat")
    assert settled["tanks"] == initial["tanks"]
    page.keyboard.up("w")
    page.keyboard.down("w")
    wire.wait_for(lambda s: player_pose(s)["position"] != spawn["position"], "movement after re-press")
    page.keyboard.up("w")
    page.get_by_role("button", name="同图重开").click()
    wire.wait_for(lambda s: s["round"] == 2, "return to shooting position")

    page.keyboard.down("Space")
    wire.wait_for(lambda s: ammo(s) == 4, "shot consuming one round")
    for _ in range(3):
        page.keyboard.down("Space")
    page.keyboard.up("Space")
    assert sum(message["type"] == "fire" for message in wire.commands) == 1
    wire.wait_for(lambda s: ammo(s) == 5 and s["outcome"] == "running", "magazine refill")
    complete = wire.wait_for(lambda s: s["outcome"] == "target_hit", "target hit")
    assert complete["target_hits"] == 1
    expect(page.locator("#outcome")).to_have_text("命中静止靶")
    expect(page.locator("#target-hits")).to_have_text("1")
    page.keyboard.press("d")
    assert wire.states[-1]["tick"] == complete["tick"]
    page.get_by_role("button", name="换图重开").click()
    next_map = wire.wait_for(lambda s: s["round"] == 3, "new-map restart")
    assert next_map["seed"] != SEED
    assert next_map["target_hits"] == next_map["self_hits"] == 0
    assert not page.evaluate("document.documentElement.scrollWidth > innerWidth")


def test_blur_releases_input(page, url):
    wire = open_playtest(page, url)
    start = wire.states[-1]
    page.keyboard.down("w")
    moving = wire.wait_for(
        lambda s: player_pose(s)["position"] != player_pose(start)["position"], "held movement"
    )
    page.evaluate("window.dispatchEvent(new Event('blur'))")
    stopped = wire.wait_for(lambda s: s["tick"] >= moving["tick"] + 4, "steps after blur")
    later = wire.wait_for(lambda s: s["tick"] >= stopped["tick"] + 4, "neutral steps after blur")
    assert player_pose(later) == player_pose(stopped)
    page.keyboard.up("w")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/arena-playtest")
    parser.add_argument("--artifacts", type=Path, default=ROOT / ".tmp")
    options = parser.parse_args()
    options.artifacts.mkdir(parents=True, exist_ok=True)
    browser_path = os.environ.get("ARENA_CHROMIUM") or shutil.which("chromium")

    with running_server(options.binary.resolve()) as (server, url), sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=browser_path)
        desktop = browser.new_page(viewport={"width": 1366, "height": 950})
        errors = []
        desktop.on("pageerror", lambda error: errors.append(str(error)))
        test_keyboard_hit_and_restart(desktop, url)
        desktop.screenshot(path=options.artifacts / "playtest-desktop.png")
        mobile = browser.new_page(viewport={"width": 390, "height": 844}, device_scale_factor=2)
        mobile.on("pageerror", lambda error: errors.append(str(error)))
        test_blur_releases_input(mobile, url)
        assert not mobile.evaluate("document.documentElement.scrollWidth > innerWidth")
        mobile.screenshot(path=options.artifacts / "playtest-narrow.png", full_page=True)
        server.terminate()
        server.wait(timeout=5)
        for page in [desktop, mobile]:
            expect(page.locator("#connection-status")).to_have_text("连接已断开")
            expect(page.get_by_role("button", name="重新连接")).to_be_enabled()
        assert not errors, errors
        browser.close()
    print("Browser playtest: keyboard, hit, refill, restarts, blur, responsive canvas and disconnect passed")


if __name__ == "__main__":
    main()
