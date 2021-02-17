#include <iostream>
#include <maolan/engine.hpp>
#include <maolan/audio/track.hpp>
#include <maolan/audio/clip.hpp>
#include <maolan/audio/oss/out.hpp>

#include "maolan/ui/app.hpp"
#include "maolan/ui/glfw/ui.hpp"


int main()
{
  maolan::audio::OSSOut out("/dev/dsp", 2);
  maolan::audio::Track one("one", 2);
  maolan::audio::Clip clip("../../libmaolan/data/stereo.wav", 0, 10000000, 0, &one);
  out.connect(&one);

  maolan::Engine::init();
  maolan::UI *display = new maolan::GLFW("maolan");
  auto app = new maolan::App();
  display->run(app);
  maolan::Engine::quit();
  delete display;
  return 0;
}
