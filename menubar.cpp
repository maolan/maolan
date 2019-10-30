//
// "$Id$"
//
// Menubar test program for the Fast Light Tool Kit (gnui).
//
// Copyright 1998-2006 by Bill Spitzak and others.
//
// This library is free software; you can redistribute it and/or
// modify it under the terms of the GNU Library General Public
// License as published by the Free Software Foundation; either
// version 2 of the License, or (at your option) any later version.
//
// This library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// Library General Public License for more details.
//
// You should have received a copy of the GNU Library General Public
// License along with this library; if not, write to the Free Software
// Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA 02111-1307
// USA.
//
// Please report all bugs and problems on the following page:
//
//    http://www.gnui.org/str.php
//

// Use compat header for GNUI_Menu_Item
#include <gnui/compat/FL/GNUI_Menu_Item.H>

#include <gnui/run.h>
#include <gnui/events.h>
#include <gnui/Output.h>
#include <gnui/Box.h>
#include <gnui/Window.h>
#include <gnui/MenuBar.h>
#include <gnui/ToggleButton.h>
#include <gnui/PopupMenu.h>
#include <gnui/Choice.h>
#include <gnui/Tooltip.h>
#include <gnui/draw.h>

#include <stdio.h>
#include <stdlib.h>
#include <gnui/string.h>

#ifdef __APPLE__
#include <gnui/SystemMenuBar.h>
#endif

gnui::Window *window;

gnui::Menu* menus[4];

void test_cb(gnui::Widget* w, void*)
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
    if (mw->find("button")) mw->replace("button", "Spitzak");
    else mw->replace("Spitzak", "button");
    menus[0]->redraw();
  }

  m->do_callback();
}

void quit_cb(gnui::Widget*, void*) { exit(0); }

GNUI_Menu_Item hugemenu[100];

