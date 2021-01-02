cmake_minimum_required(VERSION 3.10)
project(maolan VERSION 0.0.1 DESCRIPTION "Maolan")

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)

file(GLOB SRCS src/*.cpp)
file(GLOB HEADERS maolan/*.hpp)
install(FILES ${HEADERS} DESTINATION ${CMAKE_INSTALL_PREFIX}/include/maolan)
set(MY_HEADERS ${MY_HEADERS} ${HEADERS})
set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${CMAKE_CURRENT_SOURCE_DIR}/imgui)
set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${CMAKE_CURRENT_SOURCE_DIR}/imgui/examples)
set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} )
include(GNUInstallDirs)

file(GLOB IMGUI_SRCS
    imgui/imgui.cpp
    imgui/imgui_draw.cpp
    imgui/imgui_widgets.cpp
    imgui/examples/imgui_impl_glfw.cpp
    imgui/examples/imgui_impl_opengl3.cpp
    imgui/examples/libs/gl3w/GL/gl3w.c
 )
enable_testing()

find_package(PkgConfig REQUIRED)

pkg_check_modules(LO REQUIRED liblo)
set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${LO_INCLUDE_DIRS})
set(MY_LIBRARY_DIRS ${MY_LIBRARY_DIRS} ${LO_LIBRARY_DIRS})
set(MY_LIBRARIES ${MY_LIBRARIES} ${LO_LIBRARIES})

set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -DIMGUI_IMPL_OPENGL_LOADER_GL3W")
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -DIMGUI_IMPL_OPENGL_LOADER_GL3W")

option(GLFW "GLFW" OFF)
if (GLFW)
    file(GLOB GLFW_SRCS src/glfw/*.cpp)
    file(GLOB GLFW_HEADERS maolan/glfw/*.hpp)
    set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} maolan/glfw)
    set(SRCS ${SRCS} ${GLFW_SRCS})
    install(FILES ${GLFW_HEADERS} DESTINATION ${CMAKE_INSTALL_PREFIX}/include/maolan/glfw)
    set(MY_HEADERS ${MY_HEADEARS} ${GLFW_HEADERS})

    pkg_check_modules(GL REQUIRED gl)
    set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${GL_INCLUDE_DIRS})
    set(MY_LIBRARY_DIRS ${MY_LIBRARY_DIRS} ${GL_LIBRARY_DIRS})
    set(MY_LIBRARIES ${MY_LIBRARIES} ${GL_LIBRARIES})

    pkg_check_modules(GLFW3 REQUIRED glfw3)
    set(MY_INCLUDE_DIRS ${MY_INCLUDE_DIRS} ${GLFW3_INCLUDE_DIRS})
    set(MY_LIBRARY_DIRS ${MY_LIBRARY_DIRS} ${GLFW3_LIBRARY_DIRS})
    set(MY_LIBRARIES ${MY_LIBRARIES} ${GLFW3_LIBRARIES})
endif()

include_directories(${CMAKE_CURRENT_SOURCE_DIR} ${CMAKE_CURRENT_SOURCE_DIR}/imgui/examples/libs/gl3w ${CMAKE_INSTALL_PREFIX}/include ${MY_INCLUDE_DIRS})
add_executable(maolan ${SRCS} ${IMGUI_SRCS} ${MY_HEADERS})
target_link_libraries(maolan ${MY_LIBRARIES} ${CMAKE_DL_LIBS})
target_link_directories(maolan PUBLIC ${MY_LIBRARY_DIRS})
install(TARGETS maolan RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
