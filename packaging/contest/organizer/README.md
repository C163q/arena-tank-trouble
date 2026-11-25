# 坦克竞技场 · 场外操作

向参赛者分发 `participant.tar.gz`。外层的 `fff-timer` 由组织者操作。

## 准备环境

在本目录核对文件，再解压到新的参赛目录。

```sh
printf '%s  %s\n' '@ARCHIVE_SHA256@' participant.tar.gz | sha256sum -c -
printf '%s  %s\n' '@TIMER_SHA256@' fff-timer | sha256sum -c -
mkdir participant-work
tar -xzf participant.tar.gz -C participant-work
```

将 `participant-work/arena-contest/` 设为参赛者工作目录。为每位参赛者准备独立的文件访问范围，参赛者应能读取自己的题包和工具，组织者单独保管计时器、其他提交及正式赛程。

在 Linux x86_64 GNU/glibc 上运行，需要系统提供 `libgcc_s.so.1`。最低 glibc 版本见解压目录内的 `manifest.json`。自测需要 Python 3.11 或以上。计时前准备好参赛者所需的编译器，并进入解压目录检查裁判能否加载两档基准。

```sh
cd participant-work/arena-contest
python3 scripts/selftest.py check --opponent challenge --bot lib/libarena_baseline_bot.so --out runs/environment
```

## LLM 开发计时与收卷

在 LLM 开始任务时，在外层目录的独立终端运行 `./fff-timer`。计时器从零开始，按 Ctrl+C 退出。共享同一个 `/tmp` 的环境只可运行一个实例，与 Fight for Flags 的计时器共用状态位置。

LLM 从参赛目录执行 `bin/fff-timer-query` 查看总用时。让参赛环境只读访问同一个 `/tmp/fff-session-timer-v1`，并使用相同的 Linux `CLOCK_BOOTTIME` 时间基准。在正式开始前预检一次查询，再由组织者重新开启正式计时。计时异常由组织者处理。

- **60 分钟**内完成策略设计、编码、编译和自测，交付 `submission/libteam.so`。
- **70 分钟**由组织者强制结束 LLM 会话及仍在运行的开发命令，立即收取已编译的 `submission/libteam.so`，没有编译结果记为弃权。组织者通过运行平台执行收卷，计时器只显示已经过去的时间。

记录库的 SHA-256 和额外依赖，再用原题包裁判验收接口。纯人类参赛者的时间安排由组织者另行公布。

## 正式评测

收齐提交后，使用同一题包的裁判、统一的地图种子和计分规则。让每一组同图比赛完整轮换出生位。正式评测串行运行，以减少并行争抢对决策时限的影响。

在专用的隔离环境中验收并评测外部动态库。Bot 的独立进程用于处理超时和崩溃，它拥有评测用户的权限。评测环境只需开放题包、本次使用的动态库和输出目录，将项目源码、计时状态和其他文件留在场外。

在评测环境解压同一份题包，进入 `arena-contest/` 运行下面的命令。示例适用于两个提交，将库路径换成实际路径。

```sh
python3 scripts/league.py --runner bin/arena-runner \
  --players 2 --matches 12 --jobs 1 --seed 407 --out runs/formal \
  /path/to/team-a/libteam.so /path/to/team-b/libteam.so
```

按本届参赛人数确定出场名单、席位数和场数，并记录每个库路径对应的参赛者。结果按输入顺序给 Bot 编号，同名文件也能区分。用输出目录的 `plan.json` 核对换位，查看 `summary.md` 的场均积分、排名和回放。若场次失败或中断，先查对应日志并处理，再发布结果。
