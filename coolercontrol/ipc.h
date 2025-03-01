#ifndef IPC_H
#define IPC_H

#include <QSettings>

/*
    An instance of this class gets published over the WebChannel and is then accessible to HTML
   clients.
*/
class IPC final : public QObject {
  Q_OBJECT

 public:
  explicit IPC(QObject* parent = nullptr);

  Q_INVOKABLE [[nodiscard]] bool getStartInTray() const;

  Q_INVOKABLE [[nodiscard]] int getStartupDelay() const;

  Q_INVOKABLE [[nodiscard]] bool getCloseToTray() const;

  Q_INVOKABLE [[nodiscard]] double getZoomFactor() const;

  Q_INVOKABLE [[nodiscard]] QByteArray getWindowGeometry() const;

  Q_INVOKABLE [[nodiscard]] QString filePathDialog(const QString& title) const;

  Q_INVOKABLE [[nodiscard]] QString directoryPathDialog(const QString& title) const;

  /*
      Signals are emitted from the C++ side and are handed to callbacks on the JS client side.
  */
 signals:
  void sendText(const QString& text);

  void webLoadFinished() const;

  /*
      Slots are invoked from the JS client side and are processed on the server side.
  */
 public slots:
  void setStartInTray(bool startInTray) const;

  void setStartupDelay(int startupDelay) const;

  void setCloseToTray(bool closeToTray) const;

  void setZoomFactor(double zoomFactor) const;

  void setModes(const QString& modesJson) const;

  void saveWindowGeometry(const QByteArray& geometry) const;

  void acknowledgeDaemonIssues() const;

  void forceQuit() const;

  void syncSettings() const;

  void loadFinished() const { emit webLoadFinished(); }

 private:
  QSettings* m_settings;
};

#endif  // IPC_H
