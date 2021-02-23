#include "imgui.h"
#include "imgui_internal.h"
#include "maolan/ui/widgets/draglimit.hpp"




void DragLimit(maolan::audio::Track *t, float &value)
{
  const float &minHeight = 2 * ImGui::GetTextLineHeightWithSpacing() + 3 * ImGui::GetStyle().ItemInnerSpacing.y;
  ImGuiIO &io = ImGui::GetIO();
  auto window = ImGui::GetCurrentWindow();
  ImVec2 size = {window->Pos.x + window->Size.x, 2};

  ImGui::InvisibleButton(t->name().data(), size);
  const bool is_active = ImGui::IsItemActive();
  const bool is_hovered = ImGui::IsItemHovered();
  if (is_hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeNS); }
  if (is_active && io.MouseDelta.y != 0.0f)
  {
    const auto &delta = io.MouseDelta.y;
    if (value <= minHeight && delta < 0) { return; }
    value += io.MouseDelta.y;
  }
}
