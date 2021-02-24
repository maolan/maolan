#include <string>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/clip.hpp"


using namespace maolan::ui;


static auto state = State::get();


Clip::Labels::Labels()
{
  const auto suffix = std::to_string((long)this);
  start = "start" + suffix;
  end = "end" + suffix;
}


Clip::Clip(maolan::audio::Clip *c)
  : _clip{c}
{ c->data(this); }


void Clip::draw(const ImVec2 &position, const float &h)
{
  const float &minHeight = state->trackMinHeight;
  const float &height = h < minHeight ? minHeight : h;
  ImDrawList *draw_list = ImGui::GetWindowDrawList();
  const ImVec4 color = { 0, 0.8, 0.8, 0.2 };
  const float start = (float)_clip->start() / (float)state->zoom;
  const float end = (float)_clip->end() / (float)state->zoom;
  const ImVec2 minimum = {position.x + start, position.y};
  const ImVec2 maximum = {position.x + end, position.y + height};
  ImVec2 size = {end - start, height};

  ImGui::PushClipRect(minimum, maximum, true);
  ImGui::SetCursorScreenPos({minimum.x + 3, minimum.y});
  ImGui::InvisibleButton(_clip->name().data(), {size.x - 6, size.y});
  draw_list->AddRectFilled(minimum, maximum, ImGui::ColorConvertFloat4ToU32(color), 3);
  draw_list->AddRect(minimum, maximum, ImGui::ColorConvertFloat4ToU32(ImVec4(1, 1, 1, 0.3)), 3);
  draw_list->AddText(minimum, ImGui::GetColorU32(ImGuiCol_Text), _clip->name().data());
  ImGui::PopClipRect();

  size.x = 3;
  ImGui::SameLine();
  ImGui::InvisibleButton(labels.end.data(), size);
  auto active = ImGui::IsItemActive();
  auto hovered = ImGui::IsItemHovered();
  const ImGuiIO &io = ImGui::GetIO();
  if (hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeEW); }
  if (active && io.MouseDelta.x != 0)
  {
    auto newEnd = _clip->end();
    newEnd += io.MouseDelta.x * state->zoom;
    if (newEnd <= _clip->start()) { newEnd = _clip->start() + 1; }
    _clip->end(newEnd);
  }

  ImGui::SetCursorScreenPos(minimum);
  ImGui::InvisibleButton(labels.start.data(), size);
  active = ImGui::IsItemActive();
  hovered = ImGui::IsItemHovered();
  if (hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeEW); }
  if (active && io.MouseDelta.x != 0)
  {
    auto newStart = _clip->start();
    newStart += io.MouseDelta.x * state->zoom;
    if (newStart <= 0) { newStart = 1; }
    _clip->start(newStart);
  }
}
