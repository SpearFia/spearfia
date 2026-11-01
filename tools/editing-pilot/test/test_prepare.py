# SPDX-License-Identifier: MIT OR Apache-2.0
import importlib.util
import array
import tempfile
import unittest
import wave
import subprocess
from pathlib import Path

spec=importlib.util.spec_from_file_location('prepare',Path(__file__).parents[1]/'prepare.py')
prepare=importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class PreparationTests(unittest.TestCase):
    def test_anchors_normalize_punctuation_but_refuse_ambiguity(self):
        words=[['He',0,.2],['wins,',.2,.5],['here.',.5,1]]
        self.assertEqual(prepare.anchor(words,'he wins'),0)
        with self.assertRaises(ValueError):prepare.anchor(words+words,'he wins')
        with self.assertRaises(ValueError):prepare.anchor(words,'something missing')

    def test_outputs_cannot_modify_source_project(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            with self.assertRaises(ValueError):prepare.prepare(root,root/'exports')

    def test_short_fractional_rate_source_gets_the_exact_requested_frame_count(self):
        with tempfile.TemporaryDirectory() as folder:
            dest=Path(folder)/'segment.mp4'
            subprocess.run(['ffmpeg','-nostdin','-v','error','-f','lavfi','-i','testsrc2=s=64x36:r=24000/1001:d=0.96',
                            '-vf',prepare.picture_filter(35,5),'-frames:v','35','-c:v','libx264','-crf','0','-preset','ultrafast',str(dest)],check=True)
            self.assertEqual(int(prepare.probe(dest)['streams'][0]['nb_frames']),35)
            hashes=subprocess.check_output(['ffmpeg','-nostdin','-v','error','-i',str(dest),'-f','framemd5','-'],text=True)
            hashes=[line.split(',')[-1].strip() for line in hashes.splitlines() if not line.startswith('#')]
            self.assertEqual(len(set(hashes[-5:])),1)

    def test_original_sounds_have_bounded_peak_and_recorded_transient(self):
        with tempfile.TemporaryDirectory() as folder:
            for kind in ['pop','whoosh','impact']:
                info=prepare.sound(Path(folder)/(kind+'.wav'),kind,17)
                with wave.open(str(Path(folder)/(kind+'.wav'))) as source:
                    self.assertEqual(source.getframerate(),48000)
                    self.assertGreater(source.getnframes(),4000)
                    samples=array.array('h',source.readframes(source.getnframes()))
                    self.assertLessEqual(max(abs(v) for v in samples),round(.701*32767))
                    transient=max(range(len(samples)),key=lambda i:abs(samples[i]))/48000*30
                    self.assertLessEqual(abs(transient-info['transient']),.51)
                self.assertLess(info['transient'],info['duration'])
                self.assertEqual(info['sha256'],prepare.sha(Path(folder)/(kind+'.wav')))


if __name__=='__main__':unittest.main()
