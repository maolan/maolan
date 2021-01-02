#include "imgui.h"
#include "maolan/app.hpp"


using namespace maolan;


const std::string App::title = "MaolanApp";


void App::draw()
{
  menu.draw();
}
