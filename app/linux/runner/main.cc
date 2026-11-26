#include <gdk/gdk.h>
#include <stdlib.h>

#include "my_application.h"

int main(int argc, char** argv) {
  // Prefer XWayland over native Wayland. Flutter's GTK3 embedder makes typing
  // visibly laggy on native Wayland (Hyprland, Intel, Mesa EGL), while the same
  // build is smooth under X11. Wayland stays as the fallback when there is no
  // X server. An explicit GDK_BACKEND wins; UTRE_WAYLAND=1 forces native Wayland.
  // This must run before GTK initialises.
  const char* force_wayland = getenv("UTRE_WAYLAND");
  if (force_wayland != nullptr && force_wayland[0] == '1') {
    gdk_set_allowed_backends("wayland,x11");
  } else if (getenv("GDK_BACKEND") == nullptr) {
    gdk_set_allowed_backends("x11,wayland");
  }
  // GDK would capitalise the X11 WM_CLASS ("Dev.utre.utre"); keep it equal to
  // the application id so window rules and StartupWMClass match.
  gdk_set_program_class("dev.utre.utre");

  g_autoptr(MyApplication) app = my_application_new();
  return g_application_run(G_APPLICATION(app), argc, argv);
}
