#pragma once
#include "maolan/ui/widgets/playbutton.hpp"
#include "maolan/ui/widgets/stopbutton.hpp"


namespace maolan::ui
{
  class Playback
  {
    public:
      void draw();

    protected:
      PlayButton _playButton;
      StopButton _stopButton;
  };
}
