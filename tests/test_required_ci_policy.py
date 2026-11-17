"""Current-source CI registration and workflow trigger regression."""
import json
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[1]

class RequiredCIPolicyTests(unittest.TestCase):
    def test_policy_identifies_exact_pull_request_workflow(self):
        p=json.loads((ROOT/'config/delivery-required-ci.json').read_text())
        self.assertEqual(p['schema_version'],1)
        self.assertEqual(p['repository'],'Mikeyphw/zygisk-detach')
        self.assertEqual(p['base_branch'],'master')
        self.assertEqual(p['event'],'pull_request')
        self.assertEqual(p['release_qualification'],'separate')
        self.assertEqual(p['required_workflows'],[{'name':'Zygisk Detach / Regression and Build Qualification','workflow_id':420615431}])

    def test_workflow_has_single_pr_registration_and_all_python_tests(self):
        body=(ROOT/'.github/workflows/zygisk-detach-quality.yml').read_text()
        self.assertIn('pull_request:\n    branches: [master]',body)
        self.assertIn('push:\n    branches: [master]',body)
        self.assertNotIn("branches: ['feature/**', master]",body)
        self.assertIn("-p 'test_*.py'",body)

if __name__=='__main__':unittest.main()
