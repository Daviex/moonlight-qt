#include "streamingpreferences.h"
#include "utils.h"

#include <QSettings>
#include <QThreadPool>
#include <QTranslator>
#include <QCoreApplication>
#include <QLocale>
#include <QReadWriteLock>
#include <QtMath>

#include <QtDebug>

#define SER_STREAMSETTINGS "streamsettings"
#define SER_WIDTH "width"
#define SER_HEIGHT "height"
#define SER_FPS "fps"
#define SER_BITRATE "bitrate"
#define SER_UNLOCK_BITRATE "unlockbitrate"
#define SER_AUTOADJUSTBITRATE "autoadjustbitrate"
#define SER_FULLSCREEN "fullscreen"
#define SER_VSYNC "vsync"
#define SER_GAMEOPTS "gameopts"
#define SER_HOSTAUDIO "hostaudio"
#define SER_MULTICONT "multicontroller"
#define SER_AUDIOCFG "audiocfg"
#define SER_VIDEOCFG "videocfg"
#define SER_HDR "hdr"
#define SER_YUV444 "yuv444"
#define SER_VIDEODEC "videodec"
#define SER_WINDOWMODE "windowmode"
#define SER_MDNS "mdns"
#define SER_QUITAPPAFTER "quitAppAfter"
#define SER_ABSMOUSEMODE "mouseacceleration"
#define SER_ABSTOUCHMODE "abstouchmode"
#define SER_STARTWINDOWED "startwindowed"
#define SER_FRAMEPACING "framepacing"
#define SER_CONNWARNINGS "connwarnings"
#define SER_CONFWARNINGS "confwarnings"
#define SER_UIDISPLAYMODE "uidisplaymode"
#define SER_RICHPRESENCE "richpresence"
#define SER_GAMEPADMOUSE "gamepadmouse"
#define SER_DEFAULTVER "defaultver"
#define SER_PACKETSIZE "packetsize"
#define SER_DETECTNETBLOCKING "detectnetblocking"
#define SER_SHOWPERFOVERLAY "showperfoverlay"
#define SER_SWAPMOUSEBUTTONS "swapmousebuttons"
#define SER_MUTEONFOCUSLOSS "muteonfocusloss"
#define SER_BACKGROUNDGAMEPAD "backgroundgamepad"
#define SER_REVERSESCROLL "reversescroll"
#define SER_SWAPFACEBUTTONS "swapfacebuttons"
#define SER_CAPTURESYSKEYS "capturesyskeys"
#define SER_KEEPAWAKE "keepawake"
#define SER_LANGUAGE "language"

#define CURRENT_DEFAULT_VER 2

static StreamingPreferences* s_GlobalPrefs;

Q_GLOBAL_STATIC(QReadWriteLock, s_GlobalPrefsLock)

