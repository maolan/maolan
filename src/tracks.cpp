#include <maolan/audio/track.hpp>

#include "imgui.h"
#include "maolan/ui/track.hpp"
#include "maolan/ui/tracks.hpp"


using namespace maolan;


void Tracks::draw()
{
  ImGui::Begin("Tracks");
  {
    for (auto track : audio::Track::all)
    {
      Track t;
      t.draw(track);
    }
  }
  ImGui::End();
}
