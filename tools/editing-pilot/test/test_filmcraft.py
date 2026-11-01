# SPDX-License-Identifier: MIT OR Apache-2.0
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1]))
import filmcraft


class FilmCraftTests(unittest.TestCase):
    def test_native_captions_derive_from_the_shared_pages_instead_of_a_stale_snapshot(self):
        p={'duration':90,'wordCaptions':[{'id':1}],'captions':[{'text':'stale'}],
           'captionPages':[{'startMs':1000,'endMs':1500,'text':'Approved words'}]}
        self.assertEqual(filmcraft.native_caption_groups(p),[{'start':30,'end':45,'text':'Approved words'}])
        with self.assertRaises(ValueError):filmcraft.native_caption_groups(dict(p,captionPages=[]))

    def test_named_commands_preserve_their_name_parameter(self):
        self.assertEqual(filmcraft.command('file.newSequence', name='Audition'), {'id':'file.newSequence','params':{'name':'Audition'}})

    def test_overlapping_sound_tails_get_independent_tracks(self):
        sounds = [{'target':12,'transient':2,'duration':13}, {'target':15,'transient':0,'duration':6}, {'target':30,'transient':0,'duration':6}]
        self.assertEqual([track for _, track in filmcraft.sound_tracks(sounds)], ['A3','A4','A3'])

    def test_failed_or_unconfirmed_commands_do_not_look_successful(self):
        with tempfile.TemporaryDirectory() as folder:
            dest = Path(folder)/'audition.fcproj'
            for result in [
                subprocess.CompletedProcess([], 1, '{"ok":false}', 'command failed'),
                subprocess.CompletedProcess([], 0, '', ''),
                subprocess.CompletedProcess([], 0, '{"ok":false}', ''),
            ]:
                with patch('filmcraft.subprocess.run', return_value=result):
                    with self.assertRaises(RuntimeError):
                        filmcraft.cli([filmcraft.command('file.newSequence')], dest)
            dest.touch()
            with self.assertRaises(ValueError):
                filmcraft.cli([], dest)

    def test_success_preserves_the_returned_import_ids(self):
        with tempfile.TemporaryDirectory() as folder:
            result = subprocess.CompletedProcess([], 0, '{"ok":true,"result":{"items":[8,9]}}\n', '')
            with patch('filmcraft.subprocess.run', return_value=result):
                self.assertEqual(filmcraft.cli([filmcraft.command('file.import')], Path(folder)/'a.fcproj'), [{'items':[8,9]}])


if __name__ == '__main__':
    unittest.main()
