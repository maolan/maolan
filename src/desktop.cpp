#include <iostream>
#include <maolan/audio/track.hpp>
#include <maolan/audio/oss/out.hpp>

#include "maolan/ui/app.hpp"
#include "maolan/ui/glfw/ui.hpp"


int main()
{
  maolan::audio::OSSOut out("/dev/dsp", 2);
  maolan::audio::Track one("one", 2);
  one.mute(true);
  maolan::audio::Track two("two", 2);

  maolan::UI *display = new maolan::GLFW("maolan");
  auto app = new maolan::App();
  display->run(app);
  delete display;
  return 0;
}
