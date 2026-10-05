# CI for a Python project

## The test file

Save as `tests/test_quality_gates.py`. Replace `SOURCE_DIRS` with the
directories passed to `cc-check` in step 3. It runs under `unittest` and
`pytest` alike.

```python
import io
import os
import unittest
from pathlib import Path

from quality_gates import comment_debt, quality_check, skills_sync

REPO = Path(__file__).resolve().parents[1]
SOURCE_DIRS = ["src"]


class QualityGates(unittest.TestCase):
    def setUp(self):
        self.cwd = os.getcwd()
        os.chdir(REPO)

    def tearDown(self):
        os.chdir(self.cwd)

    def exit_code(self, main, argv):
        with self.assertRaises(SystemExit) as stop:
            main(argv)
        return stop.exception.code

    def test_no_file_has_more_comment_lines_than_its_baseline(self):
        self.assertEqual(comment_debt.main([], root=REPO, out=io.StringIO()), 0)

    def test_no_function_is_more_complex_than_the_baseline_allows(self):
        argv = SOURCE_DIRS + ["--baseline", "cc-baseline.json"]
        self.assertEqual(self.exit_code(quality_check.main, argv), 0)

    def test_the_skills_stage_guard_and_asking_rule_match_the_pinned_version(self):
        self.assertEqual(self.exit_code(skills_sync.main, ["--check"]), 0)
```

## The CRAP step

CRAP needs coverage from the whole suite, so it runs in CI after the tests,
not inside them. Add to the workflow's job, after the step that runs the tests:

```yaml
      - run: pip install coverage
      - run: coverage run -m unittest discover -s tests && coverage json
      - run: crap --lang python src --coverage-json coverage.json --baseline crap-baseline.json
```

Use `coverage run -m pytest` where the project uses pytest, and the same
source directories as the test file.
