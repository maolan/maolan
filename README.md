# Startkit for Desktop Application

## Building

```
bin/init.sh
mkdir build
cd build
cmake .. -DCMAKE_BUILD_TYPE=Debug -DGLFW=On
make
./desktop
```

## Requirements

* OpenGL
* GLFW
* imgui (fetched automatically via `bin/init.sh`)
