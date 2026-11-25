# 静止靶试玩版开发与验证

从仓库根目录按[试玩入口](../../README.md#本地试玩)启动。程序把页面和 WebSocket 都绑定到 `127.0.0.1`，每个连接有独立练习状态。启动参数放在 Cargo 的 `--` 后。用 `--port 0` 让系统分配端口，程序 stdout 的首行会给出实际 URL。用 `--seed 407` 固定第一张地图，便于比较同一初始条件。

## 修改入口

| 职责 | 位置 |
| --- | --- |
| 迷宫、出生、坦克移动与车身碰撞 | [arena-core](../../crates/arena-core/src/lib.rs) · [矩形车身](../../crates/arena-core/src/hull.rs) |
| 补弹、开火和单步结算 | [Round](../../crates/arena-core/src/round.rs) |
| 道具刷新与拾取 | [Pickups](../../crates/arena-core/src/pickups.rs) |
| 本地 HTTP 入口、WebSocket 与定时器 | [arena-playtest 主程序](../../crates/arena-playtest/src/main.rs) |
| 消息格式、同图与换图重开、计分 | [Session](../../crates/arena-playtest/src/session.rs) |
| 按键、连接与设置，Canvas 绘制 | [app.js](../../crates/arena-playtest/web/app.js) · [canvas.js](../../crates/arena-playtest/web/canvas.js) |

Rust 核心每次 `Round::step` 推进 1/60 秒。当前服务将键盘操作传给玩家坦克，将空操作传给静止靶。同一种子、同一构建和同一串逐步输入得到相同局面。浏览器通过 WebSocket 获取快照，按刷新频率绘制。参数校验由 Rust 执行。

## 试玩规则默认值

| 项目 | 默认值 |
| --- | --- |
| 前进或后退一格 | 0.45 秒 |
| 原地转一圈 | 1.5 秒 |
| 补回 1 发 | 3 秒 |
| 普通弹速度 | 坦克前进速度的 1.125 倍，默认每秒 2.5 格 |
| 弹丸消失前的累计触墙次数 | 10 次 |

车身碰撞使用随朝向旋转的 0.42 × 0.34 格矩形，长度沿车头方向测量。移动、原地转向和子弹命中都使用同一矩形。车身转到贴墙角度时停止旋转，弹丸沿单步路径判断是否命中车身。普通弹的显示与碰撞使用同一个半径 0.042 格。墙段是厚度 0.08 格的直角矩形，端头超出网格线端点半个墙厚。浏览器从快照读取墙厚、弹丸半径、车身尺寸和炮口距离，炮管用于显示出弹位置。

## 激光道具

开局 15 秒后刷新第一批方块，之后每 10 秒尝试刷新 1–2 件，场上最多 5 件。车身与边长 0.32 格的方块有面积重叠即可拾取。连续拾取会移除每个方块，装填状态始终只有下一发。

激光以普通弹的 6 倍速度飞行，默认每秒 15 格，第 5 次触墙或首次命中后终止。激光头和轨迹的显示与碰撞半径均为 0.030 格。每段轨迹默认保留 0.20 秒，车身碰到有效轨迹即被击毁。试玩页的「激光轨迹保留」设置可调到 0.05–2.00 秒，重开时应用。同图重开复用随机种子并重新开始计时。

## 验证

在仓库根目录运行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --locked -p arena-playtest
python3 tests/playtest_browser.py
```

浏览器测试需要 Python Playwright 和 Chromium。安装 Playwright 自带的 Chromium，或使用系统 Chromium。系统浏览器的路径可通过 `ARENA_CHROMIUM` 指定。脚本默认验证 `target/debug/arena-playtest`，可用 `--binary` 指定其他构建产物。脚本临时启动本地服务，验证按键、计分、补弹、重开、失焦、断线和激光拾取发射，保存宽屏、窄屏和激光截图到 `.tmp/`。修改规则时先跑 Rust 测试，修改连接或控件时再跑浏览器测试。
