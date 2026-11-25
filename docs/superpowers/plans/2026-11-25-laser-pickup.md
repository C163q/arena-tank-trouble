# 激光道具实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**进度**　共 3 项，已完成 0 项。规格已确认，当前待审阅实施计划。余下工作为方形拾取、激光弹道与轨迹、浏览器接入。实施中同步更新本段和任务勾选。

**目标**　玩家能在静止靶试玩中拾取方形道具、发射激光，并通过重开调整致命轨迹的保留时间。

**架构**　`Round` 管理道具、下一发激光和轨迹寿命。激光头和普通弹共用扫掠反弹代码。Canvas 根据 WebSocket 快照绘制，道具和轨迹的碰撞由 Rust 结算。

**技术栈**　现有 Rust 2024、ChaCha8、Serde、Axum、Canvas 2D 和 Python Playwright。

**规格**　[激光道具设计](../specs/2026-11-25-laser-pickup-design.md)

## 通用约束

- 60 模拟步/秒。首次刷新在第 900 步，之后每隔 600 步。每批 1 或 2 件，场上最多 5 件。方块边长 0.32 格。
- 拾取把下一发设为激光。重复拾取照常移除道具，下一发仍为一发激光。激光开火可在普通弹量为零时发生。
- 激光头速度是普通弹的 4 倍，头部和轨迹半径均为 0.042 格，第 5 次触墙或首次命中时终止。每段轨迹默认保留 0.2 秒，可调范围 0.05 至 2.00 秒。
- 复用现有同图重开、设置校验和计分。实现从 `main` 建立任务工作树，完成后按仓库约定合并。

## 优先复核

- 方块与斜置车身仅贴边时，道具仍在场上。见任务 1 的 `square_pickup_requires_area_overlap`。
- 一次刷新若只剩一个空位，最多填一个。满场时跳过该批，下一次按原周期尝试。见任务 1 的 `spawn_caps_batches_and_preserves_cadence`。
- 装填后继续拾取和空弹匣开火不能改变普通弹补充的时间。见任务 2 的 `laser_uses_only_one_pending_shot`。
- 炮管内部的直行段不产生轨迹，贴墙反弹和命中终止的轨迹都截在实际接触点。见任务 2 的 `laser_trail_starts_outside_the_barrel` 和 `laser_records_contacts_and_expires_by_segment`。
- 旧客户端重开消息省略轨迹时长时使用默认值，无效时长保留当前局。见任务 3 的 `restart_validates_and_resets_laser_state`。

## 实施任务

### 任务 1　道具刷新和方形拾取

**文件**　新增 `crates/arena-core/src/pickups.rs`，修改同目录的 `lib.rs` 和 `round.rs`。使用现有 `collision.rs` 的 `Bounds` 和 `hull.rs` 的 `OrientedBox`。测试与对应模块同文件。

**接口**

- `pub const LASER_PICKUP_SIZE: f64 = 0.32`，从 `arena-core` 导出。`Pickups::new(seed: u64) -> Pickups` 初始化独立随机序列与空场地。`Pickups::collect(&mut self, poses: [Pose; 2]) -> [bool; 2]` 返回本步拾取状态。`Pickups::spawn_due(&mut self, tick: u64, maze: &Maze, poses: [Pose; 2])` 处理到期刷新。`positions(&self) -> &[Vec2]` 返回场上的方块中心。
- `TankView` 加入 `laser_loaded: bool`。`RoundView<'a>` 加入 `pickups: &'a [Vec2]`。两个字段的初始值分别为空状态和空列表。

