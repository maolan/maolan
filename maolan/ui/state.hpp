#pragma once


namespace maolan
{
  class State
  {
    public:
      ~State();

      static State * get();

      int zoom;

    protected:
      State();

      static State * state;
  };
}
