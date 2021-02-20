#include <string_view>
#include <maolan/audio/clip.hpp>
#include "imgui.h"


bool Clip(std::string_view label, const ImVec2 &position, const float &height, maolan::audio::Clip *c);
