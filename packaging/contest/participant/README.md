# 坦克竞技场 · 参赛题目

写一个程序，在随机迷宫中控制一辆坦克，与其他参赛者的程序对战。比赛程序每步告诉你坦克的位置、墙、子弹和道具。你的程序选择前进、后退、转向或开火，由比赛程序执行。**击毁其他坦克得 1 分，成为最终存活者得 2 分。**

最终提交 `submission/libteam.so`。`.so` 是由比赛程序加载和调用的动态库。可以使用 C、C++、Rust 或其他能导出本题接口的语言。

## 第一次运行

先读[基本规则](docs/rules.md)，再看[如何接收局面和返回动作](docs/api.md)。题包在 Linux x86_64 GNU/glibc 上运行，需要系统提供 `libgcc_s.so.1`。最低 glibc 版本见包内的 `manifest.json`。自测需要 Python 3.11 或更新版本，下面的 C 示例需要 `cc`。

在解压得到的 `arena-contest/` 内运行以下命令。起步程序会原地等待，可以用它先确认编译和接入流程。

```sh
mkdir -p submission
cc -std=c11 -O2 -fPIC -shared -Iinclude starter/bot.c -o submission/libteam-next.so &&
  mv submission/libteam-next.so submission/libteam.so
python3 scripts/selftest.py check --out runs/check
python3 scripts/selftest.py run --out runs/quick
```

第一条自测检查裁判能否加载并调用你的库。第二条让你和基准 Bot 对战两局，同一张地图交换出生位。打开 `runs/quick/summary.md` 可以看到得分和回放链接。每次重测用一个新目录名。[自测和回放](docs/testing.md)

打赢基础档后，可以用挑战档检验策略。挑战 Bot 会争夺中心位置，也会利用墙面反弹开火。

```sh
python3 scripts/selftest.py run --opponent challenge --out runs/challenge
```

写策略时，可以从一个小目标开始，比如让坦克沿一条走廊移动，或朝看得见的目标开火。

**正式比赛中，其他参赛者的 Bot 可能比基准 Bot 强不少。** 即使已经打赢基准，也请在开发时限内尽力优化。可以换几个地图种子重测，从失分的回放找出原因，再比较改动前后的成绩，提交表现最好的一版。

## LLM 开发时限

**60 分钟内**完成设计、编码、编译和自测，留下可用的 `submission/libteam.so`。开发过程中用下面的只读工具查看已经过去的时间。它输出一行 `HH:MM:SS`，查询失败时请报告组织者。

```sh
bin/fff-timer-query
```

**70 分钟强制收卷**。组织者终止会话和仍在执行的开发命令，收取当时已经编译完成的提交。没有编译结果视为弃权。计时器由组织者操作。纯人类参赛者的时间安排由组织者另行公布。

提交以 `submission/libteam.so` 为准。如需其他动态库，提交时说明依赖及在评测机上的加载方式。
