#ifndef CRiftMachBridge_h
#define CRiftMachBridge_h

#include <stdbool.h>
#include <stdint.h>

bool rift_mach_send_request(
    const char *service_name,
    const char *message,
    uint32_t message_len,
    char **out_data,
    uint32_t *out_len,
    char **out_error
);

void rift_mach_free_buffer(void *buffer);

#endif
