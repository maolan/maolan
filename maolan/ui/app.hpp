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

    protected:
      Menu menu;
      Playback playback;
      Tracks tracks;
  };
}
