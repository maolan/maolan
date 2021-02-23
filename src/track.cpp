#include <sstream>
#include "imgui.h"
#include "imgui_internal.h"
#include "maolan/ui/track.hpp"
#include "maolan/ui/widgets/clip.hpp"
#include "maolan/ui/widgets/draglimit.hpp"


using namespace maolan;


Track::Labels::Labels()
{
  const std::string suffix = "##" + std::to_string((long)this);
  mute = "M" + suffix;
  solo = "S" + suffix;
  arm = "R" + suffix;
}


Track::Track(audio::Track *t)
  : track{t}
{}


void Track::draw(float &width)
{
  ImVec2 minimum = ImGui::GetCursorScreenPos();
  ImVec2 maximum = {minimum.x + width, minimum.y + ImGui::GetTextLineHeight()};
  ImGui::BeginGroup();
  {
    ImVec2 m = {maximum.x - 10, maximum.y};
    ImGui::PushClipRect(minimum, m, true);
    ImGui::Text("%s", track->name().data());
    ImGui::PopClipRect();

    const bool muted = track->mute();
    if (!muted)
    {
      ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(ImColor(0, 0, 0)));
    }
    if (ImGui::Button(labels.mute.data())) { track->mute(!muted); }
    if (!muted) { ImGui::PopStyleColor(); }

    const bool soloed = track->solo();
    ImGui::SameLine();
    if (!soloed)
    {
      ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(ImColor(0, 0, 0)));
    }
    if (ImGui::Button(labels.solo.data())) { track->solo(!soloed); }
    if (!soloed) { ImGui::PopStyleColor(); }

    const bool armed = track->arm();
    ImGui::SameLine();
    if (!armed)
    {
      ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(ImColor(0, 0, 0)));
    }
    if (ImGui::Button(labels.arm.data())) { track->arm(!armed); }
    if (!armed) { ImGui::PopStyleColor(); }
  }
  ImGui::EndGroup();

  ImGui::SameLine();
  ImGui::SetCursorScreenPos(ImVec2(maximum.x, minimum.y));
  ImGui::SeparatorEx(ImGuiSeparatorFlags_Vertical);
  ImGui::SameLine();

  ImGui::BeginGroup();
  {
    ImVec2 pos = ImGui::GetCursorScreenPos();
    for (auto clip = track->clips(); clip != nullptr; clip = clip->next())
    {
      ImGui::SameLine();
      Clip(clip, pos, _height);
    }
  }
  ImGui::EndGroup();
  ImGui::Separator();
  DragLimit(track, _height);
}


float Track::height() { return _height; }
void Track::height(float h) { _height = h; }
