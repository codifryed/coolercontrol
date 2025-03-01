#ifndef ADDRESSWIZARD_H
#define ADDRESSWIZARD_H

#include <QCheckBox>
#include <QLabel>
#include <QLineEdit>
#include <QSettings>
#include <QWizardPage>

class IntroPage final : public QWizardPage {
  Q_OBJECT

 public:
  explicit IntroPage(QWidget* parent = nullptr);

 private:
  QLabel* m_label;
};

class AddressPage final : public QWizardPage {
  Q_OBJECT

 public:
  explicit AddressPage(QWidget* parent = nullptr);

  void resetAddressInputValues() const;

 private:
  QLineEdit* m_addressLineEdit;
  QLineEdit* m_portLineEdit;
  QCheckBox* m_sslCheckbox;
};
#endif  // ADDRESSWIZARD_H
