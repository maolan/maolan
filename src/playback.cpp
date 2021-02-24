#include <maolan/engine.hpp>

#include "imgui.h"
#include "maolan/ui/playback.hpp"


using namespace maolan::ui;


void Playback::draw()
{
  ImGui::Begin("Playback");
  {
    if (ImGui::Button("Play")) { Engine::play(); }
    ImGui::SameLine();
    if (ImGui::Button("Stop")) { Engine::stop(); }
  }
  ImGui::End();
}
