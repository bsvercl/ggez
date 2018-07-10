#version 450

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 texcoord;

layout(location = 2) in vec4 src;
layout(location = 3) in vec4 col1;
layout(location = 4) in vec4 col2;
layout(location = 5) in vec4 col3;
layout(location = 6) in vec4 col4;
layout(location = 7) in vec4 color;

layout(set = 0, binding = 0, std140) uniform Globals {
    mat4 mvp;
} globals;

layout(location = 0) out vec2 v_texcoord;
layout(location = 1) out vec4 v_color;

void main() {
    v_texcoord = texcoord * src.zw + src.xy;
    v_color = color;
    mat4 instance_transform = mat4(col1, col2, col3, col4);
    vec4 position = instance_transform * vec4(position, 0.0, 1.0);

    gl_Position = globals.mvp * position;
}