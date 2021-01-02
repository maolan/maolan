#!/bin/sh

MY_PATH=`dirname $0`

pkg install -y \
  ccache \
  cmake \
  font-adobe-100dpi \
  glfw \
  liblo \
  pkgconf \
  xorg
cp ${MY_PATH}/.cshrc ~devel/.cshrc
chown devel:devel ~devel/.cshrc
