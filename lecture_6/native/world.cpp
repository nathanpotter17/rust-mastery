#include "mini.h"

#include <cstddef>
#include <cstdint>
#include <new>
#include <vector>

// The C++ class

// Defined here and nowhere else, so C and Rust only ever see a MiniWorld *. Inside, it's ordinary C++: a class with a std::vector and member functions.
struct MiniWorld {
    struct Body {
        MiniShape shape;
        MiniVec2 position;
        MiniVec2 velocity;
        float size;
        float bounciness;
        std::uint32_t bounces = 0;
    };

    // A bounce slower than this settles the body on the ground instead.
    static constexpr float rest_speed = 0.5f;

    MiniVec2 gravity;
    float ground;
    std::vector<Body> bodies;
    MiniContactFn on_contact = nullptr;
    void *user = nullptr;

    void step(float dt) {
        for (std::size_t i = 0; i < bodies.size(); ++i) {
            Body &body = bodies[i];
            body.velocity = mini_vec2_add(body.velocity, mini_vec2_scale(gravity, dt));
            body.position = mini_vec2_add(body.position, mini_vec2_scale(body.velocity, dt));

            // Above the ground, or rising: nothing more to do.
            if (body.position.y - body.size >= ground || body.velocity.y >= 0.0f) {
                continue;
            }

            // Through the ground while falling: put the body back on top, and bounce.
            const float speed = -body.velocity.y;
            body.position.y = ground + body.size;
            body.velocity.y = speed >= rest_speed ? speed * body.bounciness : 0.0f;

            // Boxes slide, and friction halves their sideways speed at every touch; circles roll on.
            if (body.shape == MINI_SHAPE_BOX) {
                body.velocity.x *= 0.5f;
            }

            if (speed >= rest_speed) {
                ++body.bounces;
                if (on_contact != nullptr) {
                    on_contact(user, static_cast<std::uint32_t>(i), speed);
                }
            }
        }
    }
};

// The C interface

// No C++ exception may escape into C or Rust: there is no way to catch one on the other side. Each function turns failure into NULL or a status code.
extern "C" {

    MiniWorld *mini_world_create(MiniVec2 gravity, float ground) {
        // new (std::nothrow) returns nullptr instead of throwing std::bad_alloc.
        return new (std::nothrow) MiniWorld{gravity, ground, {}, nullptr, nullptr};
    }

    void mini_world_destroy(MiniWorld *world) {
        delete world;
    }

    MiniStatus mini_world_add_body(MiniWorld *world, const MiniBodyDesc *desc, std::uint32_t *id) {
        if (world == nullptr || desc == nullptr || id == nullptr) {
            return MINI_INVALID_ARGUMENT;
        }

        // Written so that NaN, which fails every comparison, is rejected too.
        if (!(desc->size > 0.0f) || !(desc->bounciness >= 0.0f && desc->bounciness <= 1.0f)) {
            return MINI_INVALID_ARGUMENT;
        }

        try {
            world->bodies.push_back({desc->shape, desc->position, desc->velocity, desc->size, desc->bounciness});
        } catch (const std::bad_alloc&) {
            return MINI_OUT_OF_MEMORY;
        }

        *id = static_cast<std::uint32_t>(world->bodies.size() - 1);
        return MINI_OK;
    }

    MiniStatus mini_world_body(const MiniWorld *world, std::uint32_t id, MiniBody *body) {
        if (world == nullptr || body == nullptr) {
            return MINI_INVALID_ARGUMENT;
        }
        if (id >= world->bodies.size()) {
            return MINI_INVALID_BODY;
        }

        const MiniWorld::Body &found = world->bodies[id];
        *body = MiniBody{found.position, found.velocity, found.bounces};
        return MINI_OK;
    }

    std::size_t mini_world_body_count(const MiniWorld *world) {
        return world == nullptr ? 0 : world->bodies.size();
    }

    void mini_world_set_contact_callback(MiniWorld *world, MiniContactFn callback, void *user) {
        if (world != nullptr) {
            world->on_contact = callback;
            world->user = user;
        }
    }

    void mini_world_step(MiniWorld *world, float dt) {
        if (world != nullptr) {
            world->step(dt);
        }
    }

} // extern "C"
