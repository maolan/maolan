#include <maolan/audio/clip.hpp>
#include <maolan/audio/track.hpp>
#include <maolan/engine.hpp>

#include <maolan/ui/app.hpp>
#include <maolan/ui/glfw/ui.hpp>
#include <maolan/ui/state.hpp>

int main() {
  auto state = maolan::ui::State::get();
  maolan::Engine::init();
  auto *display = new maolan::ui::GLFW("maolan");
  auto app = new maolan::ui::App();
  display->run(app);
  maolan::Engine::quit();
  delete display;
  return 0;
}
