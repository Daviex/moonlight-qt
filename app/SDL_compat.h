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
// but still ships separate headers.
// These are still available as standalone includes.
#include <SDL3/SDL_vulkan.h>
#include <SDL3/SDL_gpu.h>
