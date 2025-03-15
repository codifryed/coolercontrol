#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2025  Guy Boldon
#  |
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU General Public License as published by
#  the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#  |
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU General Public License for more details.
#  |
#  You should have received a copy of the GNU General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
# ----------------------------------------------------------------------------------------------------------------------

import unittest

from coolercontrol_liqctld.liqctld import system_info


class MyTestCase(unittest.TestCase):
    def test_get_system_info(self):
        output = system_info()
        self.assertIsNotNone(output)
        self.assertGreater(len(output), 1)


if __name__ == "__main__":
    unittest.main()
