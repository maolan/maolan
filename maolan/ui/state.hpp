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

    protected:
      State();

      static State * state;
  };
}
