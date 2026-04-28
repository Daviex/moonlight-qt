#pragma once

#include "computermanager.h"
#include <QDir>
#include <QImage>
#include <QThreadPool>
#include <QRunnable>

class BoxArtManager : public QObject
{
    Q_OBJECT

    friend class NetworkBoxArtLoadTask;

public:
    explicit BoxArtManager(QObject *parent = nullptr);

    QUrl
    loadBoxArt(NvComputer* computer, NvApp& app);

    static
    void
    deleteBoxArt(NvComputer* computer);

signals:
    void
    boxArtLoadComplete(QString computerUuid, NvApp app, QUrl image);

public slots:

private slots:
    void
    handleBoxArtLoadComplete(QString computerUuid, NvApp app, QUrl image);

private:
    QUrl
    loadBoxArtFromNetwork(NvAddress address,
                          uint16_t httpsPort,
                          QSslCertificate serverCert,
                          QString computerUuid,
                          int appId);

    QString
    getFilePathForBoxArt(QString computerUuid, int appId);

    QDir m_BoxArtDir;
    QThreadPool m_ThreadPool;
};
