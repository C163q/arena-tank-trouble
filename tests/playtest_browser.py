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
LASER_SEED = "26532"  # Two lasers ahead in a clear corridor.
SHOTGUN_SEED = "78040"  # Laser then shotgun ahead in the same corridor.


@contextmanager
def running_server(binary, seed=SEED):
    with subprocess.Popen(
        [str(binary), "--port", "0", "--seed", seed],
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

    def wait_for(self, condition, label, timeout=4.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.states and condition(self.states[-1]):
                return self.states[-1]
            self.page.wait_for_timeout(20)
        raise AssertionError(f"did not observe {label}")


def player_pose(state):
    return state["tanks"][0]["pose"]


def ammo(state):
    return state["tanks"][0]["ammo"]


def open_playtest(page, url, seed=SEED):
    wire = Wire(page)
    page.goto(url)
    expect(page.locator("canvas")).to_be_visible(timeout=2500)
    expect(page.locator("#connection-status")).to_have_text("已连接")
    wire.wait_for(lambda s: s["seed"] == seed, "seeded start")
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
        "reload_secs": 3.0,
        "wall_contacts": 10,
        "laser_trail_secs": 0.20,
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


def wait_for_weapon_pixel(page, x, y, kind="laser", snapshot=None):
    page.wait_for_function(
        """async ({x, y, kind, snapshot}) => {
          const live = document.querySelector("canvas");
          let canvas = live;
          if (snapshot) {
            const {render} = await import("/canvas.js");
            canvas = document.createElement("canvas");
            canvas.style.position = "fixed";
            canvas.style.left = "-2000px";
            canvas.style.width = live.clientWidth + "px";
            canvas.style.height = live.clientHeight + "px";
            document.body.append(canvas);
            render(canvas, snapshot);
          }
          const ctx = canvas.getContext("2d");
          const point = new DOMPoint(x, y).matrixTransform(ctx.getTransform());
          const [red, green, blue] = ctx.getImageData(Math.floor(point.x), Math.floor(point.y), 1, 1).data;
          if (canvas !== live) canvas.remove();
          return kind === "shotgun"
            ? red > 145 && red > green + 30 && green > blue + 30
            : green > red + 20 && blue > red + 20;
        }""",
        arg={"x": x, "y": y, "kind": kind, "snapshot": snapshot},
        timeout=500,
    )


def test_laser_pickup_and_fire(page, url, artifacts):
    wire = open_playtest(page, url, LASER_SEED)
    lifetime = page.locator("#laser_trail_secs")
    expect(lifetime).to_have_value("0.2", timeout=2000)
    geometry = wire.states[-1]["geometry"]
    assert geometry["pickup_size"] == geometry["laser_pickup_size"] == 0.32
    first = wire.wait_for(lambda s: len(s["pickups"]) == 2, "first laser batch", timeout=25)
    assert min(s["tick"] for s in wire.states if s["pickups"]) == 900
    assert first["geometry"]["laser_radius"] == 0.030
    assert all(item["kind"] == "laser" for item in first["pickups"])
    marker = first["pickups"][0]
    wait_for_weapon_pixel(page, marker["x"] + 0.12, marker["y"] + 0.12)
    page.screenshot(path=artifacts / "laser-pickups.png")
    assert not first["tanks"][0]["laser_loaded"]

    page.keyboard.down("w")
    try:
        wire.wait_for(lambda s: s["tanks"][0]["laser_loaded"] and len(s["pickups"]) == 1, "first pickup")
        wire.wait_for(lambda s: s["tanks"][0]["laser_loaded"] and not s["pickups"], "second pickup")
    finally:
        page.keyboard.up("w")
    neutral = wire.states[-1]["tick"]
    loaded = wire.wait_for(lambda s: s["tick"] >= neutral + 3, "neutral after pickups")
    position = player_pose(loaded)["position"]
    wait_for_weapon_pixel(page, position["x"] + 0.27, position["y"] + 0.066)
    page.screenshot(path=artifacts / "laser-loaded.png")

    page.keyboard.press("Space")
    laser = wire.wait_for(
        lambda s: s["laser_trails"] and any(p["kind"] == "laser" for p in s["projectiles"]),
        "laser with a visible trail",
    )
    assert not laser["tanks"][0]["laser_loaded"]
    assert ammo(laser) == ammo(loaded)
    assert max(trail["expires_at"] for trail in laser["laser_trails"]) == laser["tick"] + 12

    page.keyboard.press("Space")
    normal = wire.wait_for(
        lambda s: any(p["kind"] == "shell" for p in s["projectiles"]),
        "ordinary shot after the laser",
    )
    assert ammo(normal) == ammo(loaded) - 1
    start = laser["laser_trails"][0]["start"]
    end = laser["laser_trails"][0]["end"]
    # The live first segment may have expired by the time the shell is visible.
    wait_for_weapon_pixel(page, (start["x"] + end["x"]) / 2, (start["y"] + end["y"]) / 2, snapshot=laser)
    page.screenshot(path=artifacts / "laser-trail.png")

    lifetime.fill("0.50")
    page.get_by_role("button", name="同图重开").click()
    reset = wire.wait_for(lambda s: s["round"] == 1, "restart with a new laser lifetime")
    assert reset["params"]["laser_trail_secs"] == 0.50
    assert reset["seed"] == LASER_SEED and reset["walls"] == first["walls"]
    assert not reset["pickups"] and not reset["laser_trails"] and not reset["projectiles"]
    assert not reset["tanks"][0]["laser_loaded"]
    assert reset["tanks"][0]["next_shot"] is None


def test_shotgun_replaces_laser_in_browser(page, wire, artifacts):
    first = wire.wait_for(lambda s: len(s["pickups"]) == 2, "first mixed batch", timeout=25)
    items = sorted(first["pickups"], key=lambda item: -item["x"])
    assert [item["kind"] for item in items] == ["laser", "shotgun"]
    for item in items:
        wait_for_weapon_pixel(page, item["x"] + 0.12, item["y"] + 0.12, kind=item["kind"])
    page.screenshot(path=artifacts / "mixed-pickups.png")

    page.keyboard.down("w")
    try:
        wire.wait_for(lambda s: s["tanks"][0]["next_shot"] == "laser", "laser loaded first")
        wire.wait_for(lambda s: s["tanks"][0]["next_shot"] == "shotgun", "shotgun replaces the laser")
    finally:
        page.keyboard.up("w")
    after_release = wire.states[-1]["tick"]
    ready = wire.wait_for(lambda s: s["tick"] >= after_release + 3, "stationary after pickup")
    assert not ready["pickups"]
    assert not ready["tanks"][0]["laser_loaded"]
    position = player_pose(ready)["position"]
    wait_for_weapon_pixel(page, position["x"] - 0.27, position["y"], kind="shotgun")
    page.screenshot(path=artifacts / "shotgun-loaded.png")

    page.keyboard.press("Space")
    volley = wire.wait_for(lambda s: len(s["projectiles"]) == 3, "three simultaneous pellets")
    assert all(shot["kind"] == "shell" for shot in volley["projectiles"])
    assert ammo(volley) == ammo(ready)
    assert volley["tanks"][0]["next_shot"] is None
    page.keyboard.press("Space")
    next_shot = wire.wait_for(lambda s: len(s["projectiles"]) == 4, "ordinary shot after the volley")
    assert ammo(next_shot) == ammo(ready) - 1
    page.screenshot(path=artifacts / "shotgun-volley.png")


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
        with running_server(options.binary.resolve(), LASER_SEED) as (_, laser_url), running_server(
            options.binary.resolve(), SHOTGUN_SEED
        ) as (_, shotgun_url):
            shotgun_page = browser.new_page(viewport={"width": 1366, "height": 950})
            shotgun_page.on("pageerror", lambda error: errors.append(str(error)))
            shotgun_wire = open_playtest(shotgun_page, shotgun_url, SHOTGUN_SEED)
            laser_page = browser.new_page(viewport={"width": 1366, "height": 950})
            laser_page.on("pageerror", lambda error: errors.append(str(error)))
            test_laser_pickup_and_fire(laser_page, laser_url, options.artifacts)
            shotgun_page.bring_to_front()
            test_shotgun_replaces_laser_in_browser(shotgun_page, shotgun_wire, options.artifacts)
        assert not errors, errors
        browser.close()
    print("Browser playtest: keyboard, hit, refill, restarts, blur, responsive canvas, disconnect, laser and shotgun pickups passed")


if __name__ == "__main__":
    main()