GNUI_Menu_Item menutable[] = {
  {"foo",0,0,0,GNUI_MENU_INACTIVE},
  {"&File",0,0,0,GNUI_SUBMENU},
    {"&Open",	gnui::COMMAND+'O', 0, 0, GNUI_MENU_INACTIVE},
    {"&Close",	0,	0},
    {"&Quit",	gnui::COMMAND+'Q', quit_cb, 0, gnui::MENU_DIVIDER},
    {"shortcut",'A'},
    {"shortcut",gnui::SHIFT+'A'},
    {"shortcut",gnui::COMMAND+'A'},
    {"shortcut",gnui::COMMAND+gnui::SHIFT+'A'},
    {"shortcut",gnui::ACCELERATOR+'A'},
    {"shortcut",gnui::ACCELERATOR+gnui::SHIFT+'A'},
    {"shortcut",gnui::ACCELERATOR+gnui::COMMAND+'A'},
    {"shortcut",gnui::ACCELERATOR+gnui::SHIFT+gnui::COMMAND+'A', 0,0, gnui::MENU_DIVIDER},
    {"shortcut",gnui::ReturnKey},
    {"shortcut",gnui::COMMAND+gnui::ReturnKey, 0,0, gnui::MENU_DIVIDER},
    {"shortcut",gnui::F1Key},
    {"shortcut",gnui::SHIFT+gnui::F1Key},
    {"shortcut",gnui::COMMAND+gnui::F1Key},
    {"shortcut",gnui::SHIFT+gnui::COMMAND+gnui::F1Key},
    {"shortcut",gnui::ACCELERATOR+gnui::F1Key},
    {"shortcut",gnui::ACCELERATOR+gnui::SHIFT+gnui::F1Key},
    {"shortcut",gnui::ACCELERATOR+gnui::COMMAND+gnui::F1Key},
    {"shortcut",gnui::ACCELERATOR+gnui::SHIFT+gnui::COMMAND+gnui::F1Key, 0,0, gnui::MENU_DIVIDER},
    {"&Submenus", gnui::ACCELERATOR+'S',	0, (void*)"Submenu1", GNUI_SUBMENU},
      {"A very long menu item"},
      {"&submenu",gnui::COMMAND+'S',	0, (void*)"submenu2", GNUI_SUBMENU},
	{"item 1"},
	{"item 2"},
	{"item 3"},
	{"item 4"},
	{0},
      {"after submenu"},
      {0},
    {0},
  {"&Edit",0,0,0,GNUI_SUBMENU},
    {"Undo",	gnui::COMMAND+'Z',	0},
    {"Redo",	gnui::COMMAND+'Y',	0, 0, gnui::MENU_DIVIDER},
    {"Cut",	gnui::COMMAND+'X',	0},
    {"Copy",	gnui::COMMAND+'C',	0},
    {"Paste",	gnui::COMMAND+'V',	0},
    {"Inactive",gnui::COMMAND+'D',	0, 0, GNUI_MENU_INACTIVE},
    {"Clear",	0,	0, 0, gnui::MENU_DIVIDER},
    {"Invisible",gnui::COMMAND+'E',	0, 0, GNUI_MENU_INVISIBLE},
    {"Preferences",0,	0},
    {"Larger", '+', 0, 0},
    {"Smaller", '-', 0, 0},
    {0},
  {"&Checkbox",0,0,0,GNUI_SUBMENU},
    {"&Alpha",	0,	0, (void *)1, gnui::MENU_TOGGLE|GNUI_MENU_VALUE},
    {"&Beta",	0,	0, (void *)2, gnui::MENU_TOGGLE},
    {"&Gamma",	0,	0, (void *)3, gnui::MENU_TOGGLE},
    {"&Delta",	0,	0, (void *)4, gnui::MENU_TOGGLE|GNUI_MENU_VALUE},
    {"&Epsilon",0,	0, (void *)5, gnui::MENU_TOGGLE},
    {"&Pi",	0,	0, (void *)6, gnui::MENU_TOGGLE},
    {"&Mu",	0,	0, (void *)7, gnui::MENU_TOGGLE|gnui::MENU_DIVIDER},
    {"Red",	0,	0, (void *)1, gnui::MENU_TOGGLE},
    {"Black",	0,	0, (void *)1, gnui::MENU_TOGGLE|gnui::MENU_DIVIDER},
    {"00",	0,	0, (void *)1, gnui::MENU_TOGGLE},
    {"000",	0,	0, (void *)1, gnui::MENU_TOGGLE},
    {0},
  {"&Radio",0,0,0,GNUI_SUBMENU},
    {"&Alpha",	0,	0, (void *)1, gnui::MENU_RADIO},
    {"&Beta",	0,	0, (void *)2, gnui::MENU_RADIO},
    {"&Gamma",	0,	0, (void *)3, gnui::MENU_RADIO},
    {"&Delta",	0,	0, (void *)4, gnui::MENU_RADIO|GNUI_MENU_VALUE},
    {"&Epsilon",0,	0, (void *)5, gnui::MENU_RADIO},
    {"&Pi",	0,	0, (void *)6, gnui::MENU_RADIO},
    {"&Mu",	0,	0, (void *)7, gnui::MENU_RADIO|gnui::MENU_DIVIDER},
    {"Red",	0,	0, (void *)1, gnui::MENU_RADIO},
    {"Black",	0,	0, (void *)1, gnui::MENU_RADIO|gnui::MENU_DIVIDER},
    {"00",	0,	0, (void *)1, gnui::MENU_RADIO},
    {"000",	0,	0, (void *)1, gnui::MENU_RADIO},
    {0},
  {"&Font",0,0,0,GNUI_SUBMENU},
    {"Normal",	0, 0},
    {"Bold",	0, 0},
    {"Italic",	0, 0},
    {"BoldItalic",0,0},
    {"Small",	0, 0},
    {"Large",	0, 0},
    {"Emboss",	0, 0},
    {"Engrave",	0, 0},
    {"Shadow",	0, 0},
    {"@->",	0, 0},
    {0},
  {"E&mpty",0,0,0,GNUI_SUBMENU},
    {0},
  {"&Inactive", 0,	0, 0, GNUI_MENU_INACTIVE|GNUI_SUBMENU},
    {"A very long menu item"},
    {"A very long menu item"},
    {0},
  {"Invisible",0,	0, 0, GNUI_MENU_INVISIBLE|GNUI_SUBMENU},
    {"A very long menu item"},
    {"A very long menu item"},
    {0},
  {"&Huge", 0, 0, (void*)hugemenu, GNUI_SUBMENU_POINTER},
  // these buttons demonstrates that the menubar can be used as a "toolbar"
  {"@[]"}, {"@<->"}, {"@+"},
  // it would be nice if checkmarks worked, but they don't:
  //{"toggle",0, 0, 0, gnui::MENU_TOGGLE},
  {0}
};

GNUI_Menu_Item pulldown[] = {
  {"Red",	gnui::ACCELERATOR+'r'},
  {"Green",	gnui::ACCELERATOR+'g'},
  {"Blue",	gnui::ACCELERATOR+'b'},
  {"Strange",	gnui::ACCELERATOR+'s'},
  {"&Charm",	gnui::ACCELERATOR+'c'},
  {"Truth",	gnui::ACCELERATOR+'t'},
  {"Beauty",	gnui::ACCELERATOR+'b'},
  {0}
};

#define WIDTH 600
#define HEIGHT 22 //30 // use 25 for better Windoze look