namespace {

struct StreamingPreferencesSnapshot
{
    int width;
    int height;
    int fps;
    int bitrateKbps;
    bool unlockBitrate;
    bool autoAdjustBitrate;
    bool enableVsync;
    bool gameOptimizations;
    bool playAudioOnHost;
    bool multiController;
    bool enableMdns;
    bool quitAppAfter;
    bool absoluteMouseMode;
    bool absoluteTouchMode;
    bool framePacing;
    bool connectionWarnings;
    bool configurationWarnings;
    bool richPresence;
    bool gamepadMouse;
    bool detectNetworkBlocking;
    bool showPerformanceOverlay;
    StreamingPreferences::AudioConfig audioConfig;
    bool enableHdr;
    bool enableYUV444;
    StreamingPreferences::VideoCodecConfig videoCodecConfig;
    StreamingPreferences::VideoDecoderSelection videoDecoderSelection;
    StreamingPreferences::WindowMode windowMode;
    StreamingPreferences::UIDisplayMode uiDisplayMode;
    StreamingPreferences::Language language;
    bool swapMouseButtons;
    bool muteOnFocusLoss;
    bool backgroundGamepad;
    bool reverseScrollDirection;
    bool swapFaceButtons;
    StreamingPreferences::CaptureSysKeysMode captureSysKeysMode;
    bool keepAwake;
    int packetSize;
};

StreamingPreferencesSnapshot makeSnapshot(const StreamingPreferences* prefs)
{
    return StreamingPreferencesSnapshot {
        prefs->width,
        prefs->height,
        prefs->fps,
        prefs->bitrateKbps,
        prefs->unlockBitrate,
        prefs->autoAdjustBitrate,
        prefs->enableVsync,
        prefs->gameOptimizations,
        prefs->playAudioOnHost,
        prefs->multiController,
        prefs->enableMdns,
        prefs->quitAppAfter,
        prefs->absoluteMouseMode,
        prefs->absoluteTouchMode,
        prefs->framePacing,
        prefs->connectionWarnings,
        prefs->configurationWarnings,
        prefs->richPresence,
        prefs->gamepadMouse,
        prefs->detectNetworkBlocking,
        prefs->showPerformanceOverlay,
        prefs->audioConfig,
        prefs->enableHdr,
        prefs->enableYUV444,
        prefs->videoCodecConfig,
        prefs->videoDecoderSelection,
        prefs->windowMode,
        prefs->uiDisplayMode,
        prefs->language,
        prefs->swapMouseButtons,
        prefs->muteOnFocusLoss,
        prefs->backgroundGamepad,
        prefs->reverseScrollDirection,
        prefs->swapFaceButtons,
        prefs->captureSysKeysMode,
        prefs->keepAwake,
        prefs->packetSize,
    };
}

void saveSnapshot(const StreamingPreferencesSnapshot& snapshot)
{
    QSettings settings;

    settings.setValue(SER_WIDTH, snapshot.width);
    settings.setValue(SER_HEIGHT, snapshot.height);
    settings.setValue(SER_FPS, snapshot.fps);
    settings.setValue(SER_BITRATE, snapshot.bitrateKbps);
    settings.setValue(SER_UNLOCK_BITRATE, snapshot.unlockBitrate);
    settings.setValue(SER_AUTOADJUSTBITRATE, snapshot.autoAdjustBitrate);
    settings.setValue(SER_VSYNC, snapshot.enableVsync);
    settings.setValue(SER_GAMEOPTS, snapshot.gameOptimizations);
    settings.setValue(SER_HOSTAUDIO, snapshot.playAudioOnHost);
    settings.setValue(SER_MULTICONT, snapshot.multiController);
    settings.setValue(SER_MDNS, snapshot.enableMdns);
    settings.setValue(SER_QUITAPPAFTER, snapshot.quitAppAfter);
    settings.setValue(SER_ABSMOUSEMODE, snapshot.absoluteMouseMode);
    settings.setValue(SER_ABSTOUCHMODE, snapshot.absoluteTouchMode);
    settings.setValue(SER_FRAMEPACING, snapshot.framePacing);
    settings.setValue(SER_CONNWARNINGS, snapshot.connectionWarnings);
    settings.setValue(SER_CONFWARNINGS, snapshot.configurationWarnings);
    settings.setValue(SER_RICHPRESENCE, snapshot.richPresence);
    settings.setValue(SER_GAMEPADMOUSE, snapshot.gamepadMouse);
    settings.setValue(SER_PACKETSIZE, snapshot.packetSize);
    settings.setValue(SER_DETECTNETBLOCKING, snapshot.detectNetworkBlocking);
    settings.setValue(SER_SHOWPERFOVERLAY, snapshot.showPerformanceOverlay);
    settings.setValue(SER_AUDIOCFG, static_cast<int>(snapshot.audioConfig));
    settings.setValue(SER_HDR, snapshot.enableHdr);
    settings.setValue(SER_YUV444, snapshot.enableYUV444);
    settings.setValue(SER_VIDEOCFG, static_cast<int>(snapshot.videoCodecConfig));
    settings.setValue(SER_VIDEODEC, static_cast<int>(snapshot.videoDecoderSelection));
    settings.setValue(SER_WINDOWMODE, static_cast<int>(snapshot.windowMode));
    settings.setValue(SER_UIDISPLAYMODE, static_cast<int>(snapshot.uiDisplayMode));
    settings.setValue(SER_LANGUAGE, static_cast<int>(snapshot.language));
    settings.setValue(SER_DEFAULTVER, CURRENT_DEFAULT_VER);
    settings.setValue(SER_SWAPMOUSEBUTTONS, snapshot.swapMouseButtons);
    settings.setValue(SER_MUTEONFOCUSLOSS, snapshot.muteOnFocusLoss);
    settings.setValue(SER_BACKGROUNDGAMEPAD, snapshot.backgroundGamepad);
    settings.setValue(SER_REVERSESCROLL, snapshot.reverseScrollDirection);
    settings.setValue(SER_SWAPFACEBUTTONS, snapshot.swapFaceButtons);
    settings.setValue(SER_CAPTURESYSKEYS, snapshot.captureSysKeysMode);
    settings.setValue(SER_KEEPAWAKE, snapshot.keepAwake);
    settings.sync();

    if (settings.status() != QSettings::NoError) {
        qWarning() << "Failed to save streaming preferences:" << settings.status();
    }
}

class DeferredStreamingPreferencesSaveTask : public QRunnable
{
public:
    DeferredStreamingPreferencesSaveTask(QMutex* saveMutex,
                                         quint64* saveGeneration,
                                         const StreamingPreferencesSnapshot& snapshot,
                                         quint64 generation)
        : m_SaveMutex(saveMutex)
        , m_SaveGeneration(saveGeneration)
        , m_Snapshot(snapshot)
        , m_Generation(generation)
    {
    }

