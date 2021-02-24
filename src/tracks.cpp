#include <maolan/audio/track.hpp>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/track.hpp"
#include "maolan/ui/tracks.hpp"


using namespace maolan::ui;


static auto state = State::get();
static int zoom = 0;


void Tracks::draw()
{
  ImGui::Begin("Tracks");
  {
    for (auto track : audio::Track::all)
    {
      Track *t = (Track *)track->data();
      t->draw(width);
    }
    if (ImGui::SliderInt("zoom", &zoom, 0, 31))
    {
      state->zoom = 1 << zoom;
    }
  }
  ImGui::End();
}
