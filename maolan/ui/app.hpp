#pragma once
#include <maolan/ui/menu.hpp>
#include <maolan/ui/playback.hpp>
#include <maolan/ui/tracks.hpp>
#include <string>

namespace maolan::ui {
class App {
public:
  App();

  static const std::string title;

  void draw();
  Tracks &tracks();

protected:
  Menu _menu;
  Playback _playback;
  Tracks _tracks;
};
} // namespace maolan::ui
