#pragma once

#include <QRect>
#include <QSize>
#include <QUrl>
#include <QString>
#include <QVector>

struct FrontendComputer
{
    QString name;
    QString details;
    bool online = false;
    bool paired = false;
    bool busy = false;
    bool wakeable = false;
    bool statusUnknown = false;
    bool serverSupported = false;
};

struct FrontendApp
{
    QString name;
    QUrl boxArt;
    int appId = 0;
    bool running = false;
    bool hidden = false;
    bool directLaunch = false;
    bool appCollectorGame = false;
};

struct FrontendStreamingPreferences
{
    int width = 0;
    int height = 0;
    int fps = 0;
    int bitrateKbps = 0;
    int packetSize = 0;
    int audioConfig = 0;
    int videoCodecConfig = 0;
    int videoDecoderSelection = 0;
    int windowMode = 0;
    int recommendedFullScreenMode = 0;
    int uiDisplayMode = 0;
    int language = 0;
    int captureSysKeysMode = 0;
    bool unlockBitrate = false;
    bool autoAdjustBitrate = false;
    bool enableVsync = false;
    bool gameOptimizations = false;
    bool playAudioOnHost = false;
    bool multiController = false;
    bool enableMdns = false;
    bool quitAppAfter = false;
    bool absoluteMouseMode = false;
    bool absoluteTouchMode = false;
    bool framePacing = false;
    bool connectionWarnings = false;
    bool configurationWarnings = false;
    bool richPresence = false;
    bool gamepadMouse = false;
    bool detectNetworkBlocking = false;
    bool showPerformanceOverlay = false;
    bool swapMouseButtons = false;
    bool muteOnFocusLoss = false;
    bool backgroundGamepad = false;
    bool reverseScrollDirection = false;
    bool swapFaceButtons = false;
    bool keepAwake = false;
    bool enableHdr = false;
    bool enableYUV444 = false;
};

struct FrontendDisplayInfo
{
    QRect nativeResolution;
    QRect safeAreaResolution;
    int refreshRate = 0;
};

struct FrontendSystemProperties
{
    bool isRunningWayland = false;
    bool isRunningXWayland = false;
    bool isWow64 = false;
    bool hasDesktopEnvironment = false;
    bool hasBrowser = false;
    bool hasDiscordIntegration = false;
    bool usesMaterial3Theme = false;
    bool hasHardwareAcceleration = true;
    bool rendererAlwaysFullScreen = false;
    bool supportsHdr = true;
    QString friendlyNativeArchName;
    QString versionString;
    QString unmappedGamepads;
    QSize maximumResolution;
    QVector<FrontendDisplayInfo> displays;
};
