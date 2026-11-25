# 静止靶试玩版实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**进度**：共 4 项，已完成 0 项。设计已确认，当前待审阅实施计划，余下工作从迷宫生成与驾驶开始。实施中同步更新此处和任务勾选。

**目标**　在本地浏览器完成一局静止靶练习，调整参数后复用地图和出生状态对比手感。

**架构**　Cargo 工作区包含独立 Rust 模拟库 `arena-core` 和本地程序 `arena-playtest`。后者从同一地址提供静态页面与 WebSocket，Canvas 绘制模拟快照。

**技术栈**　Rust 2024、Axum 0.8（`ws`）、Tokio 1、Serde、原生 JavaScript、Canvas 2D。测试使用 Cargo 和 Python Playwright。

**规格**　[静止靶试玩版设计](../specs/2026-11-25-static-target-prototype-design.md)

## 通用约束

- 每秒推进 60 个等长模拟步。玩法参数默认是前进耗时 0.45 秒/格、后退 0.65 秒/格、转一圈 1.5 秒、每 1.0 秒补 1 发、触墙上限 5 次。弹匣容量固定为 5 发。
- 浏览器连接各自持有练习状态。Rust 程序监听回环地址，模拟库独立于网络和浏览器。
- 实施从 `main` 创建任务分支。建立工作树前检查仍未跟踪的基础文档。基础文档与试玩改动分开提交，保证任务工作树有可用的项目约定和文档入口。
- 本机使用已安装的 stable Rust 工具链。Chrome 或 Chromium 用于浏览器验证。运行时由 Rust 程序提供全部页面资源。
- 在受限沙箱使用本机 Rust 时，若 rustup 代理因只读目录无法启动，将已安装工具链的 bin 目录放在 PATH 开头。具体路径记在本机的 `docs/local.md`。

## 优先复核

- 开火时炮口碰到墙，必须扣弹并当场触墙。见任务 2 的 `wall_at_muzzle_counts_as_first_contact`。
- 快速弹丸经过拐角或靶，必须按路径检测最先接触的物体。见任务 2 的 `corner_and_swept_hit`。
- 不合法的调参请求应给出错误并继续当前局。见任务 3 的 `invalid_restart_keeps_current_round`。
- 重开确认前按住的方向键以及旧局的开火请求不能影响新局。见任务 3 的 `stale_round_input_is_ignored` 和任务 4 的重开验收。
- 按住方向键时窗口失焦，重新回到游戏时应处于空操作。见任务 4 的 `test_blur_releases_input`。

## 文件与实现任务

### 任务 1　迷宫生成、出生和驾驶

**文件**　创建 `Cargo.toml`、`Cargo.lock`、`crates/arena-core/Cargo.toml`、`crates/arena-core/src/lib.rs`、`maze.rs`、`motion.rs`。测试与这两个模块同文件。根 `.gitignore` 加入 `/target/`。

**接口**

- `Vec2 { x: f64, y: f64 }`，坐标使用格宽为单位，右和下为正。朝向以弧度表示，0 朝右，正角为顺时针。
- `Maze::generate(seed: u64) -> (Maze, [Pose; 2])`。公开 `Maze { width: u8, height: u8, walls: Vec<Wall> }`、`Wall { a: Vec2, b: Vec2 }` 和 `Pose { position: Vec2, angle: f64 }`。出生状态索引 0 为玩家，1 为靶。
- `Params { forward_secs: f64, reverse_secs: f64, turn_secs: f64, reload_secs: f64, wall_contacts: u8 }` 实现 `Default` 与 `validate(&self) -> Result<(), &'static str>`。
- `Action { forward: bool, reverse: bool, turn_left: bool, turn_right: bool, fire: bool }`，默认全为 false。`motion::drive(maze: &Maze, pose: Pose, other: Pose, action: Action, params: &Params) -> Pose` 每次推进一步。

