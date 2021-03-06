#include <maolan/config.hpp>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/timetrack.hpp"


using namespace maolan::ui;


static const auto state = State::get();
static const auto spacing = ImVec2(0.0f, 0.0f);
static const auto color = ImGui::ColorConvertFloat4ToU32({ 1, 1, 1, 0.2 });
static const float height = 10;


TimeTrack::TimeTrack()
{}


void TimeTrack::draw(const float &width)
{
  ImGui::BeginGroup();
  {
    const auto &tempo = Config::tempos[0];
    const float delta = tempo.spt / (float)state->zoom;
    ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, spacing);
    auto position = ImGui::GetCursorScreenPos();
    ImGui::InvisibleButton("timetrack", {width, height});
    position.x += width;
    const int bars = ImGui::GetWindowWidth() / delta;
    auto drawList = ImGui::GetWindowDrawList();
    int nth = 1;
    for (; (delta * nth) < 25; ++nth);
    for (int i = 0; i < bars; i += nth)
    {
      drawList->AddLine(position, {position.x, position.y + height}, color, 1);
      position.x += nth * delta;
    }
    ImGui::PopStyleVar();
  }
  ImGui::EndGroup();
}
