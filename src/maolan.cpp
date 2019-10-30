#include <gnui/Item.h>
#include <gnui/run.h>
#include <gnui/events.h>
#include <gnui/Tooltip.h>
#include <gnui/Button.h>
#include <gnui/InvisibleBox.h>
#include <gnui/MenuBar.h>
#include <gnui/Slider.h>
#include <gnui/PopupMenu.h>

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

#include <iostream>

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

void switchToSliders(gnui::Widget *, void *v)
{
  gnui::Group *mainGrp= (gnui::Group *)v;
  mainGrp->child(0)->hide();
  mainGrp->child(1)->show();
}

void switchToTracks(gnui::Widget *, void *v)
{
  gnui::Group *mainGrp= (gnui::Group *)v;
  mainGrp->child(1)->hide();
  mainGrp->child(0)->show();
}

void changeSliderLabel(gnui::Widget *, void *v)
{
  gnui::Slider *slider = (gnui::Slider *)v;
  slider->label("All");
  slider->resize(0,0);
}

void quit_callback(gnui::Widget*, void*) { exit(0); }

void tracks(gnui::Widget *tr, int &h)
{
  for (auto item = maolan::IO::begin(); item != nullptr; item = item->next())
  {
    if (item->type() == "Track")
    {
      tr = new gnui::InvisibleBox(gnui::UP_BOX, 255 , 100 + h, TRACK_WIDTH,
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


void createMenuBar(gnui::MenuBar &menuBar)
{
  menuBar.add("",0,0,0,gnui::OUTPUT);
  menuBar.add("&File",0,0,0,gnui::SUBMENU);
  menuBar.add("&File/Open",	gnui::COMMAND+'O', 0, 0);
  menuBar.add("&File/Close",	0, 0);
  menuBar.add("&File/Quit",	gnui::COMMAND+'Q', quit_callback, 0, gnui::MENU_DIVIDER);
  menuBar.add("&Edit",0,0,0,gnui::SUBMENU);
  menuBar.add("Edit/Undo",	gnui::COMMAND+'Z',	0);
  menuBar.add("Edit/Redo",	gnui::COMMAND+'Y',	0, 0, gnui::MENU_DIVIDER);
  menuBar.add("Edit/Cut",	gnui::COMMAND+'X',	0);
  menuBar.add("Edit/Copy",	gnui::COMMAND+'C',	0);
  menuBar.add("Edit/Paste",	gnui::COMMAND+'V',	0);
  menuBar.add("@>",0,play);
  menuBar.add("@circle");
}

gnui::Button *button = nullptr;
gnui::Widget *tr = nullptr;
gnui::MenuBar *menuBar = nullptr;
gnui::Slider *slider = nullptr;

int main(int argc, char **argv)
{
  maolan::audio::Track track("name");
  gnui::Window *window = new gnui::Window(800, 600);
  window->begin();
  {
    gnui::Group *mainGrp = new gnui::Group(0,25,1500,800);
    mainGrp->begin();
    {

    gnui::Group *tracksView= new gnui::Group(0,25,1500,800);
    gnui::Group *sliderView= new gnui::Group(0,25,1500,800);
    sliderView->hide();
    tracksView->begin();
    {
    // tracks(tr, height);
    tr = new gnui::Button(255 , 100, TRACK_WIDTH,
                                TRACK_HEIGHT,"");
    gnui::Group *sliderGrp= new gnui::Group(1,26,250,700);
    sliderGrp->begin();
    {
    gnui::Widget *slider_box = new gnui::InvisibleBox(gnui::BORDER_BOX, 0, 0, 250,
                                  700,"");
    slider = new gnui::Slider(100,100,50,300,"Master");
    slider->set_vertical();
    button = new gnui::Button (0,600,125,50,"IN");
    button = new gnui::Button (125,600,125,50,"OUT");
    button = new gnui::Button (0,650,83,50,"M");
    button = new gnui::Button (83,650,83,50,"R");
    button = new gnui::Button (167,650,83,50,"S");
    }
    sliderGrp->end();
    }
    tracksView->end();
    sliderView->begin();
    {
    gnui::Group *mixete= new gnui::Group(1,26,250,700);
    mixete->begin();
    {
    gnui::Widget *slider_box = new gnui::InvisibleBox(gnui::BORDER_BOX, 0, 0, 250,
                                  700,"");
    gnui::Slider *slider2 = new gnui::Slider(100,100,50,300,"Master");
    slider2->set_vertical();
    button = new gnui::Button (0,600,125,50,"IN");
    button = new gnui::Button (125,600,125,50,"OUT");
    button = new gnui::Button (0,650,83,50,"M");
    button = new gnui::Button (83,650,83,50,"R");
    button = new gnui::Button (167,650,83,50,"S");
    }
    mixete->end();

    }
    }
    mainGrp->end();
    gnui::Group *menuBarGrp = new gnui::Group(0,0,2000,30);
    menuBarGrp->begin();
    {
    menuBar = new gnui::MenuBar(0, 0, 500, 25);
    createMenuBar(*menuBar);
    menuBar->callback(menu_callback);
    menus[0] = menuBar;
    gnui::PopupMenu* view = new gnui::PopupMenu(1265, 0, 100, 25, "View");
    view->begin();
    {
      gnui::Item *tracks = new gnui::Item ("Tracks");
      gnui::Item *sliders = new gnui::Item ("Sliders");
      tracks->callback(switchToTracks,mainGrp);
      sliders->callback(switchToSliders,mainGrp);
    }
    view->end();
    }
    menuBarGrp->end();
    tr->callback(changeSliderLabel,slider);
  }
  window->show(argc, argv);
  return gnui::run();
}