    void run() override
    {
        QMutexLocker locker(m_SaveMutex);
        if (m_Generation != *m_SaveGeneration) {
            return;
        }

        saveSnapshot(m_Snapshot);
    }

private:
    QMutex* m_SaveMutex;
    quint64* m_SaveGeneration;
    StreamingPreferencesSnapshot m_Snapshot;
    quint64 m_Generation;
};

} // namespace

StreamingPreferences::StreamingPreferences(QQmlEngine *qmlEngine)
    : m_QmlEngine(qmlEngine)
{
    reload();
}

StreamingPreferences* StreamingPreferences::get(QQmlEngine *qmlEngine)
{
    {
        QReadLocker readGuard(s_GlobalPrefsLock);

        // If we have a preference object and it's associated with a QML engine or
        // if the caller didn't specify a QML engine, return the existing object.
        if (s_GlobalPrefs && (s_GlobalPrefs->m_QmlEngine || !qmlEngine)) {
            // The lifetime logic here relies on the QML engine also being a singleton.
            Q_ASSERT(!qmlEngine || s_GlobalPrefs->m_QmlEngine == qmlEngine);
            return s_GlobalPrefs;
        }
    }

    {
        QWriteLocker writeGuard(s_GlobalPrefsLock);

        // If we already have an preference object but the QML engine is now available,
        // associate the QML engine with the preferences.
        if (s_GlobalPrefs) {
            if (!s_GlobalPrefs->m_QmlEngine) {
                s_GlobalPrefs->m_QmlEngine = qmlEngine;
            }
            else {
                // We could reach this codepath if another thread raced with us
                // and created the object while we were outside the pref lock.
                Q_ASSERT(!qmlEngine || s_GlobalPrefs->m_QmlEngine == qmlEngine);
            }
        }
        else {
            s_GlobalPrefs = new StreamingPreferences(qmlEngine);
        }

        return s_GlobalPrefs;
    }
}

