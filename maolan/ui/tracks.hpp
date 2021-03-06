#pragma once
#include "maolan/ui/widgets/timetrack.hpp"


namespace maolan::ui
{
  class Tracks
  {
    public:
      Tracks();

      void draw();
      void show();
      void hide();

    protected:
      float width;
      int zoom;
      bool shown;
      TimeTrack timetrack;
  };
}
