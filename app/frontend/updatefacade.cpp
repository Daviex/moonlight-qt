#include "updatefacade.h"

UpdateFacade::UpdateFacade(QObject* parent)
    : QObject(parent),
      m_UpdateChecker(this)
{
    connect(&m_UpdateChecker, &AutoUpdateChecker::onUpdateAvailable,
            this, &UpdateFacade::updateAvailable);
}

void UpdateFacade::start()
{
    m_UpdateChecker.start();
}
