#!/bin/sh


BIN_DIR=`dirname $0`
PROJECT_DIR="${BIN_DIR}/.."
GH_USERNAME="ocornut"
GH_PROJECT="imgui"
GH_VERSION="1.79"
GH_URL="https://github.com/${GH_USERNAME}/${GH_PROJECT}/archive/v${GH_VERSION}.tar.gz"
TEMP_DIR=`mktemp -d`
OS=`uname`

case ${OS} in
  Linux)
    FETCHCMD="wget ${GH_URL} -O /tmp/imgui.tar.xz"
    ;;
  FreeBSD)
    FETCHCMD="fetch ${GH_URL} -o /tmp/imgui.tar.xz"
    ;;
  *)
    echo "Unsupported OS" >&2
    exit 1
esac

trap "/bin/rm -rf ${TEMP_DIR} /tmp/imgui.tar.xz" HUP KILL INT ABRT BUS TERM EXIT

cd ${PROJECT_DIR}
if [ ! -d "${GH_PROJECT}" ]; then
  ${FETCHCMD}
  tar xfvp "/tmp/${GH_PROJECT}.tar.xz"
  mv "${GH_PROJECT}-${GH_VERSION}" "${GH_PROJECT}"
fi
