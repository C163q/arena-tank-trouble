# 多局 Bot 测评

在仓库根目录构建裁判。批量脚本使用 Python 3.11 或以上，加载原生 bot 时使用 Linux 裁判和本机可加载的共享库。下例假设已生成三个 bot 动态库。

```sh
cargo build --locked -p arena-runner
python3 scripts/league.py --matches 12 --jobs 2 --players 3 --seed 407 \
  --out runs/my-league ./alpha.so ./beta.so ./gamma.so
```

可以用 `demo` 替换任一库以跑通流程。输入顺序决定总榜中稳定的 bot 编号。同名文件也按编号区分。默认每局最多 4 个席位。输入池超过单局席数时，工具优先选出场少的 bot，同等出场时优先减少重复同场。每组复用同一张地图轮换出生位。总局数不足以完整轮换时，末组在指定局数结束。固定同一种子、输入顺序、人数和局数可重建赛程。

| 参数 | 用途 |
| --- | --- |
| `--matches` | 总对局数，默认等于单局席数 |
| `--jobs` | 同时运行的对局数上限，默认 1 |
| `--players` | 单局 2、3 或 4 个席位，默认取输入数与 4 中较小者 |
| `--seed` | 第一个轮换组的地图种子，决定后续地图和赛程 |
| `--max-ticks` | 每局模拟步数上限，默认值与裁判一致 |
| `--kill-points`、`--survival-points` | 覆盖每局积分权重，默认分别为 1 和 2 |
| `--runner` | 指定裁判二进制，默认使用 `target/debug/arena-runner` |

并行会让各局共享机器资源，决策超时可能随负载变化。比较不同测评结果时，报告中的并行数也是运行条件。

## 输出与排行榜

`--out` 指向尚未存在的目录。完成后打开其中的 `summary.md` 查看排行和各局回放链接。对应的 `summary.json` 包含赛程编号、实际结果、分项积分和文件路径。`plan.json` 记录赛程与输入文件的 SHA-256。每局使用期初指定的库路径。

排名按已完成场次的场均总分，场均完全相同则并列。总分为击杀分与存活分之和。报告保留出场数、胜场及分项积分，失败或中断的场次单列，并保留对应日志。部分场次失败时脚本仍生成汇总，并返回非零状态。

每个成功结束的 `match-NNNN/` 包含独立回放、可导入的 JSON 和逐步文本。把任一局的 `replay.json` 或 `timeline.jsonl` 拖入输出目录根部的 `viewer.html` 即可播放。文本字段和 bot 接口见 [Bot 对战、接口与回放](bot-match.md)。

## 验证

在 Linux 上构建裁判后运行下列命令。脚本用本机 C 编译器生成测试 bot，覆盖同名库、换位、并行执行、失败场次和中断时关闭 bot 进程组，并写出可查看的汇总。

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tests/league_integration.py --artifacts .tmp/league-check
```
