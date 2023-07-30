#pragma once
#include <maolan/ui/ui.hpp>
#include <string>

class GLFWwindow;

namespace maolan::ui {
class App;
class GLFW : public UI {
public:
  GLFW(const std::string &title = "Maolan");
  ~GLFW();

  virtual void prepare();
  virtual void render();
  virtual void run(App *app);

protected:
  GLFWwindow *_window;
};
} // namespace maolan::ui
