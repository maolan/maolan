#pragma once
#include <string>
#include "./menu.hpp"
#include "./playback.hpp"
#include "./tracks.hpp"


namespace maolan::ui
{
  class App
  {
    public:
      App();

      static const std::string title;

      void draw();
      Tracks & tracks();

    protected:
      Menu _menu;
      Playback _playback;
      Tracks _tracks;
  };
}
