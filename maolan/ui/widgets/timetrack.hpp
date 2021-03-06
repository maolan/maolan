#pragma once
#include "maolan/ui/widgets/playhead.hpp"


namespace maolan::ui
{
  class TimeTrack
  {
    public:
      void draw(const float &width);

    protected:
      PlayHead _playhead;
  };
}