- [ ] 写 `spawn_caps_batches_and_preserves_cadence`。断言 `spawn_due(899, …)` 后列表为空，第 900 步后长度在 1..=2，等种子和操作得到同一位置序列。用预先占用 4 格的场地检查数量在第 900 步达到 5，第 1500 步保持 5。移除两件后在第 1501 步仍为 3，第 2100 步为 4 或 5。检查位置为不同的半整数格心，且方块避开两辆坦克。
- [ ] 写 `square_pickup_requires_area_overlap`。方块中心放在 (2.5, 2.5)，正向车体中心 (2.13, 2.5) 与它贴边时断言 `collect(poses) == [false, false]`，前移 0.01 格则断言为 `[true, false]`。朝向 45° 时取车身中心横坐标 `2.34 - (0.21 + 0.17) / sqrt(2)`、纵坐标 2.5，分别检查车角贴边和进入 0.01 格。若两辆都重叠，检查近者胜出并覆盖等距时给索引 0 的分支。
- [ ] 运行 `cargo test -p arena-core pickups`，确认上述用例在拾取模块和刷新逻辑尚缺时失败。
- [ ] 实现 `Pickups`。使用 `ChaCha8Rng::seed_from_u64(seed ^ 0x9E37_79B9_7F4A_7C15)`，按行列顺序枚举格心，从可用位置随机无放回选取。方块使用 `OrientedBox::axis_aligned`，与车身使用现有严格面积重叠判定。刷新时排除已占用的格心和车身重叠的格心。格心距网格墙线半格，0.32 格方块可容纳在格内。
- [ ] 接入 `Round::step`。每步移动后调用 `collect`，把拾取者的 `laser_loaded` 设为 true。回合仍进行时在步末调用 `spawn_due`。重复拾取只刷新该布尔状态，地面道具仍被移除。
- [ ] 运行 `cargo test -p arena-core` 及 `cargo fmt --all -- --check`，确认通过并提交 `feat: spawn and collect laser pickups`。

### 任务 2　激光头、发射覆盖及轨迹伤害

**文件**　修改 `crates/arena-core/src/round.rs`、`projectile.rs`、`motion.rs` 和 `lib.rs`。轨迹、武器和时间测试放在 `round.rs` 或 `projectile.rs` 对应的现有测试模块。

**接口**

- `Params` 加入 `laser_trail_secs: f64`。默认 0.20，`validate` 接受 0.05..=2.00，沿用旧补弹转换函数的浮点容差后向上取整为步数。Serde 对省略这个字段的输入使用 0.20。
- `ProjectileKind::{Shell, Laser}`，在快照中序列化为 `"shell"` 或 `"laser"`。`Projectile::new(owner: usize, pose: Pose, kind: ProjectileKind) -> Projectile`，每颗弹丸以 `pub(super) kind: ProjectileKind` 保存种类。`Projectile::advance(&mut self, maze: &Maze, tanks: &[Pose; 2], distance: f64, contact_limit: u8, record: impl FnMut(FlightSegment)) -> Option<Impact>`。回调类型 `FlightSegment { start: Vec2, end: Vec2, after_bounce: bool }`。
- `ProjectileView` 加入 `kind: ProjectileKind`。`LaserTrail { start: Vec2, end: Vec2, expires_at: u64 }` 实现 Serde 序列化。`RoundView<'a>` 加入 `laser_trails: &'a [LaserTrail]`。

- [ ] 写 `laser_uses_only_one_pending_shot`。空弹匣且下次补弹尚未到期时让一辆坦克连续拾取两件，断言两件均消失。第一次开火后断言 `kind == Laser`、`ammo == 0`、`laser_loaded == false`，第二次按开火键激光数量仍为 1。到原定补弹步检查弹量变为 1，随后开火得到一发 `Shell` 并扣弹。
- [ ] 写 `laser_trail_starts_outside_the_barrel` 和 `laser_records_contacts_and_expires_by_segment`。开阔区第一段起点距车中心 0.32 格，连续两步激光头位移为 10/60 格。用贴墙发射和障碍夹道覆盖首次反弹留下路径、独立的第 5 次触墙上限、以及首次打中坦克后轨迹终点截在接触点。
- [ ] 写 `lingering_trail_hits_until_its_own_expiry`。在安全测试地图移除已终止的激光头，令坦克走进尚有效的轨迹，断言按现有胜负规则结算，包括发射者。固定轨迹产生步为 100、101，默认过期步为 112、113，断言第 112 步只剩后一段，第 113 步两段均消失。覆盖改变参数为 0.50 秒后单段保留 30 步。使用独立回合验证不同的命中时刻，避免结束冻结干扰过期检查。
- [ ] 运行 `cargo test -p arena-core`，确认武器、轨迹和调参用例失败于相应功能尚缺。
- [ ] 实现共享扫掠回调。每个非零线段都截在最近墙面、坦克接触点或本步终点，回调在反射或提前返回前执行。墙面和坦克时间相同时沿用现有墙面优先规则。普通弹用空回调；激光在炮口内部仅记录已反弹的路径，出膛后的路径全部记录。激光接触上限固定为 5，速度使用普通弹位移的 4 倍。
- [ ] 接入回合结算。`Tank::fire` 优先消费 `laser_loaded` 并创建激光头。回合步开始移除 `expires_at <= tick` 的段，移动后用 `OrientedBox::disc_contact(start, end - start, PROJECTILE_RADIUS)` 检查已有轨迹，新段的当步命中由飞行扫掠处理。所有命中按步合并并沿用现有计分，结束后快照冻结。
- [ ] 运行 `cargo test --workspace`、`cargo fmt --all -- --check` 和 `cargo clippy --workspace --all-targets -- -D warnings`。确认普通弹回归与激光测试通过并提交 `feat: simulate laser shots and damaging trails`。

