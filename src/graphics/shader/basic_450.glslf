#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(set = 0, binding = 1) uniform sampler2D t_Texture;

layout(location = 0) in vec2 v_Uv;
layout(location = 1) in vec4 v_Color;

layout(location = 0) out vec4 Target0;

void main() {
    Target0 = texture(t_Texture, v_Uv) * v_Color;
}