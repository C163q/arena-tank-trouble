const canvas = document.querySelector("canvas");
const match = document.querySelector("#match");
const play = document.querySelector("#play");
const seek = document.querySelector("#seek");
const time = document.querySelector("#time");
const speed = document.querySelector("#speed");
const scoreboard = document.querySelector("#scoreboard");
const fileStatus = document.querySelector("#file-status");
const input = document.querySelector("#open-replay");
const dropZone = document.querySelector(".drop-zone");
const dropOverlay = document.querySelector("#drop-overlay");
let replay;
let position = 0;
let previous = null;
let animation = 0;
let loadId = 0;
let dragDepth = 0;

function decodeFrame(record) {
  const { tick, tanks, projectiles, pickups, laser_trails, safe_zone } = record;
  const [minX, minY, maxX, maxY] = safe_zone ?? [];
  return {
    tick,
    tanks: tanks.map(([x, y, angle, alive, ammo, refill_at, next_shot]) =>
      ({ pose: { position: { x, y }, angle }, alive, ammo, refill_at, next_shot })),
    projectiles: projectiles.map(([owner, kind, x, y, vx, vy, wall_contacts]) =>
      ({ owner, kind, position: { x, y }, velocity: { x: vx, y: vy }, wall_contacts })),
    pickups: pickups.map(([kind, x, y]) => ({ kind, x, y })),
    laser_trails: laser_trails.map(([owner, x1, y1, x2, y2, expires_at]) =>
      ({ owner, start: { x: x1, y: y1 }, end: { x: x2, y: y2 }, expires_at })),
    safe_zone: safe_zone == null ? null : { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } },
  };
}

function parseReplay(text) {
  try {
    return JSON.parse(text);
  } catch (error) {
    if (!(error instanceof SyntaxError)) throw error;
  }
  const lines = text.split(/\r?\n/);
  const header = JSON.parse(lines.shift());
  if (header?.type !== "arena_timeline" || header.format_version !== 1) {
    throw new Error("未知的回放格式或版本");
  }
  const footer = JSON.parse(lines.pop());
  if (footer?.type !== "result" || footer.tick !== lines.length - 1) {
    throw new Error("逐步记录缺少完整结算");
  }
  return {
    ...header,
    frames: lines.map((line) => decodeFrame(JSON.parse(line))),
    scores: footer.scores,
    result: footer.result,
  };
}

const positive = (value) => Number.isFinite(value) && value > 0;
const nonnegative = (value) => Number.isSafeInteger(value) && value >= 0;
const point = (p) => p && Number.isFinite(p.x) && Number.isFinite(p.y);

function validZone(zone, { width, height }) {
  return zone == null || (point(zone.min) && point(zone.max)
    && zone.min.x >= 0 && zone.min.y >= 0 && zone.max.x <= width && zone.max.y <= height
    && zone.min.x <= zone.max.x && zone.min.y <= zone.max.y);
}

function validFrame(frame, tick, seats, map) {
  return frame?.tick === tick && Array.isArray(frame.tanks) && frame.tanks.length === seats
    && frame.tanks.every((tank) => point(tank?.pose?.position) && Number.isFinite(tank.pose.angle)
      && typeof tank.alive === "boolean"
      && [null, "laser", "shotgun"].includes(tank.next_shot))
    && Array.isArray(frame.projectiles)
    && frame.projectiles.every((shot) => point(shot?.position) && ["shell", "laser"].includes(shot.kind))
    && Array.isArray(frame.pickups)
    && frame.pickups.every((pickup) => point(pickup) && ["laser", "shotgun"].includes(pickup.kind))
    && Array.isArray(frame.laser_trails)
    && frame.laser_trails.every((trail) => point(trail?.start) && point(trail.end))
    && validZone(frame.safe_zone, map);
}

function validate(data) {
  if (data?.format_version !== 1) {
    throw new Error("未知的回放格式或版本");
  }
  const { map, geometry, seats, scores, frames, result } = data;
  if (!Array.isArray(seats) || seats.length < 2 || seats.length > tankColors.length
      || !seats.every((name) => typeof name === "string")
      || !positive(data.ticks_per_second) || typeof data.seed !== "string"
      || !positive(map?.width) || !positive(map?.height) || !Array.isArray(map.walls)
      || !map.walls.every((wall) => point(wall?.a) && point(wall.b))
      || !["tank_length", "tank_width", "muzzle_distance", "projectile_radius",
        "wall_thickness", "pickup_size", "laser_radius"].every((name) => positive(geometry?.[name]))
      || !Array.isArray(scores) || scores.length !== seats.length
      || !scores.every((score) => nonnegative(score?.kill_points) && nonnegative(score.survival_points))
      || !["winner", "time_limit", "all_eliminated"].includes(result?.reason)
      || (result.reason === "winner" && !(nonnegative(result.seat) && result.seat < seats.length))
      || !Array.isArray(frames) || frames.length === 0
      || !frames.every((frame, tick) => validFrame(frame, tick, seats.length, map))) {
    throw new Error("回放数据不完整或已损坏");
  }
}

