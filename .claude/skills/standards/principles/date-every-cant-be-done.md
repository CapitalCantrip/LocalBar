# Date every "this can't be done"

A finding that something is impossible or unsupported ("this API can't be
interrupted", "the library has no option for that") records when it was
observed and on what version, so it is re-tested instead of inherited.

**Why:** Tools, libraries and services change. An undated negative finding is
copied from session to session as fact long after it stopped being true, and
nobody re-tests what everyone believes is impossible.

**Pattern:**
- Write the finding with its date and the version observed, and how it was
  tested: "2026-10-06, foo 2.3: `bar()` ignores the timeout; tested with
  `scripts/check_bar_timeout.py`."
- Prefer a rerunnable check over a sentence (*build the lever*).
- When you rely on a dated finding from a version older than the one installed,
  re-test it first.

**Enforced by:** nothing yet; a judgement call.

Source: lessons from use, ADR-002.
