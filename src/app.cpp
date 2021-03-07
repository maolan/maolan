#include <maolan/audio/track.hpp>

#include "maolan/ui/app.hpp"
#include "maolan/ui/track.hpp"


using namespace maolan::ui;


const std::string App::title = "MaolanApp";


App::App()
{
  for (auto track : audio::Track::all)
  {
    Track *t = (Track *)track->data();
    if (!t)
    {
      t = new Track(track);
      track->data(t);
    }
  }
}


void App::draw()
{
  _menu.draw(this);
  _tracks.draw();
  _playback.draw();
}


Tracks & App::tracks() { return _tracks; }
