#pragma once

#include "backend/computermanager.h"
#include "frontend/frontendtypes.h"
#include "streaming/session.h"

#include <QObject>
#include <QVector>

class ComputerListFacade : public QObject
{
    Q_OBJECT

public:
    explicit ComputerListFacade(QObject* parent = nullptr);

    void initialize(ComputerManager* computerManager);

    int count() const;
    QVector<FrontendComputer> computers() const;
    FrontendComputer computerAt(int computerIndex) const;

    void deleteComputer(int computerIndex);
    QString generatePinString();
    void pairComputer(int computerIndex, const QString& pin);
    void testConnectionForComputer(int computerIndex);
    void wakeComputer(int computerIndex);
    void renameComputer(int computerIndex, const QString& name);
    Session* createSessionForCurrentGame(int computerIndex);

signals:
    void computersReset();
    void computerChanged(int computerIndex);
    void pairingCompleted(QString error);
    void connectionTestCompleted(int result, QString blockedPorts);

private slots:
    void handleComputerStateChanged(NvComputer* computer);
    void handlePairingCompleted(NvComputer* computer, QString error);

private:
    bool isValidComputerIndex(int computerIndex, const char* operation) const;
    FrontendComputer snapshotComputer(NvComputer* computer) const;

    QVector<NvComputer*> m_Computers;
    ComputerManager* m_ComputerManager = nullptr;
};
