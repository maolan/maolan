#pragma once
#include <string>
#include "./menu.hpp"
#include "./playback.hpp"
#include "./tracks.hpp"


namespace maolan
{
  class App
  {
    public:
      void draw();
      static const std::string title;

    protected:
      Menu menu;
      Playback playback;
      Tracks tracks;
  };
}
