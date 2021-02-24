#pragma once


namespace maolan
{
  class State
  {
    public:
      ~State();

      static State * get();

      int zoom;
      float trackMinHeight;
      float trackMinWidth = 100;

    protected:
      State();

      static State * state;
  };
}
