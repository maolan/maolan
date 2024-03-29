#include <maolan/ui/state.hpp>

using namespace maolan::ui;

State *State::state = nullptr;

State::State() : zoom{1 << 10} {}

State::~State() {}

State *State::get() {
  if (state) {
    return state;
  }
  state = new State();
  return state;
}