int main(int argc, char **argv)
{
  for (int i=0; i<99; i++) {
    char buf[100];
    sprintf(buf,"item %d",i);
    hugemenu[i].text = newstring(buf);
  }

  gnui::Window window(WIDTH,400);
  window.color(gnui::WHITE);
  window.tooltip("Press right button\nfor a pop-up menu");
  window.begin();

  gnui::MenuBar menubar(0,0,WIDTH,HEIGHT); menubar.menu(menutable);
  menubar.find("&Font/Normal")->labelfont(gnui::HELVETICA);
  menubar.find("&Font/Bold")->labelfont(gnui::HELVETICA_BOLD);
  menubar.find("&Font/Italic")->labelfont(gnui::HELVETICA_ITALIC);
  menubar.find("&Font/BoldItalic")->labelfont(gnui::HELVETICA_BOLD_ITALIC);
  menubar.find("&Font/Small")->labelsize(10);
  menubar.find("&Font/Large")->labelsize(24);
  menubar.find("&Font/Emboss")->labeltype(gnui::EMBOSSED_LABEL);
  menubar.find("&Font/Engrave")->labeltype(gnui::ENGRAVED_LABEL);
  menubar.find("&Font/Shadow")->labeltype(gnui::SHADOW_LABEL);
  menubar.find("&Font/@->")->labeltype(gnui::SYMBOL_LABEL);
  menubar.find("&Checkbox/Red")->labelcolor(gnui::RED); // label text red
  menubar.find("&Checkbox/Red")->selection_textcolor(gnui::RED); // label text red when selected
  menubar.find("&Checkbox/Red")->textcolor(gnui::RED); // check mark red
  menubar.find("&Checkbox/Black")->labelcolor(gnui::BLACK);
  menubar.find("&Checkbox/Black")->selection_textcolor(gnui::BLACK);
  menubar.find("&Checkbox/Black")->textcolor(gnui::BLACK);
  menubar.find("&Radio/Red")->labelcolor(gnui::RED);
  menubar.find("&Radio/Red")->selection_textcolor(gnui::RED);
  menubar.find("&Radio/Red")->textcolor(gnui::RED);
  menubar.find("&Radio/Black")->labelcolor(gnui::BLACK);
  menubar.find("&Radio/Black")->selection_textcolor(gnui::BLACK);
  menubar.find("&Radio/Black")->textcolor(gnui::BLACK);
  //menubar.find("&Huge/item 69")->deactivate();
  menubar.callback(test_cb);
  menubar.tooltip("This is a menu bar");
  menus[0] = &menubar;

  gnui::PopupMenu mb1(100,100,120,25,"&menubutton"); mb1.menu(pulldown);
  mb1.callback(test_cb);
  mb1.tooltip("This is a menu button");
  menus[1] = &mb1;

  gnui::Choice ch(300,100,90,25,"&choice:"); ch.menu(pulldown);
  ch.callback(test_cb);
  ch.tooltip("This is a choice");
  menus[2] = &ch;

  gnui::PopupMenu mb(0,25,WIDTH,400-HEIGHT/*,"&popup"*/);
  mb.type(gnui::PopupMenu::POPUP3);
  mb.menu(menutable);
  mb.callback(test_cb);
  menus[3] = &mb;

  window.resizable(&mb);
  window.size_range(300,20);
  window.end();
  window.show(argc, argv);

#ifdef __APPLE__
  gnui::SystemMenuBar sysmb(0, 0, 1, 1);
  sysmb.menu(menutable);
  sysmb.find("&Font/Normal")->labelfont(gnui::HELVETICA);
  sysmb.find("&Font/Bold")->labelfont(gnui::HELVETICA_BOLD);
  sysmb.find("&Font/Italic")->labelfont(gnui::HELVETICA_ITALIC);
  sysmb.find("&Font/BoldItalic")->labelfont(gnui::HELVETICA_BOLD_ITALIC);
  sysmb.find("&Font/Small")->labelsize(10);
  sysmb.find("&Font/Large")->labelsize(24);
  sysmb.find("&Font/Emboss")->labeltype(gnui::EMBOSSED_LABEL);
  sysmb.find("&Font/Engrave")->labeltype(gnui::ENGRAVED_LABEL);
  sysmb.find("&Font/Shadow")->labeltype(gnui::SHADOW_LABEL);
  sysmb.find("&Font/@->")->labeltype(gnui::SYMBOL_LABEL);
  sysmb.find("&Checkbox/Red")->labelcolor(gnui::RED); // label text red
  sysmb.find("&Checkbox/Red")->selection_textcolor(gnui::RED); // label text red when selected
  sysmb.find("&Checkbox/Red")->textcolor(gnui::RED); // check mark red
  sysmb.find("&Checkbox/Black")->labelcolor(gnui::BLACK);
  sysmb.find("&Checkbox/Black")->selection_textcolor(gnui::BLACK);
  sysmb.find("&Checkbox/Black")->textcolor(gnui::BLACK);
  sysmb.find("&Radio/Red")->labelcolor(gnui::RED);
  sysmb.find("&Radio/Red")->selection_textcolor(gnui::RED);
  sysmb.find("&Radio/Red")->textcolor(gnui::RED);
  sysmb.find("&Radio/Black")->labelcolor(gnui::BLACK);
  sysmb.find("&Radio/Black")->selection_textcolor(gnui::BLACK);
  sysmb.find("&Radio/Black")->textcolor(gnui::BLACK);
  sysmb.callback(test_cb);
  sysmb.layout();
#endif

  return gnui::run();
}

//
// End of "$Id$".
//
