"""What the upgrade contract accepts as the bytes being where they belong."""

import unittest

from contract.upgrade import SOURCES_HELD, _bytes_held_in_sources

VENDOR_STORES = ["antigravity", "claude", "codex"]


class BytesHeldInSourcesTest(unittest.TestCase):
    def held(self, before, after):
        return _bytes_held_in_sources([before, after])[0]

    def test_a_release_with_store_plugins_has_them_moved(self):
        self.assertTrue(
            self.held(["plugins"], [*VENDOR_STORES, SOURCES_HELD]),
        )

    def test_a_release_already_on_sources_has_nothing_to_move(self):
        self.assertTrue(
            self.held(VENDOR_STORES, [*VENDOR_STORES, SOURCES_HELD]),
        )

    def test_store_plugins_left_behind_fails_whatever_came_before(self):
        for before in (["plugins"], VENDOR_STORES):
            self.assertFalse(self.held(before, ["plugins", SOURCES_HELD]))

    def test_no_sources_after_the_upgrade_fails_whatever_came_before(self):
        for before in (["plugins"], VENDOR_STORES):
            self.assertFalse(self.held(before, VENDOR_STORES))

    def test_a_layout_never_printed_fails(self):
        self.assertFalse(_bytes_held_in_sources([VENDOR_STORES])[0])
        self.assertFalse(_bytes_held_in_sources([])[0])


if __name__ == "__main__":
    unittest.main()
