#include <iomanip>
#include <sstream>
#include <math.h>
#include "imgui.h"
#include "maomix/knob.hpp"


static float ANGLE_MIN = 3.141592 * 0.75;
static float ANGLE_MAX = 3.141592 * 2.25;


bool Knob(const char *label, float radius, float &p_value, const float &v_min, const float &v_max)
{
  std::stringstream s;
  s << std::fixed << std::setprecision(2) << p_value;
  ImGuiIO& io = ImGui::GetIO();
  ImGuiStyle& style = ImGui::GetStyle();

  ImVec2 pos = ImGui::GetCursorScreenPos();
  ImVec2 center = ImVec2(pos.x + radius, pos.y + radius);
  float line_height = ImGui::GetTextLineHeight();
  ImDrawList* draw_list = ImGui::GetWindowDrawList();

  ImGui::InvisibleButton(label, ImVec2(radius*2, radius*2 + line_height + style.ItemInnerSpacing.y));
  bool value_changed = false;
  bool is_active = ImGui::IsItemActive();
  bool is_hovered = ImGui::IsItemHovered();
  if (is_active && io.MouseDelta.y != 0.0f)
  {
    float step = (v_max - v_min) / 200.0f;
    p_value += (-io.MouseDelta.y) * step;
    if (p_value < v_min) p_value = v_min;
    if (p_value > v_max) p_value = v_max;
    value_changed = true;
  }

  float t = (p_value - v_min) / (v_max - v_min);
  float angle = ANGLE_MIN + (ANGLE_MAX - ANGLE_MIN) * t;
  float angle_cos = cosf(angle), angle_sin = sinf(angle);
  float radius_inner = radius*0.40f;
  draw_list->AddCircleFilled(center, radius, ImGui::GetColorU32(ImGuiCol_FrameBg), 16);
  draw_list->AddLine(ImVec2(center.x + angle_cos*radius_inner, center.y + angle_sin*radius_inner), ImVec2(center.x + angle_cos*(radius-2), center.y + angle_sin*(radius-2)), ImGui::GetColorU32(ImGuiCol_SliderGrabActive), 2.0f);
  draw_list->AddCircleFilled(center, radius_inner, ImGui::GetColorU32(is_active ? ImGuiCol_FrameBgActive : is_hovered ? ImGuiCol_FrameBgHovered : ImGuiCol_FrameBg), 16);
  draw_list->AddText(ImVec2(pos.x, pos.y + radius * 2 + style.ItemInnerSpacing.y), ImGui::GetColorU32(ImGuiCol_Text), s.str().data());

  return value_changed;
}