- [ ] 写 `seed_reproduces_connected_maze`、`drive_turn_and_block` 和 `parameter_ranges`。对 0、1、42 检查 `assert_eq!(Maze::generate(seed), Maze::generate(seed))`，从玩家格心沿开口遍历后检查 `assert_eq!(visited.len(), 96)`，两个出生格的通路距离至少 4。检查从 0 初始角右转一步得到 `TAU / 90.0`，同时前进的位移长度为 `1.0 / 27.0`。检查左转为负方向、后退一步长度为 `1.0 / 39.0`，两轴相反键各自抵消。墙或靶挡住位移时角度仍更新。对参数范围两端检查 `validate().is_ok()`，对零、非有限值和范围外值检查 `validate().is_err()`。
- [ ] 运行 `cargo test -p arena-core`，确认测试因对应行为缺失而失败。
- [ ] 用 ChaCha8 和随机深度优先挖出 12×8 连通迷宫，再打开 10 条随机内墙形成回路。以固定顺序枚举候选后随机抽选，确保种子可复现。使用整数网格墙段，外墙始终封闭。选两个相隔至少 4 步的格心，朝向从四个基本方向抽选。Rust 和 Canvas 共用 0.08 格的墙厚以及 0.23 格的坦克碰撞半径。
- [ ] 实现 `drive`，先应用转向，再按新朝向尝试位移。移动遇到碰撞停在首次接触处，沿墙远离接触的一步可以继续移动。`Params::validate` 的四个秒数范围依次为闭区间 0.15–3、0.15–3、0.25–6、0.2–10，触墙次数为 1–20。
- [ ] 运行 `cargo test -p arena-core` 与 `cargo fmt --all -- --check`，确认通过。
- [ ] 检查差异并提交 `feat: add seeded arena and tank movement`。

### 任务 2　普通弹、弹匣和回合结果

**文件**　创建 `crates/arena-core/src/round.rs`、`projectile.rs`，在 `lib.rs` 导出类型并复用任务 1 的几何数据。回合测试与 `round.rs` 同文件。

**接口**

- `Round::new(seed: u64, params: Params) -> Result<Round, &'static str>` 校验参数并创建回合。`Round::step(&mut self, actions: [Action; 2])` 推进一个模拟步。`Round::view(&self) -> RoundView<'_>` 提供只读快照，字段为 `seed: u64`、`tick: u64`、`maze: &Maze`、`params: &Params`、`tanks: [TankView; 2]`、`projectiles: Vec<ProjectileView>`、`outcome: RoundOutcome`。
- `TankView { pose: Pose, ammo: u8 }`，`ProjectileView { position: Vec2 }`。结果为 `RoundOutcome::{Running, TargetHit, SelfHit, Draw}`，两个坦克都能使用同一动作结构，试玩由外层给靶传空操作。

- [ ] 写 `magazine_refills_on_due_step`。在无命中的固定几何中连续打 5 发，检查弹量依次为 `[4, 3, 2, 1, 0]`。第 6 个开火请求不生成弹丸。首次开火后第 59 个后续模拟步仍为 0，第 60 个为 1，再过 60 步为 2。补到 5 后保持满额，下一次开火重新开始计时。
- [ ] 写 `wall_at_muzzle_counts_as_first_contact`、`corner_and_swept_hit` 和 `round_freezes_and_tracks_result`。炮口碰墙时检查 `ammo == 4` 且首触墙可让弹丸回击玩家，上限 1 时检查当场消失。斜向打中内角时两轴各反向一次，记一次触墙。用路径始末都在靶碰撞体外的测试射线检查连续命中，首次反弹前穿过发射者不造成命中。两辆同一步被命中为 `Draw`，结束后额外输入不改变步号与局面。相同种子、参数和一串动作产生相同快照。
- [ ] 运行 `cargo test -p arena-core round::tests`，确认对应规则尚未实现。
- [ ] 实现回合与弹丸。弹速取 9 格/秒，弹丸半径 0.035 格，炮口离中心 0.28 格。每步结算到期补弹、移动、开火、弹道，再写回结果。补弹间隔向上取整为模拟步，开火用掉本步可用的一发。新弹沿中心到炮口的路径先检查碰撞。忽略尚未反弹的本车碰撞，比较墙和坦克的最早接触时间，同一时刻先处理墙。反弹后推进余下的步内位移。记录每辆坦克是否被击中，完成整步后定胜负。
- [ ] 运行 `cargo test -p arena-core`、`cargo fmt --all -- --check` 和 `cargo clippy -p arena-core --all-targets -- -D warnings`，确认通过。
- [ ] 检查差异并提交 `feat: simulate tank rounds and ricochets`。

### 任务 3　本地服务器和 WebSocket

