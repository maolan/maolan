#include "imgui.h"
#include "maolan/ui/app.hpp"
#include "maolan/ui/menu.hpp"


using namespace maolan::ui;


void Menu::draw(App *app)
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
    if (ImGui::BeginMenu("View"))
    {
      if (ImGui::MenuItem("Tracks")) { app->tracks().toggle(); }
      ImGui::EndMenu();
    }
    ImGui::EndMainMenuBar();
  }
}
