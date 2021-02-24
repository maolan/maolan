#pragma once
#include <string>
#include <maolan/audio/track.hpp>


namespace maolan::ui
{
  class Track
  {
    class Labels
    {
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
      audio::Track * audio();

    protected:
      Labels labels;
      float _height = 20;
      audio::Track *_track;
  };
}
