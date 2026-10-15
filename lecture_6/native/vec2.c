#include "mini.h"

#include <math.h>
#include <stdio.h>

// Vectors

MiniVec2 mini_vec2_add(MiniVec2 a, MiniVec2 b) {
    return (MiniVec2){a.x + b.x, a.y + b.y};
}

MiniVec2 mini_vec2_scale(MiniVec2 v, float factor) {
    return (MiniVec2){v.x * factor, v.y * factor};
}

float mini_vec2_length(MiniVec2 v) {
    return sqrtf(v.x * v.x + v.y * v.y);
}

// Arrays and text

float mini_sum(const float *values, size_t count) {
    float total = 0.0f;
    for (size_t i = 0; i < count; ++i) {
        total += values[i];
    }
    return total;
}

size_t mini_vec2_format(MiniVec2 v, char *buffer, size_t capacity) {
    // snprintf writes at most capacity bytes, ending in '\0' if there's room for one, and returns the length the whole text needs. Variadic functions take floats as doubles.
    const int needed = snprintf(buffer, capacity, "(%.2f, %.2f)", (double)v.x, (double)v.y);
    return needed < 0 ? 0 : (size_t)needed;
}

// Status codes

const char *mini_status_text(MiniStatus status) {
    switch (status) {
        case MINI_OK:
            return "ok";
        case MINI_INVALID_ARGUMENT:
            return "invalid argument";
        case MINI_INVALID_BODY:
            return "no body with that id";
        case MINI_OUT_OF_MEMORY:
            return "out of memory";
    }
    return "unknown status";
}
