#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/clip.hpp"


static auto state = maolan::ui::State::get();


void Clip(maolan::audio::Clip *c, const ImVec2 &position, const float &h)
{
  const float &minHeight = state->trackMinHeight;
  const float &height = h < minHeight ? minHeight : h;
  ImDrawList *draw_list = ImGui::GetWindowDrawList();
  ImVec4 color = { 0, 0.8, 0.8, 0.2 };
  const float start = (float)c->start() / (float)state->zoom;
  const float end = (float)c->end() / (float)state->zoom;
  const ImVec2 minimum = {position.x + start, position.y};
  const ImVec2 maximum = {position.x + end, position.y + height};
  ImVec2 size = {end - start, height};
  const ImVec2 inner = {size.x - 2, size.y - 2};

  ImGui::PushClipRect(minimum, maximum, true);
  ImGui::SetCursorScreenPos({minimum.x + 1, minimum.y + 1});
  ImGui::InvisibleButton(c->name().data(), inner);
  draw_list->AddRectFilled(minimum, maximum, ImGui::ColorConvertFloat4ToU32(color), 3);
  draw_list->AddRect(minimum, maximum, ImGui::ColorConvertFloat4ToU32(ImVec4(1, 1, 1, 0.3)), 3);
  draw_list->AddText(minimum, ImGui::GetColorU32(ImGuiCol_Text), c->name().data());
  ImGui::PopClipRect();

  size.x = 2;
  ImGui::SetCursorScreenPos(minimum);
  ImGui::InvisibleButton((c->name() + "min").data(), size);
  const ImGuiIO &io = ImGui::GetIO();
  bool active = ImGui::IsItemActive();
  bool hovered = ImGui::IsItemHovered();
  auto &delta = io.MouseDelta.x;
  if (hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeEW); }
  if (active && delta != 0)
  {
    auto newStart = c->start();
    newStart += delta * state->zoom;
    if (newStart <= 0) { newStart = 1; }
    c->start(newStart);
  }
}
