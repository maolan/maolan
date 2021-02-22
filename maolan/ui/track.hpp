#pragma once
#include <string>
#include <maolan/audio/track.hpp>


namespace maolan
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
      void draw();

    protected:
      Labels labels;
      float height = 20;
      audio::Track *track;
  };
}
