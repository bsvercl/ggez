#version 450

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 texcoord;

layout(set = 0, binding = 0, std140) uniform Globals {
    mat4 mvp;
} globals;

layout(location = 0) out vec2 v_texcoord;

void main() {
    gl_Position = globals.mvp * vec4(position, 0.0, 1.0);
    v_texcoord = texcoord;
}