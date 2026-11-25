# 接口说明

裁判加载你的动态库，每步给仍存活的坦克调用一次 `arena_bot_decide`。你的函数读取一段 JSON，再返回一个整数表示所选动作。C ABI 指不同语言用同一种方式调用这些函数。声明见题包的 `include/arena_bot.h`。[C 起步文件](../starter/bot.c)

```c
uint32_t arena_bot_abi_version(void);
uint32_t arena_bot_decide(const uint8_t *observation, size_t len);
```

- `arena_bot_abi_version()` 返回 `ARENA_BOT_ABI_VERSION`（1）。
- `observation` 有且仅有 `len` 个 UTF-8 字节，末尾没有 C 字符串结束符。用长度读取，只在本次调用期间使用这块内存。
- 一个席位使用一个进程。库可以在调用之间保存自己的状态。超时、崩溃或非法返回时，裁判当步改用等待，并可在下步重启该席位的进程。重启会清空库内状态。
- 每局第 0 步最多 **100 ms**，之后每步最多 **30 ms**，按墙钟时间计。重启后使用原本步号的时限。

C++ 包含所附头文件即可沿用 C ABI。Rust 用 `cdylib` 构建，两个函数用 `extern "C"` 导出，长度对应 `usize`，指针对应 `*const u8`，返回值对应 `u32`。Rust 2024 的导出函数使用 `#[unsafe(no_mangle)]`。

## 返回动作

用按位或 `|` 组合需要的动作。例如 `ARENA_FORWARD | ARENA_TURN_LEFT` 会尝试边前进边向左转。返回 `0` 表示等待。

| 常量 | 值 | 动作 |
| --- | ---: | --- |
| `ARENA_FORWARD` | 1 | 前进 |
| `ARENA_REVERSE` | 2 | 后退 |
| `ARENA_TURN_LEFT` | 4 | 逆时针转动 |
| `ARENA_TURN_RIGHT` | 8 | 顺时针转动 |
| `ARENA_FIRE` | 16 | 开火 |

## 如何读局面

坐标以地图格为单位，向右是正 x，向下是正 y。角度用弧度，0 指向右，正角度顺时针。席位编号从 0 开始。

**自己的坦克是当步状态，其余动态对象来自 200 ms 前。** 例如当前步 `tick = 60` 时，其他坦克、道具、弹丸（包括你刚发射的）和安全区边界来自 `world_tick = 48`。开局最初 12 步都使用第 0 步的世界状态。地图和收缩时间表固定，可以提前用它们规划。

| 字段 | 内容 |
| --- | --- |
| `tick` / `world_tick` | 当前步与所见世界步，从 0 开始，后者为 `max(0, tick − 12)` |
| `seat` | 自己的席位号 |
| `ticks_per_second` / `decision_budget_ms` | 步频（60）和本次时间限制（毫秒） |
| `max_ticks` / `kill_points` / `survival_points` | 模拟步上限与本局积分权重 |
| `map` | `width`、`height` 是地图尺寸，`walls` 是墙段数组，每段有端点 `a` 和 `b` |
| `geometry` | 以格计的碰撞尺寸，见下文 |
| `params` | 速度、补弹和触墙等规则参数，见下文 |
| `own_tank` | 当前自己的坦克 |
| `other_tanks` | 延迟世界中其他坦克的数组，每项额外有 `seat`，包括已出局的坦克 |
| `projectiles` | 延迟世界中的弹丸数组，每项有 `position`、`velocity`（格/秒）、`kind`（`shell` 或 `laser`）、`owner`、`wall_contacts` |
| `laser_trails` | 延迟世界中的激光轨迹数组，每项有 `start`、`end`、`owner`、`expires_at`（到期步号） |
| `pickups` | 延迟世界中的道具数组，每项含 `x`、`y` 和 `kind`（`laser` 或 `shotgun`） |
| `zone_schedule` | 固定收缩时间表，`start_tick = 1800`，`end_tick = 5400` |
| `safe_zone` | 所见世界步的安全区，`min` 是左上角，`max` 是右下角。收缩开始前为 `null` |

普通坐标对象如 `{"x": 2.5, "y": 1.5}`。坦克的 `pose` 含 `position` 和 `angle`。其余字段是 `alive`（是否存活）、`ammo`（普通弹量）、`refill_at`（下次补弹步号，满弹时为 `null`）、`next_shot`（`null`、`laser` 或 `shotgun`）以及 `laser_loaded`（是否装填激光）。

`geometry` 中的 `tank_length`、`tank_width` 为车身长宽，`wall_thickness` 为墙厚，`muzzle_distance` 为车身中心到炮口的距离。`projectile_radius`、`laser_radius` 为普通弹和激光半径，`pickup_size` 为道具方块边长。

`params` 中的 `forward_secs`、`reverse_secs` 分别是前进或后退一格所需秒数，`turn_secs` 是原地转一圈的秒数，`reload_secs` 是补一发所需秒数，`wall_contacts` 是普通弹的触墙上限，`laser_trail_secs` 是轨迹保留秒数。