void StreamingPreferences::reload()
{
    QSettings settings;

    int defaultVer = settings.value(SER_DEFAULTVER, 0).toInt();

#ifdef Q_OS_DARWIN
    recommendedFullScreenMode = WindowMode::WM_FULLSCREEN_DESKTOP;
#else
    // Wayland doesn't support modesetting, so use fullscreen desktop mode
    // unless we have a slow GPU (which can take advantage of wp_viewporter
    // to reduce GPU load with lower resolution video streams).
    if (WMUtils::isRunningWayland() && !WMUtils::isGpuSlow()) {
        recommendedFullScreenMode = WindowMode::WM_FULLSCREEN_DESKTOP;
    }
    else {
        recommendedFullScreenMode = WindowMode::WM_FULLSCREEN;
    }
#endif

    width = settings.value(SER_WIDTH, 1280).toInt();
    height = settings.value(SER_HEIGHT, 720).toInt();
    fps = settings.value(SER_FPS, 60).toInt();
    enableYUV444 = settings.value(SER_YUV444, false).toBool();
    bitrateKbps = settings.value(SER_BITRATE, getDefaultBitrate(width, height, fps, enableYUV444)).toInt();
    unlockBitrate = settings.value(SER_UNLOCK_BITRATE, false).toBool();
    autoAdjustBitrate = settings.value(SER_AUTOADJUSTBITRATE, true).toBool();
    enableVsync = settings.value(SER_VSYNC, true).toBool();
    gameOptimizations = settings.value(SER_GAMEOPTS, true).toBool();
    playAudioOnHost = settings.value(SER_HOSTAUDIO, false).toBool();
    multiController = settings.value(SER_MULTICONT, true).toBool();
    enableMdns = settings.value(SER_MDNS, true).toBool();
    quitAppAfter = settings.value(SER_QUITAPPAFTER, false).toBool();
    absoluteMouseMode = settings.value(SER_ABSMOUSEMODE, false).toBool();
    absoluteTouchMode = settings.value(SER_ABSTOUCHMODE, true).toBool();
    framePacing = settings.value(SER_FRAMEPACING, false).toBool();
    connectionWarnings = settings.value(SER_CONNWARNINGS, true).toBool();
    configurationWarnings = settings.value(SER_CONFWARNINGS, true).toBool();
    richPresence = settings.value(SER_RICHPRESENCE, true).toBool();
    gamepadMouse = settings.value(SER_GAMEPADMOUSE, true).toBool();
    detectNetworkBlocking = settings.value(SER_DETECTNETBLOCKING, true).toBool();
    showPerformanceOverlay = settings.value(SER_SHOWPERFOVERLAY, false).toBool();
    packetSize = settings.value(SER_PACKETSIZE, 0).toInt();
    swapMouseButtons = settings.value(SER_SWAPMOUSEBUTTONS, false).toBool();
    muteOnFocusLoss = settings.value(SER_MUTEONFOCUSLOSS, false).toBool();
    backgroundGamepad = settings.value(SER_BACKGROUNDGAMEPAD, false).toBool();
    reverseScrollDirection = settings.value(SER_REVERSESCROLL, false).toBool();
    swapFaceButtons = settings.value(SER_SWAPFACEBUTTONS, false).toBool();
    keepAwake = settings.value(SER_KEEPAWAKE, true).toBool();
    enableHdr = settings.value(SER_HDR, false).toBool();
    captureSysKeysMode = static_cast<CaptureSysKeysMode>(settings.value(SER_CAPTURESYSKEYS,
                                                         static_cast<int>(CaptureSysKeysMode::CSK_OFF)).toInt());
    audioConfig = static_cast<AudioConfig>(settings.value(SER_AUDIOCFG,
                                                  static_cast<int>(AudioConfig::AC_STEREO)).toInt());
    videoCodecConfig = static_cast<VideoCodecConfig>(settings.value(SER_VIDEOCFG,
                                                  static_cast<int>(VideoCodecConfig::VCC_AUTO)).toInt());
    videoDecoderSelection = static_cast<VideoDecoderSelection>(settings.value(SER_VIDEODEC,
                                                  static_cast<int>(VideoDecoderSelection::VDS_AUTO)).toInt());
    windowMode = static_cast<WindowMode>(settings.value(SER_WINDOWMODE,
                                                        // Try to load from the old preference value too
                                                        static_cast<int>(settings.value(SER_FULLSCREEN, true).toBool() ?
                                                                             recommendedFullScreenMode : WindowMode::WM_WINDOWED)).toInt());
    uiDisplayMode = static_cast<UIDisplayMode>(settings.value(SER_UIDISPLAYMODE,
                                               static_cast<int>(settings.value(SER_STARTWINDOWED, true).toBool() ? UIDisplayMode::UI_WINDOWED
                                                                                                                 : UIDisplayMode::UI_MAXIMIZED)).toInt());
    language = static_cast<Language>(settings.value(SER_LANGUAGE,
                                                    static_cast<int>(Language::LANG_AUTO)).toInt());


    // Perform default settings updates as required based on last default version
    if (defaultVer < 1) {
#ifdef Q_OS_DARWIN
        // Update window mode setting on macOS from full-screen (old default) to borderless windowed (new default)
        if (windowMode == WindowMode::WM_FULLSCREEN) {
            windowMode = WindowMode::WM_FULLSCREEN_DESKTOP;
        }
#endif
    }
    if (defaultVer < 2) {
        if (windowMode == WindowMode::WM_FULLSCREEN && WMUtils::isRunningWayland()) {
            windowMode = WindowMode::WM_FULLSCREEN_DESKTOP;
        }
    }

    // Fixup VCC value to the new settings format with codec and HDR separate
    if (videoCodecConfig == VCC_FORCE_HEVC_HDR_DEPRECATED) {
        videoCodecConfig = VCC_AUTO;
        enableHdr = true;
    }
}

