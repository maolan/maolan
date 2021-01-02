#include <iostream>
#include "maolan/app.hpp"
#include "maolan/glfw/ui.hpp"
#include <lo/lo_cpp.h>


int main()
{
  // lo::ServerThread st(10024);
  // if (!st.is_valid()) {
      // std::cerr << "Failed creating server thread" << std::endl;
      // return 1;
  // }
  // st.set_callbacks([&st](){printf("Thread init: %p.\n",&st);},
                   // [](){printf("Thread cleanup.\n");});
  // std::cout << "URL: " << st.url() << std::endl;
  // st.add_method(nullptr, nullptr, []{std::cout << "example" << std::endl;});
  // st.start();

  maolan::UI *display = new maolan::GLFW("maolan");
  auto app = new maolan::App();
  display->run(app);
  delete display;
  return 0;
}
