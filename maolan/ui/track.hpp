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
      void draw(audio::Track *track);

    protected:
      Labels labels;
      float height = 20;
  };
}
