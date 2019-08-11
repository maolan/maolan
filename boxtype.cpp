//
// "$Id$"
//
// Boxtype test program for the Fast Light Tool Kit (FLTK).
//
// Copyright 1998-1999 by Bill Spitzak and others.
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
// Please report all bugs and problems to "fltk-bugs@easysw.com".
//

#include <stdlib.h>
#include <stdio.h>
#include <gnui/run.h>
#include <gnui/Window.h>
#include <gnui/InvisibleBox.h>

int N = 0;
#define W 150
#define H 50
#define ROWS 6

gnui::Widget* bt(const char *name, gnui::Box* type, int square=0) {
    int x = N%4;
    int y = N/4;
    N++;
    x = x*W+10;
    y = y*H+10;
    gnui::Widget *b = new gnui::InvisibleBox(type,x,y,square ? H-20 : W-20,H-20,name);
    b->labelsize(11);
    if (square) {
	b->clear_flag(gnui::ALIGN_MASK);
	b->set_flag(gnui::ALIGN_RIGHT);
    }
    return b;
}

int main(int argc, char ** argv) {
    gnui::Window window(4*W,ROWS*H);
    window.color(12);// light blue
    window.begin();
    bt("gnui::NO_BOX",gnui::NO_BOX);
    bt("gnui::FLAT_BOX",gnui::FLAT_BOX);
    //  N += 2; // go to start of next row to line up boxes & frames
    bt("gnui::UP_BOX",gnui::UP_BOX);
    bt("gnui::DOWN_BOX",gnui::DOWN_BOX);
    //  bt("gnui::UP_FRAME",gnui::UP_FRAME);
    //  bt("gnui::DOWN_FRAME",gnui::DOWN_FRAME);
    bt("gnui::THIN_UP_BOX",gnui::THIN_UP_BOX);
    bt("gnui::THIN_DOWN_BOX",gnui::THIN_DOWN_BOX);
    //  bt("gnui::THIN_UP_FRAME",gnui::THIN_UP_FRAME);
    //  bt("gnui::THIN_DOWN_FRAME",gnui::THIN_DOWN_FRAME);
    bt("gnui::ENGRAVED_BOX",gnui::ENGRAVED_BOX);
    bt("gnui::EMBOSSED_BOX",gnui::EMBOSSED_BOX);
    //  bt("gnui::ENGRAVED_FRAME",gnui::ENGRAVED_FRAME);
    //  bt("gnui::EMBOSSED_FRAME",gnui::EMBOSSED_FRAME);
    bt("gnui::ROUND_UP_BOX",gnui::ROUND_UP_BOX);
    bt("gnui::ROUND_DOWN_BOX",gnui::ROUND_DOWN_BOX);
    bt("gnui::DIAMOND_UP_BOX",gnui::DIAMOND_UP_BOX);
    bt("gnui::DIAMOND_DOWN_BOX",gnui::DIAMOND_DOWN_BOX);
    //  bt("gnui::BORDER_FRAME",gnui::BORDER_FRAME);
    //  bt("gnui::SHADOW_FRAME",gnui::SHADOW_FRAME);
    bt("gnui::BORDER_BOX",gnui::BORDER_BOX);
    bt("gnui::ROUNDED_BOX",gnui::ROUNDED_BOX);
    bt("gnui::RSHADOW_BOX",gnui::RSHADOW_BOX);
    //  bt("gnui::ROUNDED_FRAME",gnui::ROUNDED_FRAME);
    bt("gnui::RFLAT_BOX",gnui::RFLAT_BOX);
    bt("gnui::SHADOW_BOX",gnui::SHADOW_BOX);
    bt("gnui::OVAL_BOX",gnui::OVAL_BOX);
    bt("gnui::OSHADOW_BOX",gnui::OSHADOW_BOX);
    //  bt("gnui::OVAL_FRAME",gnui::OVAL_FRAME);
    bt("gnui::OFLAT_BOX",gnui::OFLAT_BOX);
    //    bt("gnui::PLASTIC_UP_BOX", gnui::PLASTIC_UP_BOX);
    //    bt("gnui::PLASTIC_DOWN_BOX", gnui::PLASTIC_DOWN_BOX);
    //    bt("gnui::FOCUS_FRAME", gnui::FOCUS_FRAME);
    bt("gnui::BORDER_FRAME", gnui::BORDER_FRAME);
    bt("gnui::PLASTIC_UP_BOX", gnui::PLASTIC_UP_BOX)->color(12);
    bt("gnui::PLASTIC_DOWN_BOX", gnui::PLASTIC_DOWN_BOX)->color(12);
    window.resizable(window);
    window.end();
    window.show(argc,argv);
    return gnui::run();
}

//
// End of "$Id$".
//
