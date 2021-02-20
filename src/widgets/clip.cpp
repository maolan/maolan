#include "imgui.h"
#include "maolan/ui/widgets/clip.hpp"


bool Clip(std::string_view label, const ImVec2 &position, const float &height, maolan::audio::Clip *c)
{
  ImDrawList *draw_list = ImGui::GetWindowDrawList();
  ImVec2 size = {100, height};

  ImGui::InvisibleButton(label.data(), size);
  bool value_changed = false;
  bool is_active = ImGui::IsItemActive();
  bool is_hovered = ImGui::IsItemHovered();
  draw_list->AddText(position, ImGui::GetColorU32(ImGuiCol_Text), label.data());

  return value_changed;
}
