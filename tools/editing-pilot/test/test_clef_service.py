# SPDX-License-Identifier: MIT OR Apache-2.0
import unittest
from types import SimpleNamespace
from clef_service import MAX_TOKENS, MODEL, validate_request


class ClefServiceTests(unittest.TestCase):
    def test_full_context_must_fit(self):
        request = {'model': MODEL, 'state': 'context', 'questions': {
            'edit': {'type': 'choice', 'criteria': {'keep': 'Keep footage', 'none': 'Abstain'}}}}
        processor = SimpleNamespace(tokenizer=None)
        def encode(*args, **kwargs):
            return SimpleNamespace(input_ids=range(MAX_TOKENS + 1))
        with self.assertRaisesRegex(ValueError, 'exceeds'):
            validate_request(request, processor, encode)
        validate_request(request, processor, lambda *a, **k: SimpleNamespace(input_ids=[1]))
        with self.assertRaisesRegex(ValueError, 'Browser origins'):
            validate_request(request, processor, encode, 'https://example.com')
        request['images'] = []
        with self.assertRaisesRegex(ValueError, 'text state'):
            validate_request(request, processor, encode)


if __name__ == '__main__':
    unittest.main()
