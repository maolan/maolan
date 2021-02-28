#include <maolan/config.hpp>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/track.hpp"
#include "maolan/ui/widgets/grid.hpp"


using namespace maolan::ui;


static const auto color = ImGui::ColorConvertFloat4ToU32({ 1, 1, 1, 0.2 });
static const auto state = State::get();


Grid::Grid(Track *t)
  : _track{t}
{}


void Grid::draw()
{
  const auto &tempo = Config::tempos[0];
  const float delta = tempo.spt / (float)state->zoom;
  ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, ImVec2(0.0f, 0.0f));
  ImVec2 position = ImGui::GetCursorScreenPos();
  ImDrawList *drawList = ImGui::GetWindowDrawList();
  drawList->AddLine(position, {position.x, position.y + _track->height()}, color, 1);

  position.x += delta;
  drawList->AddLine(position, {position.x, position.y + _track->height()}, color, 1);
  ImGui::PopStyleVar();
}
