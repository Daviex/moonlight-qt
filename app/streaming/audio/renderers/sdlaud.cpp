#include "sdl.h"

#include <Limelight.h>

SdlAudioRenderer::SdlAudioRenderer()
    : m_AudioStream(nullptr),
      m_AudioBuffer(nullptr)
{
    SDL_assert(!SDL_WasInit(SDL_INIT_AUDIO));

    if (!SDL_InitSubSystem(SDL_INIT_AUDIO)) {
        SDL_LogError(SDL_LOG_CATEGORY_APPLICATION,
                     "SDL_InitSubSystem(SDL_INIT_AUDIO) failed: %s",
                     SDL_GetError());
        SDL_assert(SDL_WasInit(SDL_INIT_AUDIO));
    }
}

bool SdlAudioRenderer::prepareForPlayback(const OPUS_MULTISTREAM_CONFIGURATION* opusConfig)
{
    SDL_AudioSpec spec;

    SDL_zero(spec);
    spec.format = SDL_AUDIO_F32;
    spec.channels = opusConfig->channelCount;
    spec.freq = opusConfig->sampleRate;

    m_FrameSize = opusConfig->samplesPerFrame *
                  opusConfig->channelCount *
                  getAudioBufferSampleSize();

    m_AudioStream = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, nullptr, nullptr);
    if (!m_AudioStream) {
        SDL_LogError(SDL_LOG_CATEGORY_APPLICATION,
                     "Failed to open audio device: %s",
                     SDL_GetError());
        return false;
    }

    m_AudioBuffer = SDL_malloc(m_FrameSize);
    if (!m_AudioBuffer) {
        SDL_LogError(SDL_LOG_CATEGORY_APPLICATION,
                     "Failed to allocate audio buffer");
        return false;
    }

    SDL_LogInfo(SDL_LOG_CATEGORY_APPLICATION,
                "Desired audio buffer: %u samples (%u bytes)",
                opusConfig->samplesPerFrame * 3,
                opusConfig->samplesPerFrame * 3 * opusConfig->channelCount * getAudioBufferSampleSize());

    SDL_LogInfo(SDL_LOG_CATEGORY_APPLICATION,
                "Obtained audio buffer: %d samples (%d bytes)",
                opusConfig->samplesPerFrame,
                m_FrameSize);

    SDL_LogInfo(SDL_LOG_CATEGORY_APPLICATION,
                "SDL audio driver: %s",
                SDL_GetCurrentAudioDriver());

    SDL_ResumeAudioStreamDevice(m_AudioStream);

    return true;
}

SdlAudioRenderer::~SdlAudioRenderer()
{
    if (m_AudioStream) {
        SDL_DestroyAudioStream(m_AudioStream);
    }

    if (m_AudioBuffer) {
        SDL_free(m_AudioBuffer);
    }

    SDL_QuitSubSystem(SDL_INIT_AUDIO);
    SDL_assert(!SDL_WasInit(SDL_INIT_AUDIO));
}

void* SdlAudioRenderer::getAudioBuffer(int*)
{
    return m_AudioBuffer;
}

bool SdlAudioRenderer::submitAudio(int bytesWritten)
{
    if (bytesWritten == 0) {
        return true;
    }

    if (LiGetPendingAudioDuration() > 30) {
        return true;
    }

    for (int i = 0; i < 100; i++) {
        if (SDL_AudioStreamDevicePaused(m_AudioStream)) {
            return false;
        }

        if (SDL_GetAudioStreamQueued(m_AudioStream) / m_FrameSize <= 10) {
            break;
        }

        SDL_Delay(1);
    }

    if (!SDL_PutAudioStreamData(m_AudioStream, m_AudioBuffer, bytesWritten)) {
        SDL_LogError(SDL_LOG_CATEGORY_APPLICATION,
                     "Failed to queue audio sample: %s",
                     SDL_GetError());
    }

    return true;
}

IAudioRenderer::AudioFormat SdlAudioRenderer::getAudioBufferFormat()
{
    return AudioFormat::Float32NE;
}
