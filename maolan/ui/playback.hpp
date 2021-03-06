#pragma once
#include "maolan/ui/widgets/playbutton.hpp"


namespace maolan::ui
{
  class Playback
  {
    public:
      void draw();

    protected:
      PlayButton _playButton;
  };
}
