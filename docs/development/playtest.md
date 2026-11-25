# 静止靶试玩版开发与验证

从仓库根目录按[试玩入口](../../README.md#本地试玩)启动。程序把页面和 WebSocket 都绑定到 `127.0.0.1`，每个连接有独立练习状态。启动参数放在 Cargo 的 `--` 后。用 `--port 0` 让系统分配端口，程序 stdout 的首行会给出实际 URL。用 `--seed 407` 固定第一张地图，便于比较同一初始条件。

## 修改入口

| 职责 | 位置 |
| --- | --- |
| 迷宫、出生、坦克移动和几何碰撞 | [arena-core](../../crates/arena-core/src/lib.rs) |
| 补弹、开火和单步结算 | [Round](../../crates/arena-core/src/round.rs) |
| 本地 HTTP 入口、WebSocket 与定时器 | [arena-playtest 主程序](../../crates/arena-playtest/src/main.rs) |
| 消息格式、同图与换图重开、计分 | [Session](../../crates/arena-playtest/src/session.rs) |
| 按键、连接与设置，Canvas 绘制 | [app.js](../../crates/arena-playtest/web/app.js) · [canvas.js](../../crates/arena-playtest/web/canvas.js) |

Rust 核心每次 `Round::step` 推进 1/60 秒。当前服务将键盘操作传给玩家坦克，将空操作传给静止靶。同一种子、同一构建和同一串逐步输入得到相同局面。浏览器通过 WebSocket 获取快照，按刷新频率绘制。参数校验由 Rust 执行。

## 验证

在仓库根目录运行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --locked -p arena-playtest
python3 tests/playtest_browser.py
```

浏览器测试需要 Python Playwright 和 Chromium。安装 Playwright 自带的 Chromium，或使用系统 Chromium。系统浏览器的路径可通过 `ARENA_CHROMIUM` 指定。脚本默认验证 `target/debug/arena-playtest`，可用 `--binary` 指定其他构建产物。脚本临时启动本地服务，验证按键、计分、补弹、重开、失焦和断线，保存宽屏和窄屏截图到 `.tmp/`。修改规则时先跑 Rust 测试，修改连接或控件时再跑浏览器测试。
