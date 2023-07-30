#pragma once
#include <imgui.h>
#include <maolan/audio/clip.hpp>

namespace maolan::ui {
class Clip {
public:
  class Labels {
  public:
    Labels();

    std::string id;
    std::string start;
    std::string end;
  };
  Clip(maolan::audio::Clip *c);

  void draw(const ImVec2 &position, const float &height);

protected:
  maolan::audio::Clip *_clip;
  Labels labels;
};
} // namespace maolan::ui
