# adl2_interp — reference interpreter (smash3 adl-interp / LANGUAGE.md).
# Event → region membership + cutflow. No ROOT, no --profile in this unit.
adl2_add_library(adl2_interp
  SOURCES
    src/event.cpp
    src/eval.cpp
    src/cutflow.cpp
    src/sample.cpp
  PUBLIC_DEPS adl2_sema
)
