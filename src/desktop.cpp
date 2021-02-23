#include <iostream>
#include <cstdint>
#include <maolan/engine.hpp>
#include <maolan/audio/track.hpp>
#include <maolan/audio/clip.hpp>
#include <maolan/audio/oss/out.hpp>

#include "imgui.h"
#include "maolan/ui/app.hpp"
#include "maolan/ui/state.hpp"
#include "maolan/ui/glfw/ui.hpp"


int main()
{
  maolan::audio::OSSOut<int32_t> out("/dev/dsp", 2);
  maolan::audio::Track one("one", 2);
  maolan::audio::Track two("two with the very long name", 2);
  maolan::audio::Clip clip("../../libmaolan/data/stereo.wav", 0, 624000, 0, &one);
  maolan::audio::Clip clip2("../../libmaolan/data/stereo.wav", 624000, 1248000, 0, &two);
  out.connect(&one);

  auto state = maolan::State::get();
  maolan::Engine::init();
  maolan::UI *display = new maolan::GLFW("maolan");
  auto app = new maolan::App();
  display->run(app);
  maolan::Engine::quit();
  delete display;
  return 0;
}
