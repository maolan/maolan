#include <maolan/audio/track.hpp>

#include "imgui.h"
#include "maolan/ui/state.hpp"
#include "maolan/ui/track.hpp"
#include "maolan/ui/tracks.hpp"


using namespace maolan;


static auto state = State::get();


void Tracks::draw()
{
  ImGui::Begin("Tracks");
  {
    for (auto track : audio::Track::all)
    {
      Track *t = (Track *)track->data();
      t->draw(width);
    }
    ImGui::SliderInt("zoom", &(state->zoom), 1, 10000, "1:%d");
  }
  ImGui::End();
}
