#include "imgui.h"
#include "imgui_internal.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/draglimit.hpp"


static auto state = maolan::State::get();


void DragLimit(maolan::Track *t, float &value)
{
  const float &minHeight = state->trackMinHeight;
  ImGuiIO &io = ImGui::GetIO();
  auto window = ImGui::GetCurrentWindow();
  ImVec2 size = {window->Pos.x + window->Size.x, 2};

  ImGui::InvisibleButton(t->audio()->name().data(), size);
  const bool active = ImGui::IsItemActive();
  const bool hovered = ImGui::IsItemHovered();
  const auto &delta = io.MouseDelta.y;
  if (hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeNS); }
  if (active && delta != 0)
  {
    value += delta;
    if (value < minHeight) { value = minHeight; }
  }
}
