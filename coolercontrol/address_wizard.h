// CoolerControl - monitor and control your cooling and other devices
// Copyright (c) 2021-2025  Guy Boldon and contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#ifndef ADDRESSWIZARD_H
#define ADDRESSWIZARD_H

#include <QCheckBox>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
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
  QPushButton* m_defaultButton;
};
#endif  // ADDRESSWIZARD_H
