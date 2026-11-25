const palette = {
  ground: "#f1f2ee",
  wall: "#3c454a",
  laser: "#087e87",
  shotgun: "#b76b18",
  danger: "#a43b3b57",
  zoneBoundary: "#ab4444",
};

const tankColors = [
  { main: "#b65430", light: "#e8a37d", dark: "#68341f" },
  { main: "#51768b", light: "#acc2cd", dark: "#314c5b" },
  { main: "#6e7841", light: "#c5cd88", dark: "#3b4729" },
  { main: "#896192", light: "#d4a8db", dark: "#513757" },
];

let latestCanvas;
let latestRound;
let latestTick;

function tank(ctx, { pose, next_shot }, colors, geometry) {
  const { tank_length: length, tank_width: width, muzzle_distance: muzzle } = geometry;
  const trackWidth = width * 0.25;
  ctx.save();
  ctx.translate(pose.position.x, pose.position.y);
  ctx.rotate(pose.angle);

  ctx.fillStyle = colors.dark;
  ctx.fillRect(-length / 2, -width / 2, length, width);
  if (next_shot === "laser") {
    const gap = width * 0.15;
    const rail = width * 0.085;
    ctx.fillStyle = palette.laser;
    ctx.fillRect(0, gap, muzzle, rail);
    ctx.fillRect(0, -gap - rail, muzzle, rail);
  } else if (next_shot === "shotgun") {
    const base = length * 0.30;
    ctx.fillStyle = palette.shotgun;
    ctx.fillRect(base, -width * 0.24, muzzle - base, width * 0.48);
    ctx.fillStyle = colors.dark;
    ctx.fillRect(muzzle - 0.024, -width * 0.17, 0.024, width * 0.34);
  } else {
    ctx.beginPath();
    ctx.roundRect(0, -width * 0.12, muzzle, width * 0.24, width * 0.05);
    ctx.fill();
  }

  ctx.fillStyle = colors.main;
  ctx.fillRect(-length / 2, -width / 2 + trackWidth, length, width - 2 * trackWidth);
  ctx.fillStyle = colors.light;
  ctx.beginPath();
  ctx.roundRect(-length * 0.20, -width * 0.2, length * 0.45, width * 0.4, width * 0.13);
  ctx.fill();
  ctx.restore();
}

function drawSafeZone(ctx, { safe_zone, width, height }) {
  if (!safe_zone) return;
  const { min, max } = safe_zone;
  if (min.x === 0 && min.y === 0 && max.x === width && max.y === height) return;

  const safeWidth = max.x - min.x;
  const safeHeight = max.y - min.y;
  ctx.fillStyle = palette.danger;
  ctx.beginPath();
  ctx.rect(0, 0, width, height);
  ctx.rect(min.x, min.y, safeWidth, safeHeight);
  ctx.fill("evenodd");
  if (safeWidth > 0 && safeHeight > 0) {
    ctx.strokeStyle = palette.zoneBoundary;
    ctx.lineWidth = 0.04;
    ctx.strokeRect(min.x, min.y, safeWidth, safeHeight);
  }
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

  drawSafeZone(ctx, state);

  const thickness = state.geometry.wall_thickness;
  const half = thickness / 2;
  ctx.fillStyle = palette.wall;
  ctx.beginPath();
  for (const { a, b } of state.walls) {
    ctx.rect(Math.min(a.x, b.x) - half, Math.min(a.y, b.y) - half,
      Math.abs(a.x - b.x) + thickness, Math.abs(a.y - b.y) + thickness);
  }
  ctx.fill();

  const pickupSize = state.geometry.pickup_size;
  for (const { x, y, kind } of state.pickups) {
    ctx.fillStyle = palette[kind];
    ctx.fillRect(x - pickupSize / 2, y - pickupSize / 2, pickupSize, pickupSize);
    ctx.fillStyle = palette.ground;
    if (kind === "shotgun") {
      for (const offset of [-1, 0, 1]) {
        ctx.beginPath();
        ctx.arc(x + offset * pickupSize * 0.22, y, pickupSize * 0.065, 0, Math.PI * 2);
        ctx.fill();
      }
    } else {
      ctx.fillRect(x - pickupSize / 4, y - pickupSize / 16, pickupSize / 2, pickupSize / 8);
    }
  }

  ctx.strokeStyle = palette.laser;
  ctx.lineWidth = state.geometry.laser_radius * 2;
  ctx.lineCap = "round";
  ctx.beginPath();
  for (const { start, end } of state.laser_trails) {
    ctx.moveTo(start.x, start.y);
    ctx.lineTo(end.x, end.y);
  }
  ctx.stroke();

  state.tanks.forEach((seat, index) => {
    if (seat.alive) tank(ctx, seat, tankColors[index], state.geometry);
  });

  for (const { kind, position: { x, y } } of state.projectiles) {
    const laser = kind === "laser";
    const radius = laser ? state.geometry.laser_radius : state.geometry.projectile_radius;
    ctx.fillStyle = laser ? palette.laser : palette.wall;
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, Math.PI * 2);
    ctx.fill();
  }
}
