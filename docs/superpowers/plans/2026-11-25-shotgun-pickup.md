# 散弹道具实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**进度**　共 2 项，已完成 2 项。实现已合入 `main`。40 项 Rust 测试、格式检查、Clippy 和 Chromium 两种道具试玩均通过，无实施阻塞。

**目标**　玩家能拾取散弹方块覆盖下一发，并在同一次开火中发射三枚独立偏转的普通弹丸。

**架构**　Rust 用单个道具槽保存下一发类型，激光和散弹共享地面刷新。散弹通过一次现有炮口扫掠后生成三枚带速度倍率的普通弹丸，继续复用普通弹反弹与命中判定。浏览器从 WebSocket 快照读取道具和装填类型。

**技术栈**　现有 Rust 2024、ChaCha8、Serde、Axum、Canvas 2D 和 Python Playwright。

**规格**　[散弹道具设计](../specs/2026-11-25-shotgun-pickup-design.md)

## 通用约束

- 首次刷新在开局 15 秒，之后每 10 秒尝试 1–2 件。每件独立等概率为激光或散弹，共用 5 件上限。方块边长 0.32 格，面积重叠时拾取。
- 后拾取的类型覆盖下一发。道具发射保留普通弹量及补弹目标步，支持空弹匣。散弹发射三枚弹丸，每枚独立取左右 20° 内的角度和 0.85–1.15 倍的普通弹速。
- 位置、类型及散弹随机数使用独立的确定性序列。保留普通弹和激光的现有反弹、计分、装填与轨迹规则。
- 从 `main` 创建任务工作树。通过整支分支审阅和验收后合并，复验并清理工作树。

## 优先复核

- 同一轮内拾取激光再拾取散弹，以及反向覆盖。见 `latest_pickup_replaces_the_pending_shot`。
- 散弹在空弹匣发射后，下一次开火恢复普通弹规则，补弹仍在原定步到期。见 `shotgun_fire_uses_no_ammo`。
- 贴墙反弹后散弹从同一个扫掠终点散开，扣除先前触墙次数。炮口内终止时不产生三枚飞行弹丸。见 `shotgun_shares_one_muzzle_sweep`。
- 同样的刷新条件下，多打一发散弹不改变下一批道具的位置和类型。见 `shotgun_randomness_is_replayable_and_independent`。
- 两种方块使用可辨识的图案，拾取混合道具时炮口实际切换。见 `test_shotgun_replaces_laser_in_browser`。

## 实施任务

### 任务 1　混合刷新、单槽装填和散弹推进

**文件**　修改 `crates/arena-core/src/pickups.rs`、`round.rs`、`projectile.rs` 和 `lib.rs`。模块内新增测试，更新受类型变化影响的现有核心测试。

**接口**

- `PickupKind::{Laser, Shotgun}`，可复制、可比较并序列化为 `"laser"`、`"shotgun"`。`Pickup { position: Vec2, kind: PickupKind }` 序列化为 `{x,y,kind}`，位置字段使用 Serde flatten。`Pickups::collect(&mut self, poses: [Pose; 2]) -> [Option<PickupKind>; 2]`，`Pickups::items(&self) -> &[Pickup]`。由 `arena-core` 导出 `Pickup`、`PickupKind` 和共享的 `PICKUP_SIZE`，以别名保留现有 `LASER_PICKUP_SIZE`。
- `Tank.next_shot` 与 `TankView.next_shot` 均为 `Option<PickupKind>`。现有 `TankView.laser_loaded` 从该字段计算，用于兼容现有快照消费者。`RoundView<'a>.pickups` 类型改为 `&'a [Pickup]`。
- `Tank::take_shot(&mut self, tick: u64, reload_ticks: u64) -> Option<Shot>`，私有 `Shot::{Shell, Laser, Shotgun}` 决定本次发射。 `Projectile::scattered(&self, angle_offset: f64, speed_scale: f64) -> Projectile` 复制扫掠后的弹丸并调整方向和速度，`Projectile.speed_scale: f64` 普通弹初始为 1.0。散弹弹丸的 `ProjectileKind` 为 `Shell`。

