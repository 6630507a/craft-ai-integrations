import base64
import json
import threading
import unittest
import urllib.error
import urllib.request
from http.server import HTTPServer

from server import handler_for


class BridgeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.seen = []

        def fake(raw, prompt):
            cls.seen.append((raw, prompt))
            return {"width": 12, "height": 8, "boxes": [[1, 2, 3, 4]], "scores": [0.7]}

        cls.server = HTTPServer(("127.0.0.1", 0), handler_for(fake))
        cls.url = "http://127.0.0.1:%s/v1" % cls.server.server_port
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=2)

    def test_health_and_detection(self):
        with urllib.request.urlopen(self.url + "/health") as resp:
            self.assertEqual(json.load(resp), {"model": "sam3", "ready": True})
        image = base64.b64encode(b"test image").decode()
        data = json.dumps({"prompt": "a car", "image": {"mime_type": "image/png", "data": image}}).encode()
        with urllib.request.urlopen(urllib.request.Request(self.url + "/detect", data=data)) as resp:
            self.assertEqual(json.load(resp)["boxes"], [[1, 2, 3, 4]])
        self.assertEqual(self.seen[-1], (b"test image", "a car"))

    def test_invalid_content_fails_without_running_inference(self):
        prior = len(self.seen)
        data = json.dumps({"prompt": "x", "image": {"mime_type": "image/png", "data": "!"}}).encode()
        with self.assertRaises(urllib.error.HTTPError) as err:
            urllib.request.urlopen(urllib.request.Request(self.url + "/detect", data=data))
        self.assertEqual(err.exception.code, 400)
        self.assertEqual(len(self.seen), prior)

    def test_oversized_request_fails_before_reading(self):
        req = urllib.request.Request(self.url + "/detect", data=b"{}", headers={"Content-Length": str(25 * 1024 * 1024)})
        with self.assertRaises(urllib.error.HTTPError) as err:
            urllib.request.urlopen(req)
        self.assertIn(err.exception.code, (400, 413))


if __name__ == "__main__":
    unittest.main()
