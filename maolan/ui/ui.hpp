#pragma once


namespace maolan::ui
{
  class App;
  class UI
  {
    public:
      virtual ~UI();

      virtual void prepare() = 0;
      virtual void render() = 0;
      virtual void run(App *app) = 0;
  };
}
