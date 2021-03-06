#include <string>
#include <maolan/config.hpp>
#include <maolan/io.hpp>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/playbutton.hpp"


using namespace maolan::ui;


static const auto state = State::get();


bool PlayButton::draw()
{
  const auto &m = ImGui::CalcTextSize("M", NULL, true);
  const auto &style = ImGui::GetStyle();
  const auto &frame = style.FramePadding;
  bool valueChanged = false;
  auto drawList = ImGui::GetWindowDrawList();
  auto position = ImGui::GetCursorScreenPos();
  const auto &io = ImGui::GetIO();
  ImVec2 size = m;
  size.x += 2 * frame.x;
  size.y += 2 * frame.y;
  auto color = ImGuiCol_Button;
  ImGui::InvisibleButton("playbutton", size);
  auto active = ImGui::IsItemActive();
  auto hovered = ImGui::IsItemHovered();
  if (hovered) { color = ImGuiCol_ButtonHovered; }
  if (active)
  {
    color = ImGuiCol_ButtonActive;
    valueChanged = true;
  }
  position.x += frame.x;
  position.y += frame.y;
  drawList->AddTriangleFilled(
    {position.x, position.y},
    {position.x, position.y + m.y},
    {position.x + m.x, position.y + (m.y / 2)},
    ImGui::ColorConvertFloat4ToU32(style.Colors[color])
  );
  return valueChanged;
}
