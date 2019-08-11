#include <gnui/Box.h>
#include <gnui/Button.h>
#include <gnui/InvisibleBox.h>
#include <gnui/ProgressBar.h>
#include <gnui/Symbol.h>
#include <gnui/Widget.h>
#include <gnui/Window.h>
#include <gnui/draw.h>
#include <gnui/run.h>

#include <fcntl.h>
#include <fstream>
#include <iostream>
#include <maolan/audio/clip.h>
#include <maolan/audio/io.h>
#include <maolan/audio/ossin.h>
#include <maolan/audio/ossout.h>
#include <maolan/audio/track.h>
#include <maolan/config.h>
#include <maolan/constants.h>
#include <maolan/io.h>
#include <maolan/midi/chunk.h>
#include <maolan/midi/clip.h>
#include <maolan/utils.h>

#define TRACK_HEIGHT 100
#define TRACK_WIDTH 2000
int height = 0;

void tracks(gnui::Widget *tr, int &h)
{
  for (auto item = maolan::IO::begin(); item != nullptr; item = item->next())
  {
    if (item->type() == "Track")
    {
      tr = new gnui::InvisibleBox(gnui::UP_BOX, 20, 100 + h, TRACK_WIDTH,
                                  TRACK_HEIGHT,"");
     h += 120;
    }
  }
}
void play(gnui::Widget *, void *)
{

  while (1)
  {
    for (auto item = maolan::IO::begin(); item != nullptr; item = item->next())
    {
      item->setup();
    }
    for (auto item = maolan::IO::begin(); item != nullptr; item = item->next())
    {
      item->fetch();
    }
    for (auto item = maolan::IO::begin(); item != nullptr; item = item->next())
    {
      item->process();
    }
    auto playhead = maolan::IO::playHead();
  }
}


gnui::Button *button = nullptr;
gnui::Widget *tr = nullptr;
int main(int argc, char **argv)
{
  maolan::audio::OSSOut out("/dev/dsp", 2);
  maolan::audio::Track track("name");
  maolan::audio::Clip clip(0, 30000, 0, "/usr/src/libmaolan/data/session.wav");
  clip.parrent(&track);
  out.connect(&track);
  gnui::Window *window = new gnui::Window(800, 430);
  window->begin();
  {
    tracks(tr, height);
    button = new gnui::Button(10, 10, 20, 20, "@>");
    button->callback(play);
  }
  window->show(argc, argv);
  return gnui::run();
}
