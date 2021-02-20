#include "imgui.h"
#include "maolan/ui/widgets/clip.hpp"


bool Clip(maolan::audio::Clip *c, const ImVec2 &position, const float &height)
{
  ImDrawList *draw_list = ImGui::GetWindowDrawList();
  ImVec2 size = {100, height};
  ImGuiStyle& style = ImGui::GetStyle();
  ImVec4 color = { 0, 0.8, 0.8, 0.2 };
  ImVec2 minimum = position;
  ImVec2 maximum = {position.x + 100, position.y + height};

  ImGui::PushClipRect(minimum, maximum, true);
  ImGui::InvisibleButton(c->name().data(), size);
  bool value_changed = false;
  bool is_active = ImGui::IsItemActive();
  bool is_hovered = ImGui::IsItemHovered();
  draw_list->AddRectFilled(minimum, maximum, ImGui::ColorConvertFloat4ToU32(color), 2);
  draw_list->AddRect(minimum, maximum, ImGui::ColorConvertFloat4ToU32(ImVec4(1, 1, 1, 0.3)), 2);
  draw_list->AddText(position, ImGui::GetColorU32(ImGuiCol_Text), c->name().data());
  ImGui::PopClipRect();

  return value_changed;
}
