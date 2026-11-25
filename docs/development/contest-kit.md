# 构建参赛题包

在项目根目录运行。构建机需要 Linux x86_64 GNU/glibc、支持 Rust 2024 的 Cargo 和 rustc、Python 3.11 或更新版本，以及 GNU `readelf`。

```sh
python3 scripts/package_contest.py
```

默认输出为 `dist/contest-kit/`。使用 `--out` 指定新目录，或者在 Cargo 依赖已缓存的机器上加 `--offline`。构建脚本使用独立的 `target/contest-build/` 缓存，并编译发布版裁判、基准 Bot 和计时工具。它检查构建产物的调试节、记录实际所需的 glibc 版本，再以不覆盖已有目录的方式发布。

| 输出 | 用途 |
| --- | --- |
| `participant.tar.gz` | 分发给参赛者，解压后从 `arena-contest/README.md` 开始 |
| `README.md` | 组织者的环境检查、计时、收卷和正式评测说明，含压缩包和计时器的 SHA-256 校验命令 |
| `fff-timer` | 组织者操作的现场计时器 |
| `LICENSE` | 项目的 MIT 许可 |

题包按打包脚本中的白名单收录裁判、基准动态库、参赛文档、只读查询器、头文件、C 起步文件、自测工具和离线播放器。引擎、裁判与基准算法以构建产物提供，内部文档、对局、历史工作区和计时器保留在包外。包内 `manifest.json` 记录构建时提交、工作区状态、目标架构、ABI、glibc 最低版本和各文件的 SHA-256。

比赛规则、接口和命令变化时，核对题包模板与实际执行结果，同步更新参赛者和组织者入口。维护代码见[打包脚本](../../scripts/package_contest.py)，维护模板见[参赛说明](../../packaging/contest/participant/README.md)和[场外说明](../../packaging/contest/organizer/README.md)。

## 计时程序来源

`fff-timer` 和 `fff-timer-query` 复用 Fight for Flags 提交 `7f1f9d2` 的 `fff-session-timer`，保留 Linux 上的时间基准、状态文件和查询方式。仅在包入口添加了平台判断，并使用本仓库的依赖版本。60/70 分钟的截止动作按场外说明执行。[项目许可](../../LICENSE)

## 验收题包

在具备上述运行环境和 C 编译器的 Linux 机器上，从全新目录解压题包并实测接入。下例指定未存在的验收目录。

```sh
python3 tests/contest_package.py --kit dist/contest-kit --out .tmp/contest-package-check
```

验收检查题包清单和编译命令，运行接入检查、两局同图换位自测，并确认非法动作会被接入检查检出。结果和回放保存在解压目录的 `runs/`。
