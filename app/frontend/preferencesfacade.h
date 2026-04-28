#pragma once

#include "frontend/frontendtypes.h"
#include "settings/streamingpreferences.h"

#include <QObject>

class PreferencesFacade : public QObject
{
    Q_OBJECT

public:
    explicit PreferencesFacade(StreamingPreferences* preferences = nullptr, QObject* parent = nullptr);

    FrontendStreamingPreferences preferences() const;
    void applyPreferences(const FrontendStreamingPreferences& preferences, bool saveAfterApply);
    void reload();
    void save();
    int getDefaultBitrate(int width, int height, int fps, bool yuv444) const;
    bool retranslate();

signals:
    void preferencesChanged();

private:
    StreamingPreferences* m_Preferences;
};
