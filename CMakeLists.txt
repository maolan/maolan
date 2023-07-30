cmake_minimum_required(VERSION 3.10)
project(maolan VERSION 0.0.1 DESCRIPTION "Maolan DAW")

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)
include(GNUInstallDirs)

file(GLOB SRCS src/*.cpp src/glfw/*.cpp src/widgets/*.cpp)
file(GLOB MY_HEADERS maolan/ui/*.hpp)
install(FILES ${MY_HEADERS} DESTINATION ${CMAKE_INSTALL_PREFIX}/include/maolan/ui)
file(GLOB MY_WIDGET_HEADERS maolan/ui/widgets/*.hpp)
install(FILES ${MY_WIDGET_HEADERS} DESTINATION ${CMAKE_INSTALL_PREFIX}/include/maolan/ui/widgets)

enable_testing()

find_package(PkgConfig REQUIRED)

set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -DIMGUI_IMPL_OPENGL_LOADER_GL3W")
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -DIMGUI_IMPL_OPENGL_LOADER_GL3W")

pkg_check_modules(MAOLAN REQUIRED libmaolan)
set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${MAOLAN_INCLUDE_DIRS})
set(MY_LIBRARY_DIRS ${MY_LIBRARY_DIRS} ${MAOLAN_LIBRARY_DIRS})
set(MY_LIBRARIES ${MY_LIBRARIES} ${MAOLAN_LIBRARIES})

pkg_check_modules(GL REQUIRED gl)
set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${GL_INCLUDE_DIRS})
set(MY_LIBRARY_DIRS ${MY_LIBRARY_DIRS} ${GL_LIBRARY_DIRS})
set(MY_LIBRARIES ${MY_LIBRARIES} ${GL_LIBRARIES})

pkg_check_modules(GLFW3 REQUIRED glfw3)
set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${GLFW3_INCLUDE_DIRS})
set(MY_LIBRARY_DIRS ${MY_LIBRARY_DIRS} ${GLFW3_LIBRARY_DIRS})
set(MY_LIBRARIES ${MY_LIBRARIES} ${GLFW3_LIBRARIES})

include_directories(${CMAKE_CURRENT_SOURCE_DIR} ${CMAKE_INSTALL_PREFIX}/include ${MY_INCLUDE_DIRS})
add_executable(maolan-bin ${SRCS} ${MY_HEADERS})
set_target_properties(maolan-bin PROPERTIES OUTPUT_NAME maolan)
target_link_libraries(maolan-bin ${MY_LIBRARIES} ${CMAKE_DL_LIBS} imgui)
target_link_directories(maolan-bin PUBLIC ${MY_LIBRARY_DIRS})
install(TARGETS maolan-bin RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
