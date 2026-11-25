# Bot 对战、接口与回放

## 运行一场比赛

在仓库根目录用支持 Rust 2024 的 stable 工具链运行。命令行按输入顺序分配 2–4 个固定席位，可以混用 Linux 共享库和内置示例控制器。

```sh
cargo run --locked -p arena-runner -- --seed 407 --out .tmp/my-match demo demo demo demo
```

输出目录必须尚未存在。比赛结束后会一同生成 `replay.html`、`replay.json` 和 `timeline.jsonl`。直接在浏览器打开 HTML 即可离线播放，支持暂停、倍速和进度跳转。1 倍速每秒推进 60 个真实游戏帧。JSON 保留规则、地图、每步局面及实际采用的动作、出局事件、积分和最终结果。示例控制器用于跑通流程。

只剩一辆坦克时继续模拟 120 步，撑到结算的坦克获胜。全灭则平局。裁判默认步数上限为 5400 步（90 秒）。可以用 `--max-ticks` 调整上限，到达上限时若有多辆坦克仍存活，计为时限平局。已进入胜负等待的对局会继续模拟到结算。击杀分给原发射席位，自杀和收缩区淘汰均为零击杀分。存活分只给最终胜者。击杀默认 1 分，最终存活默认 2 分，分别用 `--kill-points` 和 `--survival-points` 调整。

## 矩形安全区

开局 30 秒时安全区覆盖整张地图，之后以地图中心为中心等比线性收缩。经过 `x` 秒时，安全区的长和宽均为地图对应尺寸乘以 `1 − (x − 30) / 60`，第 90 秒缩为一个点。每步按移动、转向后的矩形车身判定，车身贴边仍存活，任何部分越界即淘汰。同一步既越界又被弹丸或激光击中时，按命中结算。2 秒胜负等待期间照常收缩。

试玩和回放以红色显示安全区外的区域。

## 基准 Bot

基准 Bot 是通过公开 C ABI 加载的 Rust 动态库，各席位执行相同的确定性策略。可见敌人进入射程时瞄准并间隔开火，平时沿迷宫格子的最短路径追击，未装填道具时会绕去拾取较近的道具。它用延迟观测中的弹丸位置和速度预估短时间内的直线路径，比较前进、后退和停下的避弹空间。预测到首面墙为止。

