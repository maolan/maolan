#include "imgui.h"
#include "maolan/audio/track.hpp"
#include "maolan/ui/tracks.hpp"


using namespace maolan;


float values[] = {1.0, 0.9, 0.5, 0.2, 0, -0.2, -0.5, -0.9, -1.0, -0.9, -0.5, -0.2, 0, 0.2, 0.5, 0.9, 1.0, 0.9, 0.5, 0.2, 0, -0.2, -0.5, -0.9, -1.0, -0.9, -0.5, -0.2, 0, 0.2, 0.5, 0.9, 1.0};


void Tracks::draw()
{
  ImGui::Begin("Tracks");
  {
    ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, ImVec2(0.0, 0.0));
    ImGui::PushStyleVar(ImGuiStyleVar_FramePadding, ImVec2(0.0, 3.0));
    ImGui::PlotLines("", values, IM_ARRAYSIZE(values), 0, "Track 1", -1.0, 1.0, ImVec2(150.0, 80.0));
    ImGui::SameLine();
    ImGui::PlotLines("", values, IM_ARRAYSIZE(values), 0, "Track 1", -1.0, 1.0, ImVec2(150.0, 80.0));
    ImGui::PopStyleVar(2);
  }
  ImGui::End();
}
