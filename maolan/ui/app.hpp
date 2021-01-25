#pragma once
#include <string>
#include "./menu.hpp"
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
      Tracks tracks;
  };
}
