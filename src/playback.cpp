#include <imgui.h>
#include <maolan/engine.hpp>
#include <maolan/ui/playback.hpp>

using namespace maolan::ui;

void Playback::draw() {
  ImGui::Begin("Playback");
  {
    if (_playButton.draw()) {
      Engine::play();
    }
    ImGui::SameLine();
    if (_stopButton.draw()) {
      Engine::stop();
    }
  }
  ImGui::End();
}
