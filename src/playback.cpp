#include <maolan/engine.hpp>

#include "imgui.h"
#include "maolan/ui/playback.hpp"


using namespace maolan::ui;


void Playback::draw()
{
  ImGui::Begin("Playback");
  {
    if (_playButton.draw()) { Engine::play(); }
    ImGui::SameLine();
    if (ImGui::Button("Stop")) { Engine::stop(); }
  }
  ImGui::End();
}
