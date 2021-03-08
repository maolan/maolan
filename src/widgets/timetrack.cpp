#include <string>
#include <maolan/config.hpp>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/timetrack.hpp"


using namespace maolan::ui;


static const auto state = State::get();
static const auto spacing = ImVec2(0.0f, 0.0f);
static const auto color = ImGui::ColorConvertFloat4ToU32({ 1, 1, 1, 0.2 });
static const float height = 15;


void TimeTrack::draw(const float &width)
{
  _playhead.draw(width, height);
  ImGui::BeginGroup();
  {
    const auto &tempo = Config::tempos[Config::tempoIndex];
    const float delta = tempo.spt / (float)state->zoom;
    ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, spacing);
    auto position = ImGui::GetCursorScreenPos();
    ImGui::InvisibleButton("timetrack", {width, height});
    position.x += width;
    const int bars = ImGui::GetWindowWidth() / delta;
    auto drawList = ImGui::GetWindowDrawList();
    int nth = 0;
    if (delta > 25) { nth = 1; }
    else { for (; (delta * nth) < 25; nth += 4); }
    for (int i = 0; i < bars; i += nth)
    {
      drawList->AddLine(position, {position.x, position.y + height}, color, 1);
      drawList->AddText({position.x + 3, position.y}, color, std::to_string(i+1).data());
      position.x += nth * delta;
    }
    ImGui::PopStyleVar();
  }
  ImGui::EndGroup();
}
