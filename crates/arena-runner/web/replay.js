const replay = JSON.parse(document.querySelector("#replay-data").textContent);
const canvas = document.querySelector("canvas");
const play = document.querySelector("#play");
const seek = document.querySelector("#seek");
const time = document.querySelector("#time");
const speed = document.querySelector("#speed");
const scoreboard = document.querySelector("#scoreboard");
const last = replay.frames.length - 1;
let position = 0;
let previous = null;
let animation = 0;

seek.max = last;
const seatName = (seat) => (seat + 1) + " 号 · " + replay.seats[seat];
const end = replay.result;
document.querySelector("#result").textContent =
  (end.reason === "winner" ? seatName(end.seat) + " 获胜"
    : end.reason === "time_limit" ? "到达时限 · 平局" : "全灭 · 平局")
  + "　种子 " + replay.seed;

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

function show() {
  const tick = Math.floor(position);
  seek.value = tick;
  time.textContent = (tick / replay.ticks_per_second).toFixed(1) + " / "
    + (last / replay.ticks_per_second).toFixed(1) + " 秒";
  render(canvas, { ...replay.map, ...replay.frames[tick], geometry: replay.geometry });
}

function pause() {
  cancelAnimationFrame(animation);
  animation = 0;
  previous = null;
  play.textContent = "播放";
}

function advance(now) {
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
  if (position === last) {
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
show();
