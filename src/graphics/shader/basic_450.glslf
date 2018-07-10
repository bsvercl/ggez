#version 450

layout(location = 0) in vec2 v_texcoord;
layout(location = 1) in vec4 v_color;

layout(location = 0) out vec4 Target0;

layout(set = 0, binding = 1) uniform sampler2D tex;

void main() {
    Target0 = texture(tex, v_texcoord) * v_color;
}