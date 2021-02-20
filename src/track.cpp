#include <sstream>
#include "imgui.h"
#include "imgui_internal.h"
#include "maolan/ui/track.hpp"
#include "maolan/ui/widgets/clip.hpp"


using namespace maolan;


Track::Labels::Labels()
{
  const std::string suffix = "##" + std::to_string((long)this);
  mute = "M" + suffix;
  solo = "S" + suffix;
  arm = "R" + suffix;
}


void Track::draw(audio::Track *track)
{
  ImGui::BeginGroup();
  {
    ImGui::Text("%s", track->name().data());

    const bool muted = track->mute();
    ImGui::SameLine();
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
  ImGui::SeparatorEx(ImGuiSeparatorFlags_Vertical);
  ImGui::SameLine();

  ImGui::BeginGroup();
  {
    ImVec2 pos = ImGui::GetCursorScreenPos();
    for (auto clip = track->clips(); clip != nullptr; clip = clip->next())
    {
      ImGui::SameLine();
      Clip(clip, pos, height);
    }
  }
  ImGui::EndGroup();
  ImGui::Separator();
}
