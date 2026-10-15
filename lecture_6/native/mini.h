#pragma once

#include <stddef.h>
#include <stdint.h>

// A C++ compiler must not mangle these names: C, Rust and C++ all call them by their plain C names.
#ifdef __cplusplus
extern "C" {
#endif

// Constants

#define MINI_VERSION_MAJOR 1
#define MINI_VERSION_MINOR 2

// Vectors, written in C (vec2.c)

typedef struct MiniVec2 {
    float x;
    float y;
} MiniVec2;

MiniVec2 mini_vec2_add(MiniVec2 a, MiniVec2 b);
MiniVec2 mini_vec2_scale(MiniVec2 v, float factor);
float mini_vec2_length(MiniVec2 v);

// Adds up `count` floats starting at `values`.
float mini_sum(const float *values, size_t count);

// Writes v as text into `buffer`: at most `capacity` bytes, ending in '\0' if capacity is at least 1. Returns the length the whole text needs, without the '\0', so a call with a NULL buffer and capacity 0 measures the text.
size_t mini_vec2_format(MiniVec2 v, char *buffer, size_t capacity);

// Status codes

typedef enum MiniStatus {
    MINI_OK = 0,
    MINI_INVALID_ARGUMENT = 1,
    MINI_INVALID_BODY = 2,
    MINI_OUT_OF_MEMORY = 3,
} MiniStatus;

// A message for any status, in static memory: never freed.
const char *mini_status_text(MiniStatus status);

// The world, written in C++ (world.cpp)

typedef enum MiniShape {
    MINI_SHAPE_CIRCLE = 0,
    MINI_SHAPE_BOX = 1,
} MiniShape;

typedef struct MiniBodyDesc {
    MiniShape shape;
    MiniVec2 position;
    MiniVec2 velocity;
    float size;       // a circle's radius, or half a box's side
    float bounciness; // 0 stops dead on the ground, 1 bounces back as high as it fell
} MiniBodyDesc;

typedef struct MiniBody {
    MiniVec2 position;
    MiniVec2 velocity;
    uint32_t bounces;
} MiniBody;

// Opaque: declared here, defined only in world.cpp, so callers can hold a MiniWorld * but never look inside.
typedef struct MiniWorld MiniWorld;

// Called for every bounce, with the `user` pointer given to mini_world_set_contact_callback.
typedef void (*MiniContactFn)(void *user, uint32_t body, float speed);

// NULL if out of memory. Every world must be passed to mini_world_destroy exactly once.
MiniWorld *mini_world_create(MiniVec2 gravity, float ground);
void mini_world_destroy(MiniWorld *world);

MiniStatus mini_world_add_body(MiniWorld *world, const MiniBodyDesc *desc, uint32_t *id);
MiniStatus mini_world_body(const MiniWorld *world, uint32_t id, MiniBody *body);
size_t mini_world_body_count(const MiniWorld *world);

// A NULL callback turns contact reports off.
void mini_world_set_contact_callback(MiniWorld *world, MiniContactFn callback, void *user);
void mini_world_step(MiniWorld *world, float dt);

#ifdef __cplusplus
} // extern "C"
#endif
