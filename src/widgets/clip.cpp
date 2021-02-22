#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/clip.hpp"


static auto state = maolan::State::get();


bool Clip(maolan::audio::Clip *c, const ImVec2 &position, const float &height)
{
  ImDrawList *draw_list = ImGui::GetWindowDrawList();
  ImVec2 size = {100, height};
  ImGuiStyle& style = ImGui::GetStyle();
  ImVec4 color = { 0, 0.8, 0.8, 0.2 };
  const float start = (float)c->start() / (float)state->zoom;
  const float end = (float)c->end() / (float)state->zoom;
  const ImVec2 minimum = {position.x + start, position.y};
  const ImVec2 maximum = {position.x + end, position.y + height};

  ImGui::PushClipRect(minimum, maximum, true);
  ImGui::InvisibleButton(c->name().data(), size);
  bool value_changed = false;
  const bool is_active = ImGui::IsItemActive();
  const bool is_hovered = ImGui::IsItemHovered();
  draw_list->AddRectFilled(minimum, maximum, ImGui::ColorConvertFloat4ToU32(color), 3);
  draw_list->AddRect(minimum, maximum, ImGui::ColorConvertFloat4ToU32(ImVec4(1, 1, 1, 0.3)), 3);
  draw_list->AddText(minimum, ImGui::GetColorU32(ImGuiCol_Text), c->name().data());
  ImGui::PopClipRect();

  return value_changed;
}
