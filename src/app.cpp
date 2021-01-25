#include "imgui.h"
#include "maolan/ui/app.hpp"


using namespace maolan;


const std::string App::title = "MaolanApp";


void App::draw()
{
  menu.draw();
  tracks.draw();
}
