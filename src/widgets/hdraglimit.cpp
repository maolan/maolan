#include "imgui.h"
#include "imgui_internal.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/draglimit.hpp"


static auto state = maolan::State::get();


void HDragLimit(maolan::Track *t, float &value)
{
  const float &minWidth = state->trackMinWidth;
  ImGuiIO &io = ImGui::GetIO();
  ImVec2 size = {2, t->height()};

  ImGui::InvisibleButton((t->audio()->name() + "H").data(), size);
  const bool active = ImGui::IsItemActive();
  const bool hovered = ImGui::IsItemHovered();
  const auto &delta = io.MouseDelta.x;
  if (hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeEW); }
  if (active && delta != 0)
  {
    value += delta;
    if (value < minWidth) { value = minWidth; }
  }
}