### 任务 3　WebSocket 快照、拾取画面与试玩验收

**文件**　修改 `crates/arena-playtest/src/session.rs`、`web/index.html`、`web/canvas.js`、`tests/playtest_browser.py`、`README.md`、`docs/development/playtest.md`。浏览器测试扩展现有 Playwright 脚本，保留原有普通弹场景。

**接口**

- 状态快照增加 `pickups: [{x,y}]`、`laser_trails: [{start,end,expires_at}]`、`geometry.laser_pickup_size: 0.32`。复用 `geometry.projectile_radius` 绘制头部和轨迹。`tanks[].laser_loaded`、`projectiles[].kind` 由核心类型序列化。
- 试玩表单加入 `id="laser_trail_secs"` 的数值输入，标签「激光轨迹保留」，范围 0.05–2.00 秒，步进 0.01。沿用表单已有 `field.id` 读取与重开发送方式。`Session::restart` 为无效字段显示对应参数名。

- [ ] 写 `restart_validates_and_resets_laser_state`。断言省略时长使用 0.20，提交 0.50 后新局返回 0.50，提交 0.04 时收到错误且旧快照保持一致。同图重开后 `tick == 0`、道具与轨迹列表为空、两辆车均未载弹，种子仍相同。
- [ ] 写 `test_laser_pickup_and_fire`。从 Rust 的确定性生成结果中选一个首批两件道具都可沿出生通道拾取且拾取后有安全出弹空间的种子。扩展现有 `running_server(binary, seed=SEED)` 和 `Wire.wait_for(condition, label, timeout=4.0)`，保留原用例默认值。启动真实服务，以 25 秒为上限等待第 900 步的第一批。用键盘让车身依次重叠两件道具，确认两件移除且只装填一发。检查场上方块和装填炮管像素，按空格后检查激光头、现存轨迹和原弹量。第二次空格得到普通弹。将时长改为 0.50 秒后同图重开，检查设置应用、激光状态清空且地图一致。
- [ ] 运行 `cargo test -p arena-playtest` 和 `python3 tests/playtest_browser.py`，确认新增快照和行为断言失败于接入尚缺。浏览器测试运行前用 `cargo build --locked -p arena-playtest` 编译当前版本。
- [ ] 实现快照和 UI。使用 `laser_pickup_size` 绘制轴对齐方块，载弹炮管加青蓝双槽标记，保持原有炮口位置。激光头与圆端轨迹使用同一青蓝色，轨迹线宽为弹丸直径，每帧仅绘制快照中的有效段。
- [ ] 更新现行 README 与开发指南。核对设置名和试玩说明，记录 15 秒、10 秒、5 件、4 倍弹速及 0.20 秒默认保留。
- [ ] 运行 `cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --locked -p arena-playtest` 及 `python3 tests/playtest_browser.py`，逐项确认通过。保存包含地面方块、载弹炮管或激光轨迹的浏览器截图到 `.tmp/`。
- [ ] 检查差异并提交 `feat: show laser pickups and trail settings in playtest`。整支分支审阅通过后合并到 `main`，在合并结果上复验并清理任务分支。
