#ifndef IPC_H
#define IPC_H

#include <QSettings>


/*
    An instance of this class gets published over the WebChannel and is then accessible to HTML clients.
*/
class IPC final : public QObject {
    Q_OBJECT

public:
    explicit IPC(QObject *parent = nullptr);

    Q_INVOKABLE [[nodiscard]] bool getStartInTray() const;

    Q_INVOKABLE [[nodiscard]] int getStartupDelay() const;

    Q_INVOKABLE [[nodiscard]] bool getCloseToTray() const;

    Q_INVOKABLE [[nodiscard]] QByteArray getWindowGeometry() const;

    /*
        Signals are emitted from the C++ side and are handed to callbacks on the JS client side.
    */
signals:
    void sendText(const QString &text);

    /*
        Slots are invoked from the JS client side and are processed on the server side.
    */
public slots:
    void setStartInTray(bool startInTray) const;

    void setStartupDelay(int startupDelay) const;

    void setCloseToTray(bool closeToTray) const;

    void saveWindowGeometry(const QByteArray& geometry) const;

    void forceQuit() const;

    void syncSettings() const;

private:
    QSettings *settings;
};

#endif //IPC_H
