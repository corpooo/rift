#include "CRiftMachBridge.h"

#include <mach/mach.h>
#include <servers/bootstrap.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define RIFT_MAX_MESSAGE_SIZE 16384u

typedef struct {
    mach_msg_header_t header;
    uint8_t data[RIFT_MAX_MESSAGE_SIZE];
} rift_inline_message_t;

typedef struct {
    rift_inline_message_t message;
    uint8_t trailer[512];
} rift_receive_buffer_t;

static const char *rift_default_service_name(const char *override_name) {
    if (override_name != NULL && override_name[0] != '\0') {
        return override_name;
    }

    const char *env_name = getenv("RIFT_BS_NAME");
    if (env_name != NULL && env_name[0] != '\0') {
        return env_name;
    }

    return "git.acsandmann.rift";
}

static void rift_set_error(char **out_error, const char *message) {
    if (out_error == NULL) {
        return;
    }

    size_t length = strlen(message);
    char *copy = (char *)malloc(length + 1);
    if (copy == NULL) {
        return;
    }

    memcpy(copy, message, length);
    copy[length] = '\0';
    *out_error = copy;
}

void rift_mach_free_buffer(void *buffer) {
    if (buffer != NULL) {
        free(buffer);
    }
}

bool rift_mach_send_request(
    const char *service_name,
    const char *message,
    uint32_t message_len,
    char **out_data,
    uint32_t *out_len,
    char **out_error
) {
    if (out_data != NULL) {
        *out_data = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_error != NULL) {
        *out_error = NULL;
    }

    if (message == NULL || message_len == 0 || message_len > RIFT_MAX_MESSAGE_SIZE) {
        rift_set_error(out_error, "invalid Mach request payload");
        return false;
    }

    mach_port_t bootstrap_port = MACH_PORT_NULL;
    kern_return_t kr = task_get_special_port(mach_task_self(), TASK_BOOTSTRAP_PORT, &bootstrap_port);
    if (kr != KERN_SUCCESS || bootstrap_port == MACH_PORT_NULL) {
        rift_set_error(out_error, "failed to resolve bootstrap port");
        return false;
    }

    mach_port_t service_port = MACH_PORT_NULL;
    kr = bootstrap_look_up(bootstrap_port, (char *)rift_default_service_name(service_name), &service_port);
    if (kr != KERN_SUCCESS || service_port == MACH_PORT_NULL) {
        rift_set_error(out_error, "Rift Mach service is not registered");
        return false;
    }

    mach_port_t reply_port = MACH_PORT_NULL;
    kr = mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &reply_port);
    if (kr != KERN_SUCCESS || reply_port == MACH_PORT_NULL) {
        rift_set_error(out_error, "failed to allocate Mach reply port");
        mach_port_deallocate(mach_task_self(), service_port);
        return false;
    }

    kr = mach_port_insert_right(
        mach_task_self(),
        reply_port,
        reply_port,
        MACH_MSG_TYPE_MAKE_SEND
    );
    if (kr != KERN_SUCCESS) {
        rift_set_error(out_error, "failed to insert Mach reply right");
        mach_port_mod_refs(mach_task_self(), reply_port, MACH_PORT_RIGHT_RECEIVE, -1);
        mach_port_deallocate(mach_task_self(), reply_port);
        mach_port_deallocate(mach_task_self(), service_port);
        return false;
    }

    uint32_t aligned_len = (message_len + 3u) & ~3u;

    rift_inline_message_t request;
    memset(&request, 0, sizeof(request));
    request.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, MACH_MSG_TYPE_MAKE_SEND);
    request.header.msgh_size = (mach_msg_size_t)(sizeof(mach_msg_header_t) + aligned_len);
    request.header.msgh_remote_port = service_port;
    request.header.msgh_local_port = reply_port;
    request.header.msgh_voucher_port = MACH_PORT_NULL;
    request.header.msgh_id = (mach_msg_id_t)reply_port;
    memcpy(request.data, message, message_len);

    kr = mach_msg(
        &request.header,
        MACH_SEND_MSG,
        request.header.msgh_size,
        0,
        MACH_PORT_NULL,
        MACH_MSG_TIMEOUT_NONE,
        MACH_PORT_NULL
    );
    if (kr != MACH_MSG_SUCCESS) {
        rift_set_error(out_error, "failed to send Mach request");
        mach_port_mod_refs(mach_task_self(), reply_port, MACH_PORT_RIGHT_RECEIVE, -1);
        mach_port_deallocate(mach_task_self(), reply_port);
        mach_port_deallocate(mach_task_self(), service_port);
        return false;
    }

    rift_receive_buffer_t response;
    memset(&response, 0, sizeof(response));
    kr = mach_msg(
        &response.message.header,
        MACH_RCV_MSG,
        0,
        (mach_msg_size_t)sizeof(response),
        reply_port,
        MACH_MSG_TIMEOUT_NONE,
        MACH_PORT_NULL
    );
    if (kr != MACH_MSG_SUCCESS) {
        rift_set_error(out_error, "failed to receive Mach response");
        mach_port_mod_refs(mach_task_self(), reply_port, MACH_PORT_RIGHT_RECEIVE, -1);
        mach_port_deallocate(mach_task_self(), reply_port);
        mach_port_deallocate(mach_task_self(), service_port);
        return false;
    }

    uint32_t payload_len = 0;
    if (response.message.header.msgh_size > sizeof(mach_msg_header_t)) {
        payload_len = response.message.header.msgh_size - (uint32_t)sizeof(mach_msg_header_t);
    }

    char *buffer = (char *)malloc((size_t)payload_len + 1u);
    if (buffer == NULL) {
        rift_set_error(out_error, "failed to allocate response buffer");
        mach_msg_destroy(&response.message.header);
        mach_port_mod_refs(mach_task_self(), reply_port, MACH_PORT_RIGHT_RECEIVE, -1);
        mach_port_deallocate(mach_task_self(), reply_port);
        mach_port_deallocate(mach_task_self(), service_port);
        return false;
    }

    memcpy(buffer, response.message.data, payload_len);
    buffer[payload_len] = '\0';

    mach_msg_destroy(&response.message.header);
    mach_port_mod_refs(mach_task_self(), reply_port, MACH_PORT_RIGHT_RECEIVE, -1);
    mach_port_deallocate(mach_task_self(), reply_port);
    mach_port_deallocate(mach_task_self(), service_port);

    if (out_data != NULL) {
        *out_data = buffer;
    } else {
        free(buffer);
    }

    if (out_len != NULL) {
        *out_len = payload_len;
    }

    return true;
}