function load(data) {
  validate(data);
  pause();
  replay = data;
  position = 0;
  seek.max = replay.frames.length - 1;
  const seatName = (seat) => (seat + 1) + " 号 · " + replay.seats[seat];
  const end = replay.result;
  document.querySelector("#result").textContent =
    (end.reason === "winner" ? seatName(end.seat) + " 获胜"
      : end.reason === "time_limit" ? "到达时限 · 平局" : "全灭 · 平局")
    + "　种子 " + replay.seed;

  scoreboard.replaceChildren();
  replay.scores.forEach(({ kill_points, survival_points }, seat) => {
    const row = scoreboard.insertRow();
    const cell = row.insertCell();
    const color = document.createElement("span");
    color.className = "color";
    color.style.backgroundColor = tankColors[seat].main;
    cell.append(color, seatName(seat));
    for (const points of [kill_points, survival_points, kill_points + survival_points]) {
      row.insertCell().textContent = points;
    }
  });
  match.hidden = false;
  dropZone.classList.add("has-replay");
  show();
}

function show() {
  if (!replay) return;
  const tick = Math.floor(position);
  const last = replay.frames.length - 1;
  seek.value = tick;
  time.textContent = (tick / replay.ticks_per_second).toFixed(1) + " / "
    + (last / replay.ticks_per_second).toFixed(1) + " 秒";
  // A different match can have the same tick and canvas size.
  const { tanks, projectiles, pickups, laser_trails, safe_zone } = replay.frames[tick];
  const { width, height, walls } = replay.map;
  render(canvas, {
    round: replay, tick, width, height, walls, geometry: replay.geometry,
    tanks, projectiles, pickups, laser_trails, safe_zone,
  });
}

function pause() {
  cancelAnimationFrame(animation);
  animation = 0;
  previous = null;
  play.textContent = "播放";
}

function advance(now) {
  const last = replay.frames.length - 1;
  if (previous !== null) {
    const steps = ((now - previous) / 1000) * replay.ticks_per_second * Number(speed.value);
    position = Math.min(last, position + steps);
    show();
  }
  previous = now;
  if (position >= last) pause();
  else animation = requestAnimationFrame(advance);
}

play.addEventListener("click", () => {
  if (animation) {
    pause();
    return;
  }
  if (position === replay.frames.length - 1) {
    position = 0;
    show();
  }
  play.textContent = "暂停";
  animation = requestAnimationFrame(advance);
});
seek.addEventListener("input", () => {
  position = Number(seek.value);
  previous = null;
  show();
});
speed.addEventListener("change", () => { previous = null; });
new ResizeObserver(show).observe(canvas);

function setFileStatus(message, error = false) {
  fileStatus.textContent = message;
  fileStatus.classList.toggle("error", error);
}

async function openFile(file) {
  if (!file) return;
  const id = ++loadId;
  setFileStatus("正在读取 " + file.name);
  try {
    const text = (await file.text()).trim();
    if (id !== loadId) return;
    load(parseReplay(text));
    setFileStatus("已载入 " + file.name);
  } catch {
    if (id !== loadId) return;
    setFileStatus("无法载入 " + file.name + "，请检查回放格式和文件完整性。", true);
  }
}

input.addEventListener("change", () => {
  const file = input.files[0];
  input.value = "";
  openFile(file);
});
function hasFiles(transfer) {
  return transfer?.files?.length > 0
    || Array.from(transfer?.types ?? []).includes("Files")
    || Array.from(transfer?.items ?? []).some((item) => item.kind === "file");
}

function hasText(transfer) {
  return Array.from(transfer?.types ?? []).some((type) =>
    ["text/uri-list", "text/plain", "text/html", "text/x-moz-url"].includes(type));
}

function clearDrag() {
  dragDepth = 0;
  dropOverlay.hidden = true;
}

window.addEventListener("dragenter", (event) => {
  if (!hasFiles(event.dataTransfer)) return;
  event.preventDefault();
  dragDepth += 1;
  dropOverlay.hidden = false;
});
window.addEventListener("dragover", (event) => {
  if (hasFiles(event.dataTransfer)) {
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    dropOverlay.hidden = false;
  } else if (hasText(event.dataTransfer)) {
    event.preventDefault();
  }
});
window.addEventListener("dragleave", () => {
  if (dragDepth > 0) dragDepth -= 1;
  if (dragDepth === 0) clearDrag();
});
window.addEventListener("dragend", clearDrag);
window.addEventListener("drop", (event) => {
  clearDrag();
  const transfer = event.dataTransfer;
  if (!hasFiles(transfer) && !hasText(transfer)) return;
  event.preventDefault();
  const file = transfer.files?.[0]
    ?? Array.from(transfer.items ?? []).find((item) => item.kind === "file")?.getAsFile();
  if (file) {
    openFile(file);
  } else {
    ++loadId;
    setFileStatus("请从文件管理器拖入回放文件，或点击「选择文件」。", true);
  }
});

const embedded = document.querySelector("#replay-data");
if (embedded) {
  load(parseReplay(embedded.textContent));
}

setFileStatus("");
