#include "applicationfacade.h"

#include "frontend/applistfacade.h"

#include <QDebug>

FrontendApplicationFacade::FrontendApplicationFacade(QObject* parent)
    : QObject(parent),
      m_Computers(this),
      m_Preferences(nullptr, this),
      m_System(nullptr, this),
      m_Updates(this),
      m_Sessions(this)
{
}

void FrontendApplicationFacade::initialize(ComputerManager* computerManager)
{
    m_ComputerManager = computerManager;
    m_Computers.initialize(computerManager);
}

ComputerListFacade* FrontendApplicationFacade::computers()
{
    return &m_Computers;
}

PreferencesFacade* FrontendApplicationFacade::preferences()
{
    return &m_Preferences;
}

SystemFacade* FrontendApplicationFacade::system()
{
    return &m_System;
}

UpdateFacade* FrontendApplicationFacade::updates()
{
    return &m_Updates;
}

FrontendSessionCoordinator* FrontendApplicationFacade::sessions()
{
    return &m_Sessions;
}

AppListFacade* FrontendApplicationFacade::createAppList(int computerIndex, bool showHiddenGames, QObject* parent)
{
    if (m_ComputerManager == nullptr) {
        qWarning() << "FrontendApplicationFacade::createAppList called before initialize";
        return nullptr;
    }

    auto appList = new AppListFacade(parent);
    appList->initialize(m_ComputerManager, computerIndex, showHiddenGames);
    return appList;
}
