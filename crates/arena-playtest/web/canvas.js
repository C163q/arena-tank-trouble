const palette = {
  ground: "#f1f2ee",
  wall: "#3c454a",
  player: { main: "#b65430", light: "#e8a37d", dark: "#68341f" },
  target: { main: "#51768b", light: "#acc2cd", dark: "#314c5b" },
};

let latestCanvas;
let latestRound;
let latestTick;

function tank(ctx, pose, colors) {
  ctx.save();
  ctx.translate(pose.position.x, pose.position.y);
  ctx.rotate(pose.angle);
  ctx.fillStyle = colors.dark;
  ctx.beginPath();
  ctx.roundRect(-0.18, -0.15, 0.36, 0.075, 0.025);
  ctx.roundRect(-0.18, 0.075, 0.36, 0.075, 0.025);
  ctx.roundRect(0.02, -0.036, 0.26, 0.072, 0.016);
  ctx.fill();

  ctx.fillStyle = colors.main;
  ctx.beginPath();
  ctx.roundRect(-0.18, -0.105, 0.35, 0.21, 0.045);
  ctx.fill();
  ctx.fillStyle = colors.light;
  ctx.beginPath();
  ctx.roundRect(-0.075, -0.06, 0.16, 0.12, 0.038);
  ctx.fill();
  ctx.restore();
}

export function render(canvas, state) {
  const ratio = window.devicePixelRatio || 1;
  const width = Math.round(canvas.clientWidth * ratio);
  const height = Math.round(canvas.clientHeight * ratio);
  if (width === 0 || height === 0) return;

  const resized = canvas.width !== width || canvas.height !== height;
  if (!resized && latestCanvas === canvas && latestRound === state.round && latestTick === state.tick) {
    return;
  }
  if (resized) {
    canvas.width = width;
    canvas.height = height;
  }
  latestCanvas = canvas;
  latestRound = state.round;
  latestTick = state.tick;

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const padding = 0.24;
  const scale = Math.min(width / (state.width + padding), height / (state.height + padding));
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.fillStyle = palette.ground;
  ctx.fillRect(0, 0, width, height);
  ctx.setTransform(scale, 0, 0, scale,
    (width - state.width * scale) / 2, (height - state.height * scale) / 2);

  ctx.strokeStyle = palette.wall;
  ctx.lineWidth = 0.08;
  ctx.lineCap = "round";
  ctx.beginPath();
  for (const { a, b } of state.walls) {
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
  }
  ctx.stroke();

  tank(ctx, state.tanks[0].pose, palette.player);
  tank(ctx, state.tanks[1].pose, palette.target);
  ctx.fillStyle = palette.wall;
  for (const { position: { x, y } } of state.projectiles) {
    ctx.beginPath();
    ctx.arc(x, y, 0.035, 0, Math.PI * 2);
    ctx.fill();
  }
}
