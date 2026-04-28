#include "boxartmanager.h"
#include "../path.h"

#include <QCryptographicHash>
#include <QFileInfo>
#include <QImageReader>
#include <QImageWriter>
#include <QRegularExpression>

namespace {
    QString getSafeBoxArtCacheKey(QString computerUuid)
    {
        QString trimmedUuid = computerUuid.trimmed();
        static const QRegularExpression safeUuidRegex(QStringLiteral("^[0-9A-Fa-f-]{1,64}$"));
        if (safeUuidRegex.match(trimmedUuid).hasMatch()) {
            return trimmedUuid;
        }

        qWarning() << "Using hashed box art cache key for unsafe host UUID:" << computerUuid;
        return QStringLiteral("host-") +
               QString::fromLatin1(QCryptographicHash::hash(computerUuid.toUtf8(), QCryptographicHash::Sha256).toHex());
    }

    bool isInDirectory(QString childPath, QString parentPath)
    {
        QString childCanonical = QDir::cleanPath(QFileInfo(childPath).canonicalFilePath());
        QString parentCanonical = QDir::cleanPath(QFileInfo(parentPath).canonicalFilePath());
        if (childCanonical.isEmpty() || parentCanonical.isEmpty()) {
            return false;
        }

        return childCanonical == parentCanonical ||
               childCanonical.startsWith(parentCanonical + QLatin1Char('/'));
    }
}

BoxArtManager::BoxArtManager(QObject *parent) :
    QObject(parent),
    m_BoxArtDir(Path::getBoxArtCacheDir()),
    m_ThreadPool(this)
{
    // 4 is a good balance between fast loading for large
    // app grids and not crushing GFE with tons of requests
    // and causing UI jank from constantly stalling to decode
    // new images.
    m_ThreadPool.setMaxThreadCount(4);
    if (!m_BoxArtDir.exists()) {
        if (!m_BoxArtDir.mkpath(".")) {
            qWarning() << "Failed to create box art cache directory:" << m_BoxArtDir.absolutePath();
        }
    }
}

QString
BoxArtManager::getFilePathForBoxArt(QString computerUuid, int appId)
{
    QDir dir = m_BoxArtDir;
    QString cacheKey = getSafeBoxArtCacheKey(computerUuid);

    if (!dir.exists() && !dir.mkpath(".")) {
        qWarning() << "Failed to create box art cache directory:" << dir.absolutePath();
        return QString();
    }

    // Create the cache directory if it did not already exist
    if (!dir.exists(cacheKey) && !dir.mkdir(cacheKey)) {
        qWarning() << "Failed to create host box art cache directory:" << cacheKey;
        return QString();
    }

    // Change to this computer's box art cache folder
    if (!dir.cd(cacheKey)) {
        qWarning() << "Failed to enter host box art cache directory:" << cacheKey;
        return QString();
    }

    if (!isInDirectory(dir.absolutePath(), m_BoxArtDir.absolutePath())) {
        qWarning() << "Refusing unsafe box art cache path:" << dir.absolutePath();
        return QString();
    }

    // Try to open the cached file
    return dir.filePath(QString::number(appId) + ".png");
}

class NetworkBoxArtLoadTask : public QObject, public QRunnable
{
    Q_OBJECT

public:
    NetworkBoxArtLoadTask(BoxArtManager* boxArtManager, NvComputer* computer, NvApp& app)
        : m_Bam(boxArtManager),
          m_App(app)
    {
        QReadLocker lock(&computer->lock);
        m_ComputerUuid = computer->uuid;
        m_ActiveAddress = computer->activeAddress;
        m_ActiveHttpsPort = computer->activeHttpsPort;
        m_ServerCert = computer->serverCert;

        connect(this, &NetworkBoxArtLoadTask::boxArtFetchCompleted,
                boxArtManager, &BoxArtManager::handleBoxArtLoadComplete);
    }

signals:
    void boxArtFetchCompleted(QString computerUuid, NvApp app, QUrl image);

private:
    void run()
    {
        QUrl image = m_Bam->loadBoxArtFromNetwork(m_ActiveAddress, m_ActiveHttpsPort,
                                                  m_ServerCert, m_ComputerUuid, m_App.id);
        if (image.isEmpty()) {
            // Give it another shot if it fails once
            image = m_Bam->loadBoxArtFromNetwork(m_ActiveAddress, m_ActiveHttpsPort,
                                                 m_ServerCert, m_ComputerUuid, m_App.id);
        }
        emit boxArtFetchCompleted(m_ComputerUuid, m_App, image);
    }

    BoxArtManager* m_Bam;
    QString m_ComputerUuid;
    NvAddress m_ActiveAddress;
    uint16_t m_ActiveHttpsPort;
    QSslCertificate m_ServerCert;
    NvApp m_App;
};

QUrl BoxArtManager::loadBoxArt(NvComputer* computer, NvApp& app)
{
    // Try to open the cached file if it exists and contains data
    QString cachePath = getFilePathForBoxArt(computer->uuid, app.id);
    if (!cachePath.isEmpty()) {
        QFile cacheFile(cachePath);
        if (cacheFile.exists() && cacheFile.size() > 0) {
            return QUrl::fromLocalFile(cacheFile.fileName());
        }
    }

    // If we get here, we need to fetch asynchronously.
    // Kick off a worker on our thread pool to do just that.
    NetworkBoxArtLoadTask* netLoadTask = new NetworkBoxArtLoadTask(this, computer, app);
    m_ThreadPool.start(netLoadTask);

    // Return the placeholder then we can notify the caller
    // later when the real image is ready.
    return QUrl("qrc:/res/no_app_image.png");
}

void BoxArtManager::deleteBoxArt(NvComputer* computer)
{
    QDir dir(Path::getBoxArtCacheDir());
    QString cacheKey = getSafeBoxArtCacheKey(computer->uuid);

    // Delete everything in this computer's box art directory
    if (dir.cd(cacheKey)) {
        if (!isInDirectory(dir.absolutePath(), Path::getBoxArtCacheDir())) {
            qWarning() << "Refusing unsafe box art cache deletion:" << dir.absolutePath();
            return;
        }

        if (!dir.removeRecursively()) {
            qWarning() << "Failed to delete box art cache directory:" << dir.absolutePath();
        }
    }
}

void BoxArtManager::handleBoxArtLoadComplete(QString computerUuid, NvApp app, QUrl image)
{
    if (!image.isEmpty()) {
        emit boxArtLoadComplete(computerUuid, app, image);
    }
}

QUrl BoxArtManager::loadBoxArtFromNetwork(NvAddress address,
                                           uint16_t httpsPort,
                                           QSslCertificate serverCert,
                                           QString computerUuid,
                                           int appId)
{
    NvHTTP http(address, httpsPort, serverCert);

    QString cachePath = getFilePathForBoxArt(computerUuid, appId);
    if (cachePath.isEmpty()) {
        return QUrl();
    }

    QImage image;
    try {
        image = http.getBoxArt(appId);
    } catch (const std::exception& e) {
        qWarning() << "Failed to load box art from network:" << e.what();
    } catch (...) {
        qWarning() << "Failed to load box art from network with unknown error";
    }

    // Cache the box art on disk if it loaded
    if (!image.isNull()) {
        if (image.save(cachePath)) {
            return QUrl::fromLocalFile(cachePath);
        }
        else {
            // A failed save() may leave a zero byte file. Make sure that's removed.
            qWarning() << "Failed to save box art cache file:" << cachePath;
            QFile(cachePath).remove();
        }
    }

    return QUrl();
}

#include "boxartmanager.moc"
