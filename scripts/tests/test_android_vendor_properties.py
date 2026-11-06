"""Check vendor property discovery without hardcoded device branding."""
import subprocess
import unittest
from unittest.mock import patch

from scripts import prepare_android_vendor_properties as config


ROOT = """\
  222 100600 (1) 0 0 2700 date build.prop
  641 100644 (1) 0 0 19 date custom.prop
"""


class VendorPropertiesTests(unittest.TestCase):
    def test_uses_brand_and_empty_vendor_property_file_from_image(self):
        def run(args, **_kwargs):
            command = args[2]
            output = {
                "ls -l /": ROOT,
                "cat /build.prop": (
                    "ro.product.vendor.brand=example_brand\n"
                ),
                "cat /custom.prop": "# placeholder\n",
            }[command]
            return subprocess.CompletedProcess(args, 0, output, "")

        with patch.object(config.subprocess, "run", side_effect=run):
            brand, guest_path, properties = config.prepare(
                "image.img", "guestuser", "1000", "1000"
            )

        self.assertEqual(brand, "example_brand")
        self.assertEqual(guest_path, "/vendor/custom.prop")
        self.assertEqual(properties.splitlines(), [
            "example_brand.host.user=guestuser",
            "example_brand.host.uid=1000",
            "example_brand.host.gid=1000",
            "example_brand.stub_sensors_hal=1",
        ])

    def test_rejects_ambiguous_or_unsafe_vendor_metadata(self):
        with self.assertRaises(ValueError):
            config.vendor_brand("ro.product.vendor.brand=bad name\n")
        with self.assertRaises(ValueError):
            config.prepare("image.img", "bad\nname", "1", "1")


if __name__ == "__main__":
    unittest.main()
