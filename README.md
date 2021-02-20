# Maolan, Modern and Bloat Free DAW

## Building

```
bin/init.sh
mkdir build
cd build
cmake .. -DCMAKE_BUILD_TYPE=Debug -DGLFW=On
make
./maolan
```

## Requirements

* OpenGL
* GLFW
* imgui (fetched automatically via `bin/init.sh`)
* libmaolan
