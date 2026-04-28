#pragma once

#include "backend/autoupdatechecker.h"

#include <QObject>

class UpdateFacade : public QObject
{
    Q_OBJECT

public:
    explicit UpdateFacade(QObject* parent = nullptr);

    void start();

signals:
    void updateAvailable(QString newVersion, QString url);

private:
    AutoUpdateChecker m_UpdateChecker;
};
