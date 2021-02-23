#include "imgui.h"
#include "maolan/ui/widgets/draglimit.hpp"


void DragLimit(maolan::audio::Track *t, float &value)
{
  ImVec2 size = {100, 2};
  ImGuiIO &io = ImGui::GetIO();

  ImGui::InvisibleButton(t->name().data(), size);
  const bool is_active = ImGui::IsItemActive();
  const bool is_hovered = ImGui::IsItemHovered();
  if (is_hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeNS); }
  if (is_active && io.MouseDelta.y != 0.0f) { value += io.MouseDelta.y; }
}