- [x] 写 `mixed_pickups_share_the_cap` 和 `latest_pickup_replaces_the_pending_shot`。固定两种道具都有刷新的种子，连续模拟 900、1500、2100、2700 步的刷新，检查批次增量至多 2、总数至多 5 和已占用位置不重复。用同一辆车分别先激光后散弹、先散弹后激光，断言 `next_shot == Some(最后的类型)` 且两块均从场上移除。保留正向和斜向贴边、重叠、等距归属用例。
- [x] 运行 `cargo test -p arena-core`，确认新增测试在类型和覆盖逻辑尚缺时失败。
- [x] 实现混合刷新和状态。`Pickups` 保留现有位置随机序列，用回合种子异或 `0xD1B5_4A32_D192_ED03` 初始化类型序列，每成功生成一件调用一次 `gen_bool(0.5)`。仅对本步拾取到 `Some(kind)` 的坦克替换装填槽，沿用已有先移动、再拾取、后开火顺序。更新激光相关核心夹具，使其拾取明确为激光的道具。
- [x] 写 `shotgun_fire_uses_no_ammo` 和 `shotgun_randomness_is_replayable_and_independent`。清空弹匣、装填散弹并设置明确的下一次补弹步，在开阔场景开火后断言三枚 `Shell`、槽为空且弹量及补弹步保持原值。再次按火时数量仍为三枚，到补弹步后下一枪只新增一枚普通弹。用连续两步位移计算每枚弹丸角度和速度，断言都落在规格区间内且在一组固定种子中出现三枚同侧、不同速度的结果。相同种子和输入下逐步快照相等。另在安全对照局消耗一次散弹随机数，在两局同一姿态和相同道具占用条件下比对下一批位置与类型。
- [x] 写 `shotgun_shares_one_muzzle_sweep`。墙位于 `x = 1.0`，玩家中心 `x = 0.62`、面向右，前进耗时设为 3 秒/格。用连续两步位置差反推本步飞行起点，检查三枚均从炮口反弹后的 `x = 0.896` 出发，扇形朝左。在 `projectile.rs` 的模块测试验证 `scattered` 继承炮口扫掠记录的一次触墙。把触墙上限设为 1 后，断言开火消耗装填及产生零枚飞行弹丸，普通弹量保持原值。再用炮口内命中目标的贴身夹具验证统一结算。
- [x] 运行 `cargo test -p arena-core`，确认散弹用例失败于发射尚未接入。
- [x] 实现发射。`Round` 用种子异或 `0x94D0_49BB_1331_11EB` 初始化散弹序列。每次散弹开火先为三枚弹丸分别抽取偏角和速度倍率，再做炮口扫掠，使每次开火消费相同数量的随机样本。普通弹模型做一次车头方向的炮口扫掠，若存活则用 `scattered` 派生三枚，每枚继承扫掠后的位置、所有者和触墙次数。以 `普通弹单步位移 × speed_scale` 推进散弹，激光仍以现有倍率推进并留下轨迹。
- [x] 运行 `cargo fmt --all -- --check`、`cargo test -p arena-core` 与 `cargo clippy -p arena-core --all-targets -- -D warnings`，确认核心用例和旧弹道回归通过。检查差异并提交 `feat: simulate mixed pickups and shotgun spread`。

### 任务 2　快照、散弹外观与完整试玩

**文件**　修改 `crates/arena-playtest/src/session.rs`、`crates/arena-playtest/web/canvas.js`、`tests/playtest_browser.py`、`README.md` 及 `docs/development/playtest.md`。

**接口**

- 快照的 `pickups` 项包含 `x`、`y` 和 `kind`，坦克包含 `next_shot`，可为 `null`、`"laser"` 或 `"shotgun"`。`geometry.pickup_size` 使用 `PICKUP_SIZE`，保留 `laser_pickup_size` 作为旧快照键。普通弹和散弹的 `projectiles[].kind` 均为 `"shell"`。
- Canvas 使用道具类型选择图案，散弹为暖色方块且有三点标记。使用 `next_shot` 决定炮口状态，散弹装填时绘制暖色宽口炮管并沿用现有炮口位置。

- [x] 更新 `restart_validates_and_resets_laser_state` 及 `test_laser_pickup_and_fire` 的固定地图夹具。挑选首批可沿出生通道拾取两件激光的种子，保留现有实况发射、轨迹像素、时长调整及同图重开断言。重开后 `next_shot == null`。
- [x] 写 `test_shotgun_replaces_laser_in_browser`。选首批两种道具都位于一条出生通道、先激光后散弹且随后有安全出弹空间的固定种子。用真浏览器等首批刷新，验证方块图案和两次装填状态切换。第一次按空格新增三枚普通外观弹丸，`ammo` 保持原值，`next_shot` 回到 `null`。第二次开火只新增一枚并消耗一发普通弹。
- [x] 运行 `cargo test -p arena-playtest`，然后编译当前服务并运行 `python3 tests/playtest_browser.py`，确认新增断言可发现快照或炮口接入尚缺。
- [x] 接通快照字段与 Canvas，更新当前 README 和开发指南的共享刷新、覆盖、角度与速度说明。保存含混合方块或暖色炮口的截图到 `.tmp/`。
- [x] 逐项运行 `cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --locked -p arena-playtest` 与 `python3 tests/playtest_browser.py`，确认现有和新增用例通过。检查差异并提交 `feat: render and verify shotgun pickups in playtest`。
- [x] 整支分支审阅通过后合并到 `main`，在合并结果上复验，更新计划完成状态并清理任务工作树和分支。