在 Linux 上构建后，把 `./target/release/libarena_baseline_bot.so` 当作普通 Bot 路径传给裁判或测评脚本。[基准 Bot 源码](../../crates/arena-baseline-bot/src/lib.rs) · [批量测评示例](league.md#与基准-bot-对比)

```sh
cargo build --locked --release -p arena-runner -p arena-baseline-bot -p arena-challenge-bot
```

挑战档位于 `target/release/libarena_challenge_bot.so`，可以传给同一裁判或测评脚本。它沿最短路径向中心靠拢，搜索可达朝向上的直射与反弹射线，使用连续观测估计敌方位置，并根据弹丸反弹后的路径选择避让动作。[挑战 Bot 源码](../../crates/arena-challenge-bot/src/lib.rs)

## 逐步文本与通用播放器

`replay.json` 将全局和每帧数据放在一个 JSON 对象中，各字段使用完整名称，适合程序直接读取。`timeline.jsonl` 从同一份裁判帧导出，为逐行查看和 AI 分析压缩了重复字段。两种文件使用相同的裁判帧和结算结果。

`timeline.jsonl` 是供分析使用的 UTF-8 文本。首行包含地图、种子、规则和 `columns` 列定义。从第二行开始，每个模拟步一条 JSON 记录，最后一行是本场结果和分项积分。坦克与动作按席位顺序排列。第 0 步是初始局面，之后的第 `t` 行描述推进到第 `t` 步后的真实状态，`decisions` 记录推进到这一帧采用的操作。动作用 W/S/A/D/F 表示，F 表示开火，短横线表示空操作。

坐标以格为单位，角度用弧度，速度为格每秒。`zone_schedule` 记录起止步，`safe_zone` 记录当前帧的安全区边界，开始前为 `null`。在 `replay.json` 中边界为 `{min, max}` 两个坐标，在 `timeline.jsonl` 中为 `[min_x, min_y, max_x, max_y]`。`eliminations` 记录淘汰坦克和发射者的席位号，收缩区淘汰的 `owner` 为 `null`。动作状态还能辨认超时和其他错误。可按行截取所需模拟步，附上首行的列定义交给 AI 分析。

用浏览器打开项目根目录的通用播放器。把 `replay.json` 或 `timeline.jsonl` 从文件管理器拖到页面任意位置，也可点击「选择文件」。播放器从第 0 步开始显示所选对局，并保留原先打开的回放直到新文件通过校验。[通用播放器](../../viewer.html)

对局目录中的 `replay.html` 自带该局数据，打开后即可播放，也接受文件导入。裁判用同一套绘图和播放脚本生成这两种 HTML。修改模板或脚本后，可用以下命令更新项目根目录的播放器。[回放源码模板](../../crates/arena-runner/web/replay.html)

```sh
cargo run --locked -p arena-runner -- viewer --out .tmp/viewer-update.html
mv .tmp/viewer-update.html viewer.html
```

## 编写 C ABI bot

实现下列头文件声明的两个导出函数，使用位置参数把生成的共享库交给裁判。例子假设当前目录中已有 `my_bot.c`，编译机器上有 C 编译器。ABI 支持 Linux 动态库。

```sh
cc -std=c11 -O2 -shared -fPIC -I include my_bot.c -o my_bot.so
cargo run --locked -p arena-runner -- --out .tmp/native-match ./my_bot.so demo
```

参阅 [C ABI 头文件](../../include/arena_bot.h)。

`arena_bot_decide` 每次接收一份 UTF-8 JSON 观测并返回动作位。字段如下。

| 字段 | 含义 |
| --- | --- |
| `seat` | 自身席位，从 0 编号 |
| `tick`、`world_tick` | 当前模拟步和所见世界步，分别从 0 编号 |
| `ticks_per_second`、`decision_budget_ms` | 步频 60，本次墙钟预算 |
| `max_ticks`、`kill_points`、`survival_points` | 模拟步上限和本场两项计分权重 |
| `map`、`geometry`、`params` | 固定地图及墙体、物体尺寸和规则参数，位置单位为格 |
| `zone_schedule` | 固定的收缩起止步 `start_tick` 和 `end_tick` |
| `own_tank` | 当前自身坦克的朝向、位置、弹量、下次补弹步及装填道具 |
| `other_tanks` | 延迟世界中的其他坦克，附席位号和存活状态 |
| `safe_zone` | `world_tick` 时的安全区边界 `{min, max}`，开始收缩前为 `null` |
| `projectiles`、`laser_trails`、`pickups` | 延迟世界中的所有弹丸、激光轨迹和道具。速度为格每秒，激光到期步与 `world_tick` 同一时间轴 |

`world_tick = max(0, tick − 12)`。即使是自己刚发射的弹丸，观测也要在 12 步后才反映。车头角度以弧度表示。坐标向右为正 x、向下为正 y，角度零指向右，正角度顺时针。

每场首步的计算预算为 100 ms，之后为 30 ms。超时、崩溃或非法动作当步采用空操作。裁判结束故障 worker，下步可重启该席位，库内状态随进程重置，比赛步号和预算继续。独立进程用于限制决策时长和隔离崩溃，运行的库仍拥有当前用户的系统权限。

## 验证入口

头文件的真实编译和动态加载测试覆盖了首步与后续时限、崩溃、非法返回、错误版本和裁判退出时终止活跃 worker。验收脚本需要带 pidfd 支持的 Linux、Python 3.10 以上和 C 编译器。离线查看器测试还需要 Python Playwright 和 Chromium，可通过 `ARENA_CHROMIUM` 指定浏览器路径。

```sh
cargo test --workspace --locked
cargo build --locked -p arena-runner
python3 tests/bot_match.py --artifacts .tmp/bot-match-check --browser
```

对局和观测入口见 [裁判](../../crates/arena-runner/src/matchplay.rs)，动态库调度见 [Linux worker](../../crates/arena-runner/src/native.rs)，回放实现见 [导出](../../crates/arena-runner/src/replay.rs)和 [Canvas 绘制](../../crates/arena-playtest/web/canvas.js)。
