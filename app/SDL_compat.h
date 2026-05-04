//
// SDL3 compatibility header.
// Include this instead of SDL3/SDL.h directly.
//

#pragma once

// Don't let SDL hook our main function
#define SDL_MAIN_HANDLED

#include <SDL3/SDL.h>

// SDL3 moved EGL and Vulkan into the main header namespace,
// but ship separate headers for backwards compat with SDL2.
// These are still available as standalone includes.
#include <SDL3/SDL_vulkan.h>
#include <SDL3/SDL_gpu.h>

// --- SDL3_ttf forward declarations ---
// Forward-declare needed types/functions for SDL3_ttf.
struct TTF_Font;
typedef struct TTF_Font TTF_Font;

extern "C" {
    int TTF_Init(void);
    void TTF_Quit(void);
    TTF_Font* TTF_OpenFontIO(SDL_IOStream* src, int freesrc, int ptsize);
    void TTF_CloseFont(TTF_Font* font);
    SDL_Surface* TTF_RenderText_Blended_Wrapped(TTF_Font* font, const char* text, SDL_Color fg, Uint32 wrapLength);
    // TTF_GetError was removed in SDL3_ttf; use SDL_GetError() instead
}

// --- SDL2 type compat for migration ---

// SDL_version struct removed in SDL3
struct SDL_version { int major, minor, patch; };
#define SDL_VERSION(v) do { \
    (v)->major = SDL_MAJOR_VERSION; \
    (v)->minor = SDL_MINOR_VERSION; \
    (v)->patch = SDL_MICRO_VERSION; \
} while(0)
#define SDL_VERSIONNUM(major, minor, patch) (((major) * 1000000) + ((minor) * 10000) + (patch))
#define SDL_COMPILEDVERSION SDL_VERSIONNUM(SDL_MAJOR_VERSION, SDL_MINOR_VERSION, SDL_MICRO_VERSION)
#define SDL_VERSION_ATLEAST(X, Y, Z) (SDL_COMPILEDVERSION >= SDL_VERSIONNUM(X, Y, Z))

// SDL_SetMainReady removed in SDL3
#define SDL_SetMainReady()

// SDL_INIT_TIMER removed in SDL3; timer is always available
#define SDL_INIT_TIMER 0

// SDL_JoystickPowerLevel renamed to SDL_PowerState
typedef SDL_PowerState SDL_JoystickPowerLevel;

// SDL_JOYSTICK_POWER_* removed; map to SDL_POWERSTATE_*
#define SDL_JOYSTICK_POWER_UNKNOWN SDL_POWERSTATE_UNKNOWN
#define SDL_JOYSTICK_POWER_EMPTY   SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_LOW     SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_MEDIUM  SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_FULL    SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_WIRED   SDL_POWERSTATE_NO_BATTERY
#define SDL_JOYSTICK_POWER_MAX     SDL_POWERSTATE_CHARGED

// --- Deprecated hints removed from SDL3 ---
#ifndef SDL_HINT_GAMECONTROLLER_USE_BUTTON_LABELS
#define SDL_HINT_GAMECONTROLLER_USE_BUTTON_LABELS "SDL_GAMECONTROLLER_USE_BUTTON_LABELS"
#endif
#ifndef SDL_HINT_GRAB_KEYBOARD
#define SDL_HINT_GRAB_KEYBOARD "SDL_GRAB_KEYBOARD"
#endif
#ifndef SDL_HINT_VIDEO_WAYLAND_EMULATE_MOUSE_WARP
#define SDL_HINT_VIDEO_WAYLAND_EMULATE_MOUSE_WARP "SDL_VIDEO_WAYLAND_EMULATE_MOUSE_WARP"
#endif
#ifndef SDL_HINT_VIDEO_X11_FORCE_EGL
#define SDL_HINT_VIDEO_X11_FORCE_EGL "SDL_VIDEO_X11_FORCE_EGL"
#endif
#ifndef SDL_HINT_MOUSE_RELATIVE_SCALING
#define SDL_HINT_MOUSE_RELATIVE_SCALING "SDL_MOUSE_RELATIVE_SCALING"
#endif
#ifndef SDL_HINT_AUDIO_DEVICE_APP_NAME
#define SDL_HINT_AUDIO_DEVICE_APP_NAME "SDL_AUDIO_DEVICE_APP_NAME"
#endif
#ifndef SDL_HINT_WINDOWS_DISABLE_THREAD_NAMING
#define SDL_HINT_WINDOWS_DISABLE_THREAD_NAMING "SDL_WINDOWS_DISABLE_THREAD_NAMING"
#endif
