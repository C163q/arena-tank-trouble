# 自测与回放

在解压得到的 `arena-contest/` 目录运行。示例默认加载 `submission/libteam.so`，可以用 `--bot ./other.so` 测试已有的其他版本。每次使用尚未存在的输出目录。

```sh
python3 scripts/selftest.py check --out runs/check
python3 scripts/selftest.py run --out runs/quick
```

`check` 让裁判加载你的库和基准库，各调用两步。加载失败时先检查库路径、ABI 版本和两个导出函数。超时、崩溃或非法动作可以在生成的 `replay.json` 中查看，找到 `decisions` 下的 `status` 和 `diagnostic`。

`run` 用同一张迷宫打两局，第二局互换出生位。加 `--seed 408` 可以换一张图。先看 `summary.md` 的各局得分，再看回放找出坦克被卡住、错过射击或没及时躲进安全区的位置。

打赢基础档后，可以用挑战档检验策略。挑战 Bot 会争夺中心位置，也会利用墙面反弹开火。

```sh
python3 scripts/selftest.py run --opponent challenge --out runs/challenge
```

## 看一局

- 在浏览器打开 `match-0001/replay.html`，可以暂停、调倍速和拖动进度条。
- 也可以打开题包根目录的 `viewer.html`，把 `replay.json` 或 `timeline.jsonl` 拖进页面。
- `replay.json` 包含完整字段。`timeline.jsonl` 在首行说明字段与地图，之后每个模拟步一行，末行记录结果。把首行和想分析的帧一起交给 LLM。回放是裁判当步的真实状态，可以用来和有 200 ms 延迟的 Bot 观测比较。

有多份版本需要自己比较时，可以直接用 `scripts/league.py`，指定 `--runner bin/arena-runner` 和各库路径。用 `python3 scripts/league.py --help` 查场数和席位参数。先用少量场次确认结果，较大的自测会占用开发时间和磁盘。
