#pragma once

#include <stddef.h>

typedef struct {
    size_t len;
    char *data;
} buffer;

int buffer_push(buffer *into, char value);
