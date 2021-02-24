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
  ImVec4 color = { 0, 0.8, 0.8, 0.2 };
  const float start = (float)_clip->start() / (float)state->zoom;
  const float end = (float)_clip->end() / (float)state->zoom;
  const ImVec2 minimum = {position.x + start, position.y};
  const ImVec2 maximum = {position.x + end, position.y + height};
  ImVec2 size = {end - start, height};
  const ImVec2 inner = {size.x - 6, size.y};

  ImGui::PushClipRect(minimum, maximum, true);
  ImGui::SetCursorScreenPos({minimum.x + 1, minimum.y + 1});
  ImGui::InvisibleButton(_clip->name().data(), inner);
  draw_list->AddRectFilled(minimum, maximum, ImGui::ColorConvertFloat4ToU32(color), 3);
  draw_list->AddRect(minimum, maximum, ImGui::ColorConvertFloat4ToU32(ImVec4(1, 1, 1, 0.3)), 3);
  draw_list->AddText(minimum, ImGui::GetColorU32(ImGuiCol_Text), _clip->name().data());
  ImGui::PopClipRect();

  size.x = 3;
  ImGui::SetCursorScreenPos(minimum);
  ImGui::InvisibleButton(labels.start.data(), size);
  const ImGuiIO &io = ImGui::GetIO();
  bool active = ImGui::IsItemActive();
  bool hovered = ImGui::IsItemHovered();
  auto &delta = io.MouseDelta.x;
  if (hovered) { ImGui::SetMouseCursor(ImGuiMouseCursor_ResizeEW); }
  if (active && delta != 0)
  {
    auto newStart = _clip->start();
    newStart += delta * state->zoom;
    if (newStart <= 0) { newStart = 1; }
    _clip->start(newStart);
  }
}
