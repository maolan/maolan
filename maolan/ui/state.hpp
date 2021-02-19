#pragma once


namespace maolan
{
  class State
  {
    public:
      ~State();

      static State * get();

    protected:
      State();
      static State * state;
  };
}
