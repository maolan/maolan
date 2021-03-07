#pragma once
#include "maolan/ui/widgets/timetrack.hpp"


namespace maolan::ui
{
  class App;
  class Tracks
  {
    public:
      Tracks();

      void draw();
      void show();
      void hide();
      void toggle();

    protected:
      float width;
      int zoom;
      bool shown;
      TimeTrack timetrack;
  };
}
