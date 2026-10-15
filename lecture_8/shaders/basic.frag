#version 450

layout(location = 0) in vec3 fragColor;
layout(location = 0) out vec4 outColor;

void main() {
    // Simple fake lighting: brighter colors (by perceived luminance) get more light, on top of a fixed ambient level.
    float ambient = 0.3;
    float brightness = dot(fragColor, vec3(0.299, 0.587, 0.114));
    float light = ambient + (1.0 - ambient) * brightness;

    outColor = vec4(fragColor * light, 1.0);
}
