#include <string>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/clip.hpp"


using namespace maolan::ui;


static auto state = State::get();
static const ImVec4 color = { 0, 0.8, 0.8, 0.2 };


Clip::Labels::Labels()
{
  id = std::to_string((long)this);
  start = "start" + id;
  end = "end" + id;
}


Clip::Clip(maolan::audio::Clip *c)
  : _clip{c}
{ c->data(this); }


void Clip::draw(const ImVec2 &position, const float &h)
{
  const float &minHeight = state->trackMinHeight;
  const float &height = h < minHeight ? minHeight : h;
  ImDrawList *draw_list = ImGui::GetWindowDrawList();
  const float start = (float)_clip->start() / (float)state->zoom;
  const float end = (float)_clip->end() / (float)state->zoom;
  const ImVec2 minimum = {position.x + start, position.y};
  const ImVec2 maximum = {position.x + end, position.y + height};
  const ImGuiIO &io = ImGui::GetIO();
  ImVec2 size = {end - start, height};

  ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, ImVec2(0.0f, 0.0f));
  ImGui::PushClipRect(minimum, maximum, true);
  ImGui::SetCursorScreenPos({minimum.x + 3, minimum.y});
  ImGui::InvisibleButton(labels.id.data(), {size.x - 6, size.y});
  auto active = ImGui::IsItemActive();
  auto hovered = ImGui::IsItemHovered();
  if (hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_Hand); }
  if (active && io.MouseDelta.x != 0)
  {
    auto delta = io.MouseDelta.x * state->zoom;
    auto newStart = _clip->start() + delta;
    if (delta < 0)
    {
      if (newStart < 0)
      {
        delta -= newStart;
        newStart = 0;
      }
    }
    auto newEnd = _clip->end() + delta;
    _clip->start(newStart);
    _clip->end(newEnd);
  }
  draw_list->AddRectFilled(minimum, maximum, ImGui::ColorConvertFloat4ToU32(color), 3);
  draw_list->AddText(minimum, ImGui::GetColorU32(ImGuiCol_Text), _clip->name().data());
  ImGui::PopClipRect();
  draw_list->AddRect(minimum, maximum, ImGui::ColorConvertFloat4ToU32(ImVec4(1, 1, 1, 0.3)), 3);

  size.x = 3;
  ImGui::SameLine();
  ImGui::InvisibleButton(labels.end.data(), size);
  active = ImGui::IsItemActive();
  hovered = ImGui::IsItemHovered();
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
  ImGui::PopStyleVar();
}
