#ifndef ARENA_BOT_H
#define ARENA_BOT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define ARENA_BOT_ABI_VERSION UINT32_C(1)
#define ARENA_FORWARD         UINT32_C(1)
#define ARENA_REVERSE         UINT32_C(2)
#define ARENA_TURN_LEFT       UINT32_C(4)
#define ARENA_TURN_RIGHT      UINT32_C(8)
#define ARENA_FIRE            UINT32_C(16)

uint32_t arena_bot_abi_version(void);

/* Return a bitwise OR of the desired operations, or zero to wait.
 * The UTF-8 JSON data is borrowed for this call, has exactly len bytes
 * and is not null-terminated. Each seat runs in its own process. */
uint32_t arena_bot_decide(const uint8_t *observation, size_t len);

#ifdef __cplusplus
}
#endif

#endif
