#include "imgui.h"
#include "maolan/ui/menu.hpp"


using namespace maolan;


void Menu::draw()
{
  if (ImGui::BeginMainMenuBar())
  {
    if (ImGui::BeginMenu("File"))
    {
      if (ImGui::MenuItem("New")) { }
      if (ImGui::MenuItem("Open")) { }
      if (ImGui::MenuItem("Quit")) { }
      ImGui::EndMenu();
    }
    if (ImGui::BeginMenu("Edit"))
    {
      if (ImGui::MenuItem("Preferences")) { }
      ImGui::EndMenu();
    }
    ImGui::EndMainMenuBar();
  }
}
