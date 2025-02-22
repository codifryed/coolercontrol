#ifndef IPC_H
#define IPC_H

#include <iostream>
#include "constants.h"

#include <QObject>


/*
    An instance of this class gets published over the WebChannel and is then accessible to HTML clients.
*/
class IPC final : public QObject {
    Q_OBJECT

public:
    explicit IPC(QObject *parent = nullptr)
        : QObject(parent)
          , settings(new QSettings) {
        //        connect(dialog, &Dialog::sendText, this, &Core::sendText);
    }

    Q_INVOKABLE [[nodiscard]] bool getStartInTray() const {
        return settings->value(SETTING_START_IN_TRAY.data(), false).toBool();
    }

    Q_INVOKABLE [[nodiscard]] int getStartupDelay() const {
        return settings->value(SETTING_STARTUP_DELAY.data(), 0).toInt();
    }

    /*
        Signals are emitted from the C++ side and are handed to callbacks on the JS client side.
    */
signals:
    void sendText(const QString &text);

    /*
        Slots are invoked from the JS client side and are processed on the server side.
    */
public slots:
    void receiveText(const QString &text) {
        std::cout << "Received text: " << text.toStdString() << std::endl;
        // //        m_dialog->displayMessage(Dialog::tr("Received message: %1").arg(text));
    }

    void setStartInTray(const bool startInTray) const {
        settings->setValue(SETTING_START_IN_TRAY.data(), startInTray);
    }

    void setStartupDelay(const int startupDelay) const {
        settings->setValue(SETTING_STARTUP_DELAY.data(), startupDelay);
    }

private:
    QSettings *settings;
};

#endif //IPC_H