bool StreamingPreferences::retranslate()
{
    static QTranslator* translator = nullptr;

#if QT_VERSION < QT_VERSION_CHECK(5, 10, 0)
    if (m_QmlEngine != nullptr) {
        // Dynamic retranslation is not supported until Qt 5.10
        return false;
    }
#endif

    QTranslator* newTranslator = new QTranslator();
    QString languageSuffix = getSuffixFromLanguage(language);

    // Remove the old translator, even if we can't load a new one.
    // Otherwise we'll be stuck with the old translated values instead
    // of defaulting to English.
    if (translator != nullptr) {
        QCoreApplication::removeTranslator(translator);
        delete translator;
        translator = nullptr;
    }

    if (newTranslator->load(QString(":/languages/qml_") + languageSuffix)) {
        qInfo() << "Successfully loaded translation for" << languageSuffix;

        translator = newTranslator;
        QCoreApplication::installTranslator(translator);
    }
    else {
        qInfo() << "No translation available for" << languageSuffix;
        delete newTranslator;
    }

    if (m_QmlEngine != nullptr) {
#if QT_VERSION >= QT_VERSION_CHECK(5, 10, 0)
        // This is a dynamic retranslation from the settings page.
        // We have to kick the QML engine into reloading our text.
        m_QmlEngine->retranslate();
#else
        // Unreachable below Qt 5.10 due to the check above
        Q_ASSERT(false);
#endif
    }
    else {
        // This is a translation from a non-QML context, which means
        // it is probably app startup. There's nothing to refresh.
    }

    return true;
}

