#include "maolan/ui/state.hpp"


using namespace maolan;


State * State::state = nullptr;


State::State()
{}


State::~State()
{}


State * State::get()
{
  if (state) { return state; }
  state = new State();
  return state;
}