**文件**　将 `crates/arena-playtest` 加入工作区，创建其 `Cargo.toml`、`src/main.rs`、`src/session.rs`。协议测试与 `session.rs` 同文件。更新锁文件。Axum 提供 WebSocket 升级，Tokio 为每条连接安排模拟时钟。[Axum WebSocket 文档](https://docs.rs/axum/latest/axum/extract/ws/index.html) · [Tokio 定时器文档](https://docs.rs/tokio/latest/tokio/time/enum.MissedTickBehavior.html)

**接口**

- `Session::new(initial_seed: u64) -> Session`，`Session::receive(&mut self, text: &str) -> Option<ServerMessage>`，`Session::tick(&mut self) -> ServerMessage`，`Session::state(&self) -> ServerMessage`。连接建立后立即发送初始状态。
- 客户端 JSON 以 `type` 区分消息。`input` 含 `round` 和四个布尔字段 `forward`、`reverse`、`turn_left`、`turn_right`。`fire` 含 `round`。`restart` 含 `round`、`mode`（`same` 或 `new`）、`params`（任务 1 的字段）。每一步最多消费一次开火请求。
- 状态消息为 `type: "state"`，字段为 `round`、`tick`、`seed`（十进制字符串）、`width`、`height`、`walls`（`a`、`b`）、`tanks`（`pose.position`、`pose.angle`、`ammo`）、`projectiles`（`position`）、`params`、`outcome`（`running`、`target_hit`、`self_hit`、`draw`）、`target_hits` 和 `self_hits`。错误消息为 `type: "error"`，含 `message`。
- `cargo run -p arena-playtest` 默认监听 `127.0.0.1:8080`。`--port 0` 可用于测试，启动时打印实际 URL。`--seed <u64>` 可复现实验。GET `/ws` 升级连接。

- [ ] 写 `same_and_new_restart`、`invalid_restart_keeps_current_round` 和 `stale_round_input_is_ignored`。同图检查 `seed`、墙和出生状态相同，弹量为 5、步号归零、计数保留且新参数应用。换图检查种子改变且计数为 0。非法参数检查收到错误且处理前后的状态相等。旧 `round` 的按键、开火和重开消息发来后，当前局仍保持无输入。还要验证两条连接各自的计数和输入独立。
- [ ] 运行 `cargo test -p arena-playtest`，确认协议测试失败。
- [ ] 实现协议与服务器。通过一次有效重开原子地替换整局并增加 `round`，同图与换图分别处理计数。换图时从初始种子派生下一局种子，每个连接单独持有序列。只在结果首次进入结束状态时增加相应计数。定时器若延迟则跳过过期的墙钟唤醒，模拟步长仍为 1/60 秒。写入失败或输入连接关闭后结束连接任务。
- [ ] 运行 `cargo test --workspace` 和 `cargo clippy --workspace --all-targets -- -D warnings`，确认通过。用浏览器或测试脚本打开真实 `/ws`，检查首帧和后续步号都能收到。
- [ ] 检查差异并提交 `feat: serve playtest rounds over WebSocket`。

### 任务 4　Canvas 试玩页与端到端验收

**文件**　创建 `crates/arena-playtest/web/index.html`、`style.css`、`app.js`、`canvas.js`、`tests/playtest_browser.py`、根 `README.md`、`docs/development/playtest.md`。通过 `src/main.rs` 内嵌并提供页面资源，从 `docs/README.md` 链接持续维护的开发指南。

**接口**　复用任务 3 的消息格式。页面提供「同图重开」「换图重开」、五项参数、五格弹量、两项计数、种子和连接状态。`canvas.js` 导出 `render(canvas, state)`，比例适配视口和设备像素密度。测试启动真实程序并解析其打印的 URL。

- [ ] 写 `test_keyboard_hit_and_restart` 和 `test_blur_releases_input`，用 Playwright 键盘和 WebSocket 帧观察真实服务端状态。固定一颗开局可直接打靶的种子，通过 WASD 验证位置和朝向改变，同图复位后按空格命中并检查结束时计数增加一次。长按空格检查只有一次开火请求。用 W 触发移动，再派发浏览器失焦，检查后续快照位置停止变化。同图检查出生状态一致和参数生效，换图检查种子变化、计数清零。重开前按住的键在松开并重新按下之前不产生新局操作。关闭服务器后检查看得见的断线状态。
- [ ] 运行 `cargo build -p arena-playtest` 后执行 `python3 tests/playtest_browser.py`，确认在页面或交互尚缺时失败。
- [ ] 实现画布和控件。浅底迷宫使用深色线条，原创俯视坦克画出车头。底部用色块、计数和五格弹量表示状态。用键盘的物理按键代码处理 WASD 和空格，忽略重复开火。离开窗口、页面隐藏或进入参数区时发送清空操作。锁定旧局键直到释放，收到重开快照再允许新的操作。连接关闭后显示「重新连接」按钮，重新连接开始新练习。
- [ ] 编写启动、控件和验证文档。用 Playwright 检查实际绘制与键盘流程，保存桌面和窄窗口截图到 `.tmp/` 查看 HUD、参数区和迷宫是否易读。
- [ ] 运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo build -p arena-playtest`、`python3 tests/playtest_browser.py`，逐项确认通过。检查文档路径和实际启动命令。将玩家对参数手感的判断留给真人试玩。
- [ ] 检查差异并提交 `feat: add browser arena playtest`。完成整体验证后合并到 `main`，确认提交可达再清理任务分支。
