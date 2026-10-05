# Test behaviour, not implementation

A test calls the code the way its users do and asserts the result they observe
against a literal expected value.

**The check:** before keeping a test, ask whether it would still pass if every
function it imports returned nothing. If yes, it observes no behaviour and
cannot fail for a defect. Rewrite the assertion or delete the test.

**Why:** A test that cannot fail for a defect costs CI time and review
attention and catches nothing. A test that restates a constant also fails when
someone legitimately edits the constant, so it blocks that edit.

**Five shapes that pass when every import returns nothing:**
- **Weak assertion:** none, or only "is defined", "is truthy", "did not raise",
  "is an instance of", "is greater than zero".
- **Mock or absence only:** only "was called", "was not called", "is empty",
  "is not this wrong value".
- **Self-referential:** the expected value comes from the code under test,
  `assertEqual(f(a), f(a))`.
- **Constant pin:** the assertion restates a hand-maintained constant, default,
  table row or prompt string.
- **Fixture asserts fixture:** the assertion reads data the test built, and the
  code under test never runs in the test body.

**The fix:** call the code inside the test with one concrete input and assert
the literal output or the observable effect:
`assertEqual(slugify("Hello, World!"), "hello-world")`. For an absence, assert
the presence on the other input in the same test. For a constant, test the
mechanism that reads it. For a mock, assert the payload it received or the
state after the call. Name the test as a sentence stating the behaviour.

**Keep** a test of a relation across a table's rows (a key present in two
tables, a parent that exists).

**Enforced by:** `crap`, in part: it fails a complex function with too little
coverage. Whether the covering tests assert behaviour is a judgement call.

Source: adapted from pstack's *test-behavior-not-implementation* (Lauren Tan,
MIT; LICENSE-pstack).
