#include <string>
#include <maolan/config.hpp>
#include <maolan/io.hpp>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/widgets/playhead.hpp"


using namespace maolan::ui;


static const auto state = State::get();
static const auto color = ImGui::ColorConvertFloat4ToU32({ 1, 0, 0, 0.6 });


void PlayHead::draw(const float &width, const float &height)
{
  const auto &playhead = IO::playHead();
  const auto &tempo = Config::tempos[Config::tempoIndex];
  const float delta = tempo.spt / (float)state->zoom;
  auto position = ImGui::GetCursorScreenPos();
  position.x += width;
  position.x += playhead / state->zoom;
  auto drawList = ImGui::GetWindowDrawList();
  drawList->AddTriangleFilled(
    {position.x - 3, position.y},
    {position.x, position.y + height},
    {position.x + 3, position.y},
    color
  );
}
