#include "preferencesfacade.h"

PreferencesFacade::PreferencesFacade(StreamingPreferences* preferences, QObject* parent)
    : QObject(parent),
      m_Preferences(preferences ? preferences : StreamingPreferences::get())
{
}

FrontendStreamingPreferences PreferencesFacade::preferences() const
{
    FrontendStreamingPreferences snapshot;

    snapshot.width = m_Preferences->width;
    snapshot.height = m_Preferences->height;
    snapshot.fps = m_Preferences->fps;
    snapshot.bitrateKbps = m_Preferences->bitrateKbps;
    snapshot.packetSize = m_Preferences->packetSize;
    snapshot.audioConfig = static_cast<int>(m_Preferences->audioConfig);
    snapshot.videoCodecConfig = static_cast<int>(m_Preferences->videoCodecConfig);
    snapshot.videoDecoderSelection = static_cast<int>(m_Preferences->videoDecoderSelection);
    snapshot.windowMode = static_cast<int>(m_Preferences->windowMode);
    snapshot.recommendedFullScreenMode = static_cast<int>(m_Preferences->recommendedFullScreenMode);
    snapshot.uiDisplayMode = static_cast<int>(m_Preferences->uiDisplayMode);
    snapshot.language = static_cast<int>(m_Preferences->language);
    snapshot.captureSysKeysMode = static_cast<int>(m_Preferences->captureSysKeysMode);
    snapshot.unlockBitrate = m_Preferences->unlockBitrate;
    snapshot.autoAdjustBitrate = m_Preferences->autoAdjustBitrate;
    snapshot.enableVsync = m_Preferences->enableVsync;
    snapshot.gameOptimizations = m_Preferences->gameOptimizations;
    snapshot.playAudioOnHost = m_Preferences->playAudioOnHost;
    snapshot.multiController = m_Preferences->multiController;
    snapshot.enableMdns = m_Preferences->enableMdns;
    snapshot.quitAppAfter = m_Preferences->quitAppAfter;
    snapshot.absoluteMouseMode = m_Preferences->absoluteMouseMode;
    snapshot.absoluteTouchMode = m_Preferences->absoluteTouchMode;
    snapshot.framePacing = m_Preferences->framePacing;
    snapshot.connectionWarnings = m_Preferences->connectionWarnings;
    snapshot.configurationWarnings = m_Preferences->configurationWarnings;
    snapshot.richPresence = m_Preferences->richPresence;
    snapshot.gamepadMouse = m_Preferences->gamepadMouse;
    snapshot.detectNetworkBlocking = m_Preferences->detectNetworkBlocking;
    snapshot.showPerformanceOverlay = m_Preferences->showPerformanceOverlay;
    snapshot.swapMouseButtons = m_Preferences->swapMouseButtons;
    snapshot.muteOnFocusLoss = m_Preferences->muteOnFocusLoss;
    snapshot.backgroundGamepad = m_Preferences->backgroundGamepad;
    snapshot.reverseScrollDirection = m_Preferences->reverseScrollDirection;
    snapshot.swapFaceButtons = m_Preferences->swapFaceButtons;
    snapshot.keepAwake = m_Preferences->keepAwake;
    snapshot.enableHdr = m_Preferences->enableHdr;
    snapshot.enableYUV444 = m_Preferences->enableYUV444;

    return snapshot;
}

void PreferencesFacade::applyPreferences(const FrontendStreamingPreferences& preferences, bool saveAfterApply)
{
    m_Preferences->width = preferences.width;
    m_Preferences->height = preferences.height;
    m_Preferences->fps = preferences.fps;
    m_Preferences->bitrateKbps = preferences.bitrateKbps;
    m_Preferences->packetSize = preferences.packetSize;
    m_Preferences->audioConfig = static_cast<StreamingPreferences::AudioConfig>(preferences.audioConfig);
    m_Preferences->videoCodecConfig = static_cast<StreamingPreferences::VideoCodecConfig>(preferences.videoCodecConfig);
    m_Preferences->videoDecoderSelection = static_cast<StreamingPreferences::VideoDecoderSelection>(preferences.videoDecoderSelection);
    m_Preferences->windowMode = static_cast<StreamingPreferences::WindowMode>(preferences.windowMode);
    m_Preferences->uiDisplayMode = static_cast<StreamingPreferences::UIDisplayMode>(preferences.uiDisplayMode);
    m_Preferences->language = static_cast<StreamingPreferences::Language>(preferences.language);
    m_Preferences->captureSysKeysMode = static_cast<StreamingPreferences::CaptureSysKeysMode>(preferences.captureSysKeysMode);
    m_Preferences->unlockBitrate = preferences.unlockBitrate;
    m_Preferences->autoAdjustBitrate = preferences.autoAdjustBitrate;
    m_Preferences->enableVsync = preferences.enableVsync;
    m_Preferences->gameOptimizations = preferences.gameOptimizations;
    m_Preferences->playAudioOnHost = preferences.playAudioOnHost;
    m_Preferences->multiController = preferences.multiController;
    m_Preferences->enableMdns = preferences.enableMdns;
    m_Preferences->quitAppAfter = preferences.quitAppAfter;
    m_Preferences->absoluteMouseMode = preferences.absoluteMouseMode;
    m_Preferences->absoluteTouchMode = preferences.absoluteTouchMode;
    m_Preferences->framePacing = preferences.framePacing;
    m_Preferences->connectionWarnings = preferences.connectionWarnings;
    m_Preferences->configurationWarnings = preferences.configurationWarnings;
    m_Preferences->richPresence = preferences.richPresence;
    m_Preferences->gamepadMouse = preferences.gamepadMouse;
    m_Preferences->detectNetworkBlocking = preferences.detectNetworkBlocking;
    m_Preferences->showPerformanceOverlay = preferences.showPerformanceOverlay;
    m_Preferences->swapMouseButtons = preferences.swapMouseButtons;
    m_Preferences->muteOnFocusLoss = preferences.muteOnFocusLoss;
    m_Preferences->backgroundGamepad = preferences.backgroundGamepad;
    m_Preferences->reverseScrollDirection = preferences.reverseScrollDirection;
    m_Preferences->swapFaceButtons = preferences.swapFaceButtons;
    m_Preferences->keepAwake = preferences.keepAwake;
    m_Preferences->enableHdr = preferences.enableHdr;
    m_Preferences->enableYUV444 = preferences.enableYUV444;

    if (saveAfterApply) {
        m_Preferences->save();
    }

    emit preferencesChanged();
}

void PreferencesFacade::reload()
{
    m_Preferences->reload();
    emit preferencesChanged();
}

void PreferencesFacade::save()
{
    m_Preferences->save();
}

int PreferencesFacade::getDefaultBitrate(int width, int height, int fps, bool yuv444) const
{
    return StreamingPreferences::getDefaultBitrate(width, height, fps, yuv444);
}

bool PreferencesFacade::retranslate()
{
    bool result = m_Preferences->retranslate();
    emit preferencesChanged();
    return result;
}
