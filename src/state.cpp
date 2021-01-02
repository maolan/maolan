#include "maolan/state.hpp"


using namespace maolan;


State * State::state = nullptr;


State::State()
  : dummyvar{42}
{}


State::~State()
{}


State * State::get()
{
  if (state) { return state; }
  state = new State();
  return state;
}


void State::init()
{}