QString StreamingPreferences::getSuffixFromLanguage(StreamingPreferences::Language lang)
{
    switch (lang)
    {
    case LANG_DE:
        return "de";
    case LANG_EN:
        return "en";
    case LANG_FR:
        return "fr";
    case LANG_ZH_CN:
        return "zh_CN";
    case LANG_NB_NO:
        return "nb_NO";
    case LANG_RU:
        return "ru";
    case LANG_ES:
        return "es";
    case LANG_JA:
        return "ja";
    case LANG_VI:
        return "vi";
    case LANG_TH:
        return "th";
    case LANG_KO:
        return "ko";
    case LANG_HU:
        return "hu";
    case LANG_NL:
        return "nl";
    case LANG_SV:
        return "sv";
    case LANG_TR:
        return "tr";
    case LANG_UK:
        return "uk";
    case LANG_ZH_TW:
        return "zh_TW";
    case LANG_PT:
        return "pt";
    case LANG_PT_BR:
        return "pt_BR";
    case LANG_EL:
        return "el";
    case LANG_IT:
        return "it";
    case LANG_HI:
        return "hi";
    case LANG_PL:
        return "pl";
    case LANG_CS:
        return "cs";
    case LANG_HE:
        return "he";
    case LANG_CKB:
        return "ckb";
    case LANG_LT:
        return "lt";
    case LANG_ET:
        return "et";
    case LANG_BG:
        return "bg";
    case LANG_EO:
        return "eo";
    case LANG_TA:
        return "ta";
    case LANG_AUTO:
    default:
        return QLocale::system().name();
    }
}

void StreamingPreferences::save()
{
    const StreamingPreferencesSnapshot snapshot = makeSnapshot(this);

    QMutexLocker locker(&m_SaveMutex);
    ++m_SaveGeneration;
    saveSnapshot(snapshot);
}

void StreamingPreferences::saveAsync()
{
    const StreamingPreferencesSnapshot snapshot = makeSnapshot(this);

    QMutexLocker locker(&m_SaveMutex);
    const quint64 generation = ++m_SaveGeneration;
    QThreadPool::globalInstance()->start(new DeferredStreamingPreferencesSaveTask(&m_SaveMutex,
                                                                                  &m_SaveGeneration,
                                                                                  snapshot,
                                                                                  generation));
}

int StreamingPreferences::getDefaultBitrate(int width, int height, int fps, bool yuv444)
{
    // Don't scale bitrate linearly beyond 60 FPS. It's definitely not a linear
    // bitrate increase for frame rate once we get to values that high.
    float frameRateFactor = (fps <= 60 ? fps : (qSqrt(fps / 60.f) * 60.f)) / 30.f;

    // TODO: Collect some empirical data to see if these defaults make sense.
    // We're just using the values that the Shield used, as we have for years.
    static const struct resTable {
        int pixels;
        int factor;
    } resTable[] {
        { 640 * 360, 1 },
        { 854 * 480, 2 },
        { 1280 * 720, 5 },
        { 1920 * 1080, 10 },
        { 2560 * 1440, 20 },
        { 3840 * 2160, 40 },
        { -1, -1 },
    };

    // Calculate the resolution factor by linear interpolation of the resolution table
    float resolutionFactor;
    int pixels = width * height;
    for (int i = 0;; i++) {
        if (pixels == resTable[i].pixels) {
            // We can bail immediately for exact matches
            resolutionFactor = resTable[i].factor;
            break;
        }
        else if (pixels < resTable[i].pixels) {
            if (i == 0) {
                // Never go below the lowest resolution entry
                resolutionFactor = resTable[i].factor;
            }
            else {
                // Interpolate between the entry greater than the chosen resolution (i) and the entry less than the chosen resolution (i-1)
                resolutionFactor = ((float)(pixels - resTable[i-1].pixels) / (resTable[i].pixels - resTable[i-1].pixels)) * (resTable[i].factor - resTable[i-1].factor) + resTable[i-1].factor;
            }
            break;
        }
        else if (resTable[i].pixels == -1) {
            // Never go above the highest resolution entry
            resolutionFactor = resTable[i-1].factor;
            break;
        }
    }

    if (yuv444) {
        // This is rough estimation based on the fact that 4:4:4 doubles the amount of raw YUV data compared to 4:2:0
        resolutionFactor *= 2;
    }

    return qRound(resolutionFactor * frameRateFactor) * 1000;
}
