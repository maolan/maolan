#pragma once
#include <maolan/audio/track.hpp>
#include <maolan/ui/widgets/grid.hpp>
#include <string>

namespace maolan::ui {
class Track {
  class Labels {
  public:
    Labels();

    std::string mute;
    std::string solo;
    std::string arm;
  };

public:
  Track(audio::Track *track);

  void draw(float &width);
  float height();
  void height(float h);
  audio::Track *audio();

protected:
  Labels labels;
  Grid grid;
  float _height = 20;
  audio::Track *_track;
};
} // namespace maolan::ui
