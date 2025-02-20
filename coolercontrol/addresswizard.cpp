#include "addresswizard.h"
#include "constants.h"

#include <QVBoxLayout>
#include <QLineEdit>
#include <QCheckBox>
#include <QIntValidator>
#include <QRegularExpressionValidator>
#include <QRegularExpression>

IntroPage::IntroPage(QWidget *parent)
    : QWizardPage(parent) {
    label = new QLabel(
        "<p>A connection to the CoolerControl Daemon could not be established.<br/>"
        "Please make sure that the systemd service is running and available.</p>"
        "<p>Check the <a href=\"https:/gitlab.com/coolercontrol/coolercontrol/\" target=\"_blank\">project page</a>"
        " for installation instructions.</p>"
        "<p>Some helpful commands to enable and verify the daemon status:</p>"
        "<p><code>"
        "sudo systemctl enable --now coolercontrold<br />"
        "sudo systemctl status coolercontrold<br />"
        "</code></p>"
        "<p>If you have configured a non-standard address to connect to the daemon, you can set it in the following steps: </p>"
    );
    label->setWordWrap(true);

    auto *layout = new QVBoxLayout;
    layout->addWidget(label);
    setLayout(layout);
}

AddressPage::AddressPage(QWidget *parent)
    : QWizardPage(parent) {
    setTitle("Daemon Address - Desktop Application");
    setSubTitle("Adjust the address fields as necessary.");

    auto *addressLabel = new QLabel("Host address:");
    addressLineEdit = new QLineEdit;
    addressLabel->setBuddy(addressLineEdit);
    addressLineEdit->setToolTip(
        "The IPv4, IPv6 address or hostname to use to communicate with the daemon."
    );
    addressLineEdit->setValidator(
        new QRegularExpressionValidator(QRegularExpression("[0-9a-zA-Z.-]+"))
    );
    registerField("address", addressLineEdit);

    auto *portLabel = new QLabel("Port:");
    portLineEdit = new QLineEdit;
    portLabel->setBuddy(portLineEdit);
    portLineEdit->setToolTip("The port number to use to communicate with the daemon.");
    portLineEdit->setValidator(new QIntValidator(80, 65535, portLineEdit));
    registerField("port", portLineEdit);

    sslCheckbox = new QCheckBox("SSL/TLS");
    sslCheckbox->setToolTip("Enable or disable SSL/TLS (HTTPS)");
    registerField("ssl", sslCheckbox);

    auto *layout = new QGridLayout;
    layout->addWidget(addressLabel, 0, 0);
    layout->addWidget(addressLineEdit, 0, 1);
    layout->addWidget(portLabel, 1, 0);
    layout->addWidget(portLineEdit, 1, 1);
    layout->addWidget(sslCheckbox, 2, 0, 1, 2);
    setLayout(layout);

    const QSettings settings;
    addressLineEdit->setText(
        settings.value(
            SETTING_DAEMON_ADDRESS,
            DEFAULT_DAEMON_ADDRESS.data()
        ).toString()
    );
    portLineEdit->setText(
        QString::number(settings.value(SETTING_DAEMON_PORT, DEFAULT_DAEMON_PORT).toInt())
    );
    sslCheckbox->setChecked(settings.value(SETTING_DAEMON_SSL_ENABLED, DEFAULT_DAEMON_SSL_ENABLED).toBool());
}

void AddressPage::resetAddressInputValues() const {
    addressLineEdit->setText(DEFAULT_DAEMON_ADDRESS.data());
    portLineEdit->setText(QString::number(DEFAULT_DAEMON_PORT));
    sslCheckbox->setChecked(DEFAULT_DAEMON_SSL_ENABLED);
}
