#include "arena_bot.h"

uint32_t arena_bot_abi_version(void) {
    return ARENA_BOT_ABI_VERSION;
}

uint32_t arena_bot_decide(const uint8_t *observation, size_t len) {
    /* Read the JSON and choose actions here. Returning zero waits one tick. */
    (void)observation;
    (void)len;
    return 0;
}
