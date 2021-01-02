#pragma once


namespace maolan
{
  class State
  {
    public:
      ~State();
      static State * get();

      void init();

      int dummyvar;

    protected:
      State();
      static State * state;
  };
}
