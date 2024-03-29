#include <imgui.h>
#include <maolan/audio/track.hpp>
#include <maolan/ui/state.hpp>
#include <maolan/ui/track.hpp>
#include <maolan/ui/tracks.hpp>

using namespace maolan::ui;

static auto state = State::get();

Tracks::Tracks() : width{100}, zoom{10}, shown{true} {}

void Tracks::draw() {
  if (shown) {
    ImGui::Begin("Tracks");
    {
      timetrack.draw(width);
      for (auto track : audio::Track::all()) {
        Track *t = (Track *)track->data();
        if (t->height() < state->trackMinHeight) {
          t->height(state->trackMinHeight);
        }
        t->draw(width);
      }
      if (ImGui::SliderInt("zoom", &zoom, 0, 30)) {
        state->zoom = 1 << zoom;
      }
    }
    ImGui::End();
  }
}

void Tracks::show() { shown = true; }
void Tracks::hide() { shown = false; }
void Tracks::toggle() { shown = !shown; }
