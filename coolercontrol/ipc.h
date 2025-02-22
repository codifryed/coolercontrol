#ifndef IPC_H
#define IPC_H

#include <iostream>
#include <QObject>

/*
    An instance of this class gets published over the WebChannel and is then accessible to HTML clients.
*/
class IPC final : public QObject
{
    Q_OBJECT

public:
    explicit IPC(QObject *parent = nullptr) : QObject(parent)
    {
//        connect(dialog, &Dialog::sendText, this, &Core::sendText);
    }

    // todo: adjust for specific signals and slots: (setCloseToTray, etc)
    /*
        This signal is emitted from the C++ side and the text displayed on the HTML client side.
    */
signals:
    void sendText(const QString &text);

    /*
        This slot is invoked from the HTML client side and the text displayed on the server side.
    */
public slots:

     void receiveText(const QString &text)
     {
         std::cout << "Received text: " << text.toStdString() << std::endl;
// //        m_dialog->displayMessage(Dialog::tr("Received message: %1").arg(text));
     }

// private:

};

#endif //IPC_H
