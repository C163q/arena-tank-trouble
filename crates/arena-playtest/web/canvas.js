const palette = {
  ground: "#f1f2ee",
  wall: "#3c454a",
  player: { main: "#b65430", light: "#e8a37d", dark: "#68341f" },
  target: { main: "#51768b", light: "#acc2cd", dark: "#314c5b" },
};

let latestCanvas;
let latestRound;
let latestTick;

function tank(ctx, pose, colors, geometry) {
  const { tank_length: length, tank_width: width, muzzle_distance: muzzle } = geometry;
  const trackWidth = width * 0.25;
  ctx.save();
  ctx.translate(pose.position.x, pose.position.y);
  ctx.rotate(pose.angle);

  ctx.fillStyle = colors.dark;
  ctx.fillRect(-length / 2, -width / 2, length, width);
  ctx.beginPath();
  ctx.roundRect(0, -width * 0.12, muzzle, width * 0.24, width * 0.05);
  ctx.fill();

  ctx.fillStyle = colors.main;
  ctx.fillRect(-length / 2, -width / 2 + trackWidth, length, width - 2 * trackWidth);
  ctx.fillStyle = colors.light;
  ctx.beginPath();
  ctx.roundRect(-length * 0.20, -width * 0.2, length * 0.45, width * 0.4, width * 0.13);
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

  const thickness = state.geometry.wall_thickness;
  const half = thickness / 2;
  ctx.fillStyle = palette.wall;
  ctx.beginPath();
  for (const { a, b } of state.walls) {
    ctx.rect(Math.min(a.x, b.x) - half, Math.min(a.y, b.y) - half,
      Math.abs(a.x - b.x) + thickness, Math.abs(a.y - b.y) + thickness);
  }
  ctx.fill();

  tank(ctx, state.tanks[0].pose, palette.player, state.geometry);
  tank(ctx, state.tanks[1].pose, palette.target, state.geometry);
  ctx.fillStyle = palette.wall;
  for (const { position: { x, y } } of state.projectiles) {
    ctx.beginPath();
    ctx.arc(x, y, state.geometry.projectile_radius, 0, Math.PI * 2);
    ctx.fill();
  }
}
