#include "imgui.h"
#include "maolan/ui/widgets/grid.hpp"
#include "maolan/ui/track.hpp"


using namespace maolan::ui;


static const ImVec4 color = { 1, 1, 1, 0.2 };


Grid::Grid(Track *t)
  : _track{t}
{}


void Grid::draw()
{
  ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, ImVec2(0.0f, 0.0f));
  const ImVec2 minimum = ImGui::GetCursorScreenPos();
  ImDrawList *draw_list = ImGui::GetWindowDrawList();
  draw_list->AddLine(minimum, {minimum.x, minimum.y + _track->height()}, ImGui::ColorConvertFloat4ToU32(color), 1);
  draw_list->AddLine({minimum.x + 100, minimum.y}, {minimum.x + 100, minimum.y + _track->height()}, ImGui::ColorConvertFloat4ToU32(color), 1);
  ImGui::PopStyleVar();
}
