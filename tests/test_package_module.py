"""Reject missing/mismatched binaries, bad assets and unsafe artifact files."""
import importlib.util
import tempfile
import unittest
from pathlib import Path
from zipfile import ZipFile

MOD=Path(__file__).resolve().parents[1]/'tools/package_module.py'
spec=importlib.util.spec_from_file_location('module_assembly', MOD)
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

def put(p,b):p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b)
def fake_elf(klass,machine):return b'\x7fELF'+bytes([klass,1,1,0])+b'\x00'*10+machine.to_bytes(2,'little')+b'\x00'*20

class ModulePackageTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
  root=Path(self.tmp.name);self.repo=root/'repo';self.repo.mkdir()
  self.webroot=root/'webroot';self.webroot.mkdir()
  self.built=root/'built';self.built.mkdir()
  self.output=root/'result.zip'
  put(self.repo/'LICENSE',b'Apache-2.0 original license placeholder for structural test')
  for path in ['customize.sh','post-fs-data.sh']:
   put(self.repo/'module'/path,b'#!/system/bin/sh\ntrue\n')
  put(self.repo/'module/module.prop',b'id=zygisk-detach\nname=Zygisk Detach\nversion=1\nversionCode=1\nupdateJson=https://raw.githubusercontent.com/j-hc/zygisk-detach/master/update.json\n')
  put(self.repo/'module/META-INF/com/google/android/update-binary',b'#!/sbin/sh\ntrue\n')
  put(self.repo/'module/META-INF/com/google/android/updater-script',b'#MAGISK\n')
  put(self.webroot/'index.html',b'<html><script src="index.js"></script></html>')
  put(self.webroot/'index.js',b'console.log("hi")')
  for abi,(triple,klass,machine) in m.ABI.items():
   put(self.built/f'zygisk/libs/{abi}/libzygisk_detach.so',fake_elf(klass,machine))
   put(self.built/f'cli/target/{triple}/release/cli',fake_elf(klass,machine))
 def test_valid_and_deterministic(self):
  info=m.package(self.repo,self.webroot,self.built,self.output)
  self.assertFalse(info['runtime_or_device_verified'])
  with ZipFile(self.output) as z:
   self.assertTrue(z.read('zygisk/arm64-v8a.so').startswith(b'\x7fELF'))
   self.assertTrue(z.read('webroot/index.html').startswith(b'<html'))
   self.assertIn(b'version=1-unqualified', z.read('module.prop'))
   self.assertNotIn(b'updateJson=',z.read('module.prop'))
  other=self.output.with_name('again.zip')
  self.assertEqual(info['sha256'],m.package(self.repo,self.webroot,self.built,other)['sha256'])
 def test_refuse_missing_binaries(self):
  (self.built/'cli/target/aarch64-linux-android/release/cli').unlink()
  with self.assertRaises(m.Refusal):m.package(self.repo,self.webroot,self.built,self.output)
  self.assertFalse(self.output.exists())
 def test_refuse_wrong_architecture(self):
  p=self.built/'zygisk/libs/arm64-v8a/libzygisk_detach.so'
  p.write_bytes(fake_elf(1,40))
  with self.assertRaisesRegex(m.Refusal,'architecture|class'):
   m.package(self.repo,self.webroot,self.built,self.output)
 def test_refuse_symlink_asset(self):
  (self.webroot/'secret.js').symlink_to('/etc/passwd')
  with self.assertRaises(m.Refusal):m.package(self.repo,self.webroot,self.built,self.output)
 def test_refuse_opaque_helper_and_overwrite(self):
  p=self.repo/'module/customize.sh'
  p.write_bytes(b'ksu_profile\n')
  with self.assertRaisesRegex(m.Refusal,'helper'):m.package(self.repo,self.webroot,self.built,self.output)
  p.write_bytes(b'#!/system/bin/sh\ntrue\n')
  self.output.write_text('user artifact')
  with self.assertRaisesRegex(m.Refusal,'overwrite'):m.package(self.repo,self.webroot,self.built,self.output)
  self.assertEqual(self.output.read_text(),'user artifact')

if __name__=='__main__': unittest.main(verbosity=2)
