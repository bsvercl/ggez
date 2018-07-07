#version 450

layout(location = 0) in vec2 v_texcoord;

layout(location = 0) out vec4 Target0;

layout(set = 0, binding = 1) uniform sampler2D t_texture;

void main() {
    Target0 = texture(t_texture, v_texcoord);
}