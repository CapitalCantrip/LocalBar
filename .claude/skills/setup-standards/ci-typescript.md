# CI for a TypeScript or JavaScript project

Add these steps to the workflow's job, after the step that runs the tests.
Replace `src` with the source directories passed to `cc-check` in step 3.
quality-gates is a Python package, so the job needs Python as well as Node.

```yaml
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - run: pip install "quality-gates @ git+https://github.com/CapitalCantrip/quality-gates@v0.12.0"
      - run: cc-check --lang typescript src --baseline cc-baseline.json
      - run: npx vitest run --coverage --coverage.reporter=json
      - run: crap --lang typescript src --istanbul-json coverage/coverage-final.json --baseline crap-baseline.json
```

Coverage needs `@vitest/coverage-v8` in the dev dependencies. Under Jest, run
`npx jest --coverage --coverageReporters=json` instead; it writes the same
`coverage/coverage-final.json`. A project already writing coverage in its test
step reuses that file rather than running the tests twice. A project on
`node --test` leaves out the last two steps and reports CRAP as not done, as
step 2 of the skill says.

Done when the workflow runs both gates, or `cc-check` alone on `node --test`,
and passes on today's tree.
