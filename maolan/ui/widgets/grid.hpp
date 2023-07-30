#pragma once

namespace maolan::ui {
class Track;
class Grid {
public:
  Grid(Track *t);

  void draw();

protected:
  Track *_track;
};
} // namespace maolan::ui
