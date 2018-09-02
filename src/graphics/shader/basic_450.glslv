#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(set = 0, binding = 0, std140) uniform Globals {
    mat4 u_MVP;
};

layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_Uv;
layout(location = 2) in vec4 a_VertColor;

layout(location = 3) in vec4 a_Src;
layout(location = 4) in vec4 a_TCol1;
layout(location = 5) in vec4 a_TCol2;
layout(location = 6) in vec4 a_TCol3;
layout(location = 7) in vec4 a_TCol4;
layout(location = 8) in vec4 a_Color;

layout(location = 0) out vec2 v_Uv;
layout(location = 1) out vec4 v_Color;

void main() {
    v_Uv = a_Uv * a_Src.zw + a_Src.xy;
    v_Color = a_Color * a_VertColor;
    mat4 instance_transform = mat4(a_TCol1, a_TCol2, a_TCol3, a_TCol4);
    vec4 position = instance_transform * vec4(a_Pos, 0.0, 1.0);

    gl_Position = u_MVP * position;
}