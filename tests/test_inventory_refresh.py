import os
from pathlib import Path
import unittest

from measure_inventory import measure


class InventoryRefresh(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires the Rust compatibility adapter")
    def test_request_budget_and_final_rescan(self):
        measure(Path(os.environ["COVEN_TEST_NATIVE"]), 8, 1, verify=True)
