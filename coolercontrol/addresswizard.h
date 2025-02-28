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
  QLabel* label;
};

class AddressPage final : public QWizardPage {
  Q_OBJECT

 public:
  explicit AddressPage(QWidget* parent = nullptr);

  void resetAddressInputValues() const;

 private:
  QLineEdit* addressLineEdit;
  QLineEdit* portLineEdit;
  QCheckBox* sslCheckbox;
};
#endif  // ADDRESSWIZARD_H
