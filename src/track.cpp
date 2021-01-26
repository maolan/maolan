#include "imgui.h"
#include "maolan/ui/track.hpp"


using namespace maolan;


float values[] = {1.0, 0.9, 0.5, 0.2, 0, -0.2, -0.5, -0.9, -1.0, -0.9, -0.5, -0.2, 0, 0.2, 0.5, 0.9, 1.0, 0.9, 0.5, 0.2, 0, -0.2, -0.5, -0.9, -1.0, -0.9, -0.5, -0.2, 0, 0.2, 0.5, 0.9, 1.0};


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
    if (ImGui::Button("M")) { track->mute(!muted); }
    if (!muted) { ImGui::PopStyleColor(); }

    const bool soloed = track->solo();
    ImGui::SameLine();
    if (!soloed)
    {
      ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(ImColor(0, 0, 0)));
    }
    if (ImGui::Button("S")) { track->solo(!soloed); }
    if (!soloed) { ImGui::PopStyleColor(); }

    const bool armed = track->arm();
    ImGui::SameLine();
    if (!armed)
    {
      ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(ImColor(0, 0, 0)));
    }
    if (ImGui::Button("R")) { track->arm(!armed); }
    if (!armed) { ImGui::PopStyleColor(); }
  }
  ImGui::EndGroup();
}
