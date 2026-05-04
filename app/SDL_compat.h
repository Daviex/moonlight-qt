//
// SDL3 compatibility header.
// Include this instead of SDL3/SDL.h directly.
//

#pragma once

// Don't let SDL hook our main function
#define SDL_MAIN_HANDLED

#include <SDL3/SDL.h>

#if defined(__has_include) && __has_include(<SDL3_ttf/SDL_ttf.h>)
#include <SDL3_ttf/SDL_ttf.h>
#else
struct TTF_Font;
typedef struct TTF_Font TTF_Font;

#ifdef __cplusplus
extern "C" {
#endif
    bool TTF_Init(void);
    void TTF_Quit(void);
    TTF_Font* TTF_OpenFontIO(SDL_IOStream* src, bool closeio, float ptsize);
    void TTF_CloseFont(TTF_Font* font);
    SDL_Surface* TTF_RenderText_Blended_Wrapped(TTF_Font* font, const char* text, size_t length, SDL_Color fg, int wrapLength);
#ifdef __cplusplus
}
#endif
#endif

// SDL3 moved EGL and Vulkan into the main header namespace,
// but ship separate headers for backwards compat with SDL2.
// These are still available as standalone includes.
#include <SDL3/SDL_vulkan.h>
#include <SDL3/SDL_gpu.h>

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

// SDL_syswm.h was removed in SDL3. These local constants keep existing
// renderer subsystem checks readable while using SDL3 window properties.
#define SDL_SYSWM_UNKNOWN 0
#define SDL_SYSWM_X11 1
#define SDL_SYSWM_WAYLAND 2
#define SDL_SYSWM_KMSDRM 3

// SDL_JOYSTICK_POWER_* removed; map to SDL_POWERSTATE_*
#define SDL_JOYSTICK_POWER_UNKNOWN SDL_POWERSTATE_UNKNOWN
#define SDL_JOYSTICK_POWER_EMPTY   SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_LOW     SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_MEDIUM  SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_FULL    SDL_POWERSTATE_ON_BATTERY
#define SDL_JOYSTICK_POWER_WIRED   SDL_POWERSTATE_NO_BATTERY
#define SDL_JOYSTICK_POWER_MAX     SDL_POWERSTATE_CHARGED

#ifndef SDL_HINT_GRAB_KEYBOARD
#define SDL_HINT_GRAB_KEYBOARD "SDL_GRAB_KEYBOARD"
#endif
