#pragma once

#include "backend/computermanager.h"
#include "frontend/computerlistfacade.h"
#include "frontend/preferencesfacade.h"
#include "frontend/sessioncoordinator.h"
#include "frontend/systemfacade.h"
#include "frontend/updatefacade.h"

#include <QObject>

class AppListFacade;

class FrontendApplicationFacade : public QObject
{
    Q_OBJECT

public:
    explicit FrontendApplicationFacade(QObject* parent = nullptr);

    void initialize(ComputerManager* computerManager);

    ComputerListFacade* computers();
    PreferencesFacade* preferences();
    SystemFacade* system();
    UpdateFacade* updates();
    FrontendSessionCoordinator* sessions();

    AppListFacade* createAppList(int computerIndex, bool showHiddenGames, QObject* parent = nullptr);

private:
    ComputerListFacade m_Computers;
    PreferencesFacade m_Preferences;
    SystemFacade m_System;
    UpdateFacade m_Updates;
    FrontendSessionCoordinator m_Sessions;
    ComputerManager* m_ComputerManager = nullptr;
};
