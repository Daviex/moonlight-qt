QT += quick quickcontrols2 testlib
CONFIG += testcase c++17
TARGET = tst_game_settings
INCLUDEPATH += ../../app
SOURCES += tst_game_settings.cpp \
    ../../app/settings/streamingpreferences.cpp \
    ../../app/settings/gamestreamingsettings.cpp \
    ../../app/cli/commandlineparser.cpp
HEADERS += ../../app/settings/streamingpreferences.h ../../app/settings/gamestreamingsettings.h
RESOURCES += ../../app/qml.qrc ../../app/resources.qrc ../navigation/mocks.qrc
