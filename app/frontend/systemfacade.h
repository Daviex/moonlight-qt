#pragma once

#include "backend/systemproperties.h"
#include "frontend/frontendtypes.h"

#include <QObject>

class SystemFacade : public QObject
{
    Q_OBJECT

public:
    explicit SystemFacade(SystemProperties* systemProperties = nullptr, QObject* parent = nullptr);

    FrontendSystemProperties properties() const;
    FrontendDisplayInfo displayInfo(int displayIndex) const;
    void startAsyncLoad();
    void waitForAsyncLoad();
    void refreshDisplays();

signals:
    void propertiesChanged();
    void unmappedGamepadsChanged();
    void hasHardwareAccelerationChanged();
    void rendererAlwaysFullScreenChanged();
    void maximumResolutionChanged();
    void supportsHdrChanged();

private:
    QVector<FrontendDisplayInfo> displays() const;

    SystemProperties* m_SystemProperties;
    bool m_OwnsSystemProperties = false;
    bool m_DisplaysLoaded = false;
};
