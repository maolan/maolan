#pragma once
#include <string>
#include "./menu.hpp"


namespace maolan
{
  class App
  {
    public:
      void draw();
      static const std::string title;

    protected:
      Menu menu;
  };
}
