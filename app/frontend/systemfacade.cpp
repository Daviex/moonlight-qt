#include "systemfacade.h"

#include <QVariant>

namespace {
    constexpr int MaxDisplaysToSnapshot = 16;
}

SystemFacade::SystemFacade(SystemProperties* systemProperties, QObject* parent)
    : QObject(parent),
      m_SystemProperties(systemProperties ? systemProperties : new SystemProperties()),
      m_OwnsSystemProperties(systemProperties == nullptr)
{
    if (m_OwnsSystemProperties) {
        m_SystemProperties->setParent(this);
    }

    connect(m_SystemProperties, &SystemProperties::unmappedGamepadsChanged, this, [this]() {
        emit unmappedGamepadsChanged();
        emit propertiesChanged();
    });
    connect(m_SystemProperties, &SystemProperties::hasHardwareAccelerationChanged, this, [this]() {
        emit hasHardwareAccelerationChanged();
        emit propertiesChanged();
    });
    connect(m_SystemProperties, &SystemProperties::rendererAlwaysFullScreenChanged, this, [this]() {
        emit rendererAlwaysFullScreenChanged();
        emit propertiesChanged();
    });
    connect(m_SystemProperties, &SystemProperties::maximumResolutionChanged, this, [this]() {
        emit maximumResolutionChanged();
        emit propertiesChanged();
    });
    connect(m_SystemProperties, &SystemProperties::supportsHdrChanged, this, [this]() {
        emit supportsHdrChanged();
        emit propertiesChanged();
    });
}

FrontendSystemProperties SystemFacade::properties() const
{
    FrontendSystemProperties snapshot;

    snapshot.isRunningWayland = m_SystemProperties->property("isRunningWayland").toBool();
    snapshot.isRunningXWayland = m_SystemProperties->property("isRunningXWayland").toBool();
    snapshot.isWow64 = m_SystemProperties->property("isWow64").toBool();
    snapshot.hasDesktopEnvironment = m_SystemProperties->property("hasDesktopEnvironment").toBool();
    snapshot.hasBrowser = m_SystemProperties->property("hasBrowser").toBool();
    snapshot.hasDiscordIntegration = m_SystemProperties->property("hasDiscordIntegration").toBool();
    snapshot.usesMaterial3Theme = m_SystemProperties->property("usesMaterial3Theme").toBool();
    snapshot.hasHardwareAcceleration = m_SystemProperties->property("hasHardwareAcceleration").toBool();
    snapshot.rendererAlwaysFullScreen = m_SystemProperties->property("rendererAlwaysFullScreen").toBool();
    snapshot.supportsHdr = m_SystemProperties->property("supportsHdr").toBool();
    snapshot.friendlyNativeArchName = m_SystemProperties->property("friendlyNativeArchName").toString();
    snapshot.versionString = m_SystemProperties->property("versionString").toString();
    snapshot.unmappedGamepads = m_SystemProperties->property("unmappedGamepads").toString();
    snapshot.maximumResolution = m_SystemProperties->property("maximumResolution").toSize();
    if (m_DisplaysLoaded) {
        snapshot.displays = displays();
    }

    return snapshot;
}

FrontendDisplayInfo SystemFacade::displayInfo(int displayIndex) const
{
    FrontendDisplayInfo snapshot;
    snapshot.nativeResolution = m_SystemProperties->getNativeResolution(displayIndex);
    snapshot.safeAreaResolution = m_SystemProperties->getSafeAreaResolution(displayIndex);
    snapshot.refreshRate = m_SystemProperties->getRefreshRate(displayIndex);
    return snapshot;
}

QVector<FrontendDisplayInfo> SystemFacade::displays() const
{
    QVector<FrontendDisplayInfo> snapshot;

    for (int i = 0; i < MaxDisplaysToSnapshot; i++) {
        FrontendDisplayInfo display = displayInfo(i);
        if (display.nativeResolution.isNull() && display.safeAreaResolution.isNull() && display.refreshRate == 0) {
            break;
        }
        snapshot.append(display);
    }

    return snapshot;
}

void SystemFacade::startAsyncLoad()
{
    m_SystemProperties->startAsyncLoad();
}

void SystemFacade::waitForAsyncLoad()
{
    m_SystemProperties->waitForAsyncLoad();
}

void SystemFacade::refreshDisplays()
{
    m_SystemProperties->refreshDisplays();
    m_DisplaysLoaded = true;
    emit propertiesChanged();
}
