//#include <gnui/compat/FL/GNUI_Menu_Item.H>
#include <gnui/Item.h>
#include <gnui/run.h>
#include <gnui/events.h>
#include <gnui/Output.h>
#include <gnui/Box.h>
#include <gnui/Window.h>
#include <gnui/ToggleButton.h>
#include <gnui/Menu.h>
#include <gnui/PopupMenu.h>
#include <gnui/Choice.h>
#include <gnui/Tooltip.h>
#include <gnui/draw.h>
#include <gnui/Button.h>
#include <gnui/InvisibleBox.h>
#include <gnui/ProgressBar.h>
#include <gnui/Symbol.h>
#include <gnui/Widget.h>
#include <gnui/MenuBar.h>
#include <gnui/Group.h>
#include <gnui/string.h>

#include <fcntl.h>
#include <fstream>
#include <iostream>
#include <stdio.h>
#include <stdlib.h>

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
gnui::Menu* menus[4];

void menu_callback(gnui::Widget* w, void*)
{
  gnui::Menu* mw = (gnui::Menu*)w;
  gnui::Widget* m = mw->item();
  if (!m)
    printf("NULL\n");
  else if (m->shortcut())
    printf("%s - %s\n", m->label(), gnui::key_name(m->shortcut()));
  else
    printf("%s\n", m->label());

  if (!strcmp("item 77", m->label())) {
    if (mw->find("button")) mw->replace("button");
    else mw->replace("button");
    menus[0]->redraw();
  }

  m->do_callback();
}

void quit_callback(gnui::Widget*, void*) { exit(0); }

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

gnui::Item menutable[] = {
  {"",0,0,0},
  {"&File",0,0,0,gnui::SUBMENU},
    {"&Open",	gnui::COMMAND+'O', 0, 0},
    {"&Close",	0,	0},
    {"&Quit",	gnui::COMMAND+'Q', quit_callback, 0, gnui::MENU_DIVIDER},
    {0},
  {"&Edit",0,0,0},
    {"Undo",	gnui::COMMAND+'Z',	0},
    {"Redo",	gnui::COMMAND+'Y',	0, 0, gnui::MENU_DIVIDER},
    {"Cut",	gnui::COMMAND+'X',	0},
    {"Copy",	gnui::COMMAND+'C',	0},
    {"Paste",	gnui::COMMAND+'V',	0},
    {0},
    {"@>",0,play},
    {"@circle"},
    {0},
    {0}
};

void createMenuBar(gnui::MenuBar &menuBar)
{
  menuBar.add("",0,0,0);
  menuBar.add("&File",0,0,0,gnui::SUBMENU);
  menuBar.add("&Open",	gnui::COMMAND+'O', 0, 0);
  menuBar.add("&Close",	0,	0);
  menuBar.add("&Quit",	gnui::COMMAND+'Q', quit_callback, 0, gnui::MENU_DIVIDER);
  menuBar.add("&Edit",0,0,0);
  menuBar.add("Undo",	gnui::COMMAND+'Z',	0);
  menuBar.add("Redo",	gnui::COMMAND+'Y',	0, 0, gnui::MENU_DIVIDER);
  menuBar.add("Cut",	gnui::COMMAND+'X',	0);
  menuBar.add("Copy",	gnui::COMMAND+'C',	0);
  menuBar.add("Paste",	gnui::COMMAND+'V',	0);
  menuBar.add("@>",0,play);
  menuBar.add("@circle");
}

gnui::Button *button = nullptr;
gnui::Widget *tr = nullptr;
gnui::MenuBar *menuBar = nullptr;

int main(int argc, char **argv)
{
  gnui::Window *window = new gnui::Window(800, 600);
  window->begin();
  {
    // tracks(tr, height);
    gnui::Group *menuBarGrp = new gnui::Group(0,0,1000,30);
    menuBarGrp->begin();
    {
    menuBar = new gnui::MenuBar(0, 0, 500, 25);
    createMenuBar(*menuBar);
    menuBar->callback(menu_callback);
    menus[0] = menuBar;
    }
    menuBarGrp->end();
    gnui::Group *sliderGrp= new gnui::Group(1,26,700,250);
    sliderGrp->begin();
    {
    gnui::Widget *slider_box = new gnui::InvisibleBox(gnui::BORDER_BOX, 0, 0, 250,
                                  700,"");
    }
    sliderGrp->end();
  }
  window->show(argc, argv);
  return gnui::run();
}
