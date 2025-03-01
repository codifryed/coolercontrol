#include "address_wizard.h"

#include <QCheckBox>
#include <QIntValidator>
#include <QLineEdit>
#include <QRegularExpression>
#include <QRegularExpressionValidator>
#include <QVBoxLayout>

#include "constants.h"

IntroPage::IntroPage(QWidget* parent) : QWizardPage(parent) {
  m_label = new QLabel(
      "<p>A connection to the CoolerControl Daemon could not be established.<br/>"
      "Please make sure that the systemd service is running and available.</p>"
      "<p>Check the <a href=\"https://docs.coolercontrol.org\" target=\"_blank\">docs website</a>"
      " for installation instructions.</p>"
      "<p>Some helpful commands to enable and verify the daemon status:</p>"
      "<p><code>"
      "sudo systemctl enable --now coolercontrold<br />"
      "sudo systemctl status coolercontrold<br />"
      "</code></p>"
      "<p>If you have configured a non-standard address to connect to the daemon, you can set it "
      "in the following steps: </p>");
  m_label->setWordWrap(true);
  m_label->setOpenExternalLinks(true);
  m_label->setTextInteractionFlags(Qt::TextSelectableByMouse | Qt::LinksAccessibleByMouse);

  auto* layout = new QVBoxLayout;
  layout->addWidget(m_label);
  setLayout(layout);
}

AddressPage::AddressPage(QWidget* parent) : QWizardPage(parent) {
  setTitle("Daemon Address - Desktop Application");
  setSubTitle("Adjust the address fields as necessary.");

  auto* addressLabel = new QLabel("Host address:");
  m_addressLineEdit = new QLineEdit;
  addressLabel->setBuddy(m_addressLineEdit);
  m_addressLineEdit->setToolTip(
      "The IPv4, IPv6 address or hostname to use to communicate with the daemon.");
  m_addressLineEdit->setValidator(
      new QRegularExpressionValidator(QRegularExpression("[0-9a-zA-Z.-]+")));
  registerField("address", m_addressLineEdit);

  auto* portLabel = new QLabel("Port:");
  m_portLineEdit = new QLineEdit;
  portLabel->setBuddy(m_portLineEdit);
  m_portLineEdit->setToolTip("The port number to use to communicate with the daemon.");
  m_portLineEdit->setValidator(new QIntValidator(80, 65535, m_portLineEdit));
  registerField("port", m_portLineEdit);

  m_sslCheckbox = new QCheckBox("SSL/TLS");
  m_sslCheckbox->setToolTip("Enable or disable SSL/TLS (HTTPS)");
  registerField("ssl", m_sslCheckbox);

  auto* layout = new QGridLayout;
  layout->addWidget(addressLabel, 0, 0);
  layout->addWidget(m_addressLineEdit, 0, 1);
  layout->addWidget(portLabel, 1, 0);
  layout->addWidget(m_portLineEdit, 1, 1);
  layout->addWidget(m_sslCheckbox, 2, 0, 1, 2);
  setLayout(layout);

  const QSettings settings;
  m_addressLineEdit->setText(
      settings.value(SETTING_DAEMON_ADDRESS, DEFAULT_DAEMON_ADDRESS.data()).toString());
  m_portLineEdit->setText(
      QString::number(settings.value(SETTING_DAEMON_PORT, DEFAULT_DAEMON_PORT).toInt()));
  m_sslCheckbox->setChecked(
      settings.value(SETTING_DAEMON_SSL_ENABLED, DEFAULT_DAEMON_SSL_ENABLED).toBool());
}

void AddressPage::resetAddressInputValues() const {
  m_addressLineEdit->setText(DEFAULT_DAEMON_ADDRESS.data());
  m_portLineEdit->setText(QString::number(DEFAULT_DAEMON_PORT));
  m_sslCheckbox->setChecked(DEFAULT_DAEMON_SSL_ENABLED);
}
