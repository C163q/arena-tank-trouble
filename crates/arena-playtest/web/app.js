import { render } from "/canvas.js";

const canvas = document.querySelector("canvas");
const status = document.querySelector("#connection-status");
const same = document.querySelector("#restart-same");
const newMap = document.querySelector("#restart-new");
const reconnect = document.querySelector("#reconnect");
const parameters = document.querySelector("#parameters");
const fields = Array.from(parameters.querySelectorAll("input"));
const error = document.querySelector("#error");
const ammo = document.querySelector("#ammo");
const ammoSlots = Array.from(ammo.children);
const targetHits = document.querySelector("#target-hits");
const selfHits = document.querySelector("#self-hits");
const outcome = document.querySelector("#outcome");
const seed = document.querySelector("#seed");

const directions = {
  KeyW: "forward",
  KeyS: "reverse",
  KeyA: "turn_left",
  KeyD: "turn_right",
};
const outcomes = {
  running: "练习中",
  target_hit: "命中静止靶",
  self_hit: "被反弹弹击中",
  draw: "同时命中",
};
const held = new Set();
let state;
let socket;
let restartPending = false;

function send(type, detail = {}) {
  if (!state || socket?.readyState !== WebSocket.OPEN) return false;
  socket.send(JSON.stringify({ type, round: state.round, ...detail }));
  return true;
}

function sendDirections() {
  const input = Object.fromEntries(
    Object.entries(directions).map(([code, name]) => [name, held.has(code)])
  );
  send("input", input);
}

function releaseDirections() {
  if (held.size === 0) return;
  held.clear();
  sendDirections();
}

function showError(message) {
  error.textContent = message;
  error.hidden = !message;
}

function syncButtons() {
  same.disabled = newMap.disabled =
    !state || socket?.readyState !== WebSocket.OPEN || restartPending;
}

function requestRestart(mode) {
  if (restartPending || !parameters.reportValidity()) return;
  const params = Object.fromEntries(fields.map((field) => [field.id, field.valueAsNumber]));
  releaseDirections();
  showError("");
  restartPending = send("restart", { mode, params });
  syncButtons();
}

function applyState(next) {
  const previous = state;
  state = next;
  if (next.round !== previous?.round) {
    restartPending = false;
    held.clear();
    for (const field of fields) field.value = next.params[field.id];
    seed.value = next.seed;
    syncButtons();
    if (previous) canvas.focus({ preventScroll: true });
  }

  const count = next.tanks[0].ammo;
  if (count !== previous?.tanks[0].ammo) {
    ammoSlots.forEach((slot, index) => slot.classList.toggle("loaded", index < count));
    ammo.setAttribute("aria-label", `弹量 ${count} / 5`);
  }
  if (next.target_hits !== previous?.target_hits) targetHits.textContent = next.target_hits;
  if (next.self_hits !== previous?.self_hits) selfHits.textContent = next.self_hits;
  if (next.outcome !== previous?.outcome) outcome.textContent = outcomes[next.outcome];
}

function connect() {
  const endpoint = new URL("/ws", location.href);
  endpoint.protocol = location.protocol === "https:" ? "wss:" : "ws:";
  const connection = new WebSocket(endpoint);
  socket = connection;
  state = undefined;
  restartPending = false;
  status.textContent = "正在连接";
  status.classList.remove("online");
  reconnect.hidden = true;
  showError("");
  syncButtons();

  connection.addEventListener("open", () => {
    status.textContent = "已连接";
    status.classList.add("online");
  });
  connection.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    if (message.type === "state") {
      applyState(message);
    } else if (message.type === "error") {
      restartPending = false;
      showError(message.message);
      syncButtons();
    }
  });
  connection.addEventListener("close", () => {
    held.clear();
    status.textContent = "连接已断开";
    status.classList.remove("online");
    reconnect.hidden = false;
    syncButtons();
  });
}

window.addEventListener("keydown", (event) => {
  const { code, repeat, target } = event;
  if (!(code in directions) && code !== "Space") return;
  if (target instanceof Element && target.closest("input, select, textarea, [contenteditable]")) return;
  if (code === "Space" && target instanceof Element && target.closest("button")) return;
  event.preventDefault();
  if (repeat || restartPending || socket?.readyState !== WebSocket.OPEN || state?.outcome !== "running") return;
  if (code === "Space") {
    send("fire");
  } else {
    held.add(code);
    sendDirections();
  }
});

window.addEventListener("keyup", (event) => {
  if (held.delete(event.code)) sendDirections();
});
window.addEventListener("blur", releaseDirections);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) releaseDirections();
});
parameters.addEventListener("focusin", releaseDirections);
parameters.addEventListener("submit", (event) => event.preventDefault());
same.addEventListener("click", () => requestRestart("same"));
newMap.addEventListener("click", () => requestRestart("new"));
reconnect.addEventListener("click", connect);

function draw() {
  if (state) render(canvas, state);
  requestAnimationFrame(draw);
}

connect();
requestAnimationFrame(draw);
