# 坦克竞技场

坦克竞技场是一款原创的迷宫坦克对战游戏，目标是让不同大语言模型编写的坦克控制算法在同一套规则下对战。当前版本提供 2–4 席 bot 对战、批量测评和单人静止靶练习。

## 参赛题包

参赛者编写控制一辆坦克的 Bot，最终提交 `submission/libteam.so`。使用 LLM 时有 **60 分钟开发时限，70 分钟强制收卷**。从题包内的 `README.md` 开始，题包提供基本规则、接口、C 起步文件、自测和回放。

组织者可在 Linux x86_64 GNU/glibc 上生成题包。

```sh
python3 scripts/package_contest.py
```

将 `dist/contest-kit/participant.tar.gz` 分发给参赛者，按同目录的 `README.md` 操作计时器和收卷。[参赛题目模板](packaging/contest/participant/README.md) · [构建参赛题包](docs/development/contest-kit.md)

## Bot 对战

```sh
cargo build --locked --release -p arena-runner -p arena-baseline-bot
target/release/arena-runner --seed 407 --out runs/my-match \
  ./target/release/libarena_baseline_bot.so demo
```

上例在 Linux 上运行基准 Bot 与内置示例的对局。可换入自己的 Bot 动态库。比赛结束后直接打开对局目录中的 `replay.html`，也可将 `replay.json` 或 `timeline.jsonl` 拖进通用播放器。[打开通用播放器](viewer.html) · [Bot 对战、接口与回放](docs/development/bot-match.md)

批量对比可配置局数与并行数，输出排行榜和各局回放。用法见 [多局 Bot 测评](docs/development/league.md)。

## 本地试玩

安装支持 Rust 2024 的 stable 工具链和现代浏览器。在仓库根目录运行：

```sh
cargo run --locked -p arena-playtest
```

打开 [http://127.0.0.1:8080](http://127.0.0.1:8080)。W / S 前进和后退，A / D 左转和右转，空格开火。可以同时移动和转向。矩形车身的命中范围随朝向改变。普通弹碰墙会反弹，也可能打中自己。五格弹量显示可用的发数，未满时会逐发恢复。

开局 15 秒后会出现道具方块。青蓝色是激光，暖色三点标记是散弹。车身重叠方块就会装填下一发，后拾取的道具覆盖此前的装填。道具发射保留普通弹量。激光留下短暂的致命轨迹，散弹同时发出三枚随机偏转的普通弹丸。

开局 30 秒后，红色危险区从四边向中心扩张，车身越出安全区即淘汰。只剩一辆坦克时继续对局 2 秒，坚持到期才获胜，期间全灭则为平局。[收缩规则](docs/development/bot-match.md#矩形安全区)

在页面右侧填写参数。点「同图重开」应用参数并保留地图、出生位置、朝向和计数。点「换图重开」应用参数、生成另一张图并清空计数。

[开发与验证指南](docs/development/playtest.md) · [项目文档](docs/README.md)
