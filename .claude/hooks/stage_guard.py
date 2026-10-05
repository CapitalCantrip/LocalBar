#!/usr/bin/env python3
import json
import shlex
import sys

BLOCK_EXIT = 2
SEPARATORS = {"&&", "||", ";", "|", "&", "(", ")", "\n"}
GIT_OPTIONS_WITH_VALUE = {"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--exec-path"}
BULK_ADD_ARGS = {"-A", "--all", "-u", "--update", "--no-ignore-removal", ".", "./", ":/", "*"}
BULK_ADD_LETTERS = set("Au")
BULK_COMMIT_ARGS = {"-a", "--all"}
COMMIT_LETTERS_WITH_VALUE = set("mFCct")
ADD_SUBCOMMANDS = {"add", "stage"}

MESSAGE = (
    "Blocked by the stage guard: `{segment}` stages every changed file, which sweeps in "
    "build output and other sessions' unfinished work. Stage the files this change touched, "
    "by name: git add path/to/one path/to/two"
)


def tokens(command):
    lexer = shlex.shlex(command, posix=True, punctuation_chars=";&|()")
    lexer.whitespace_split = True
    lexer.commenters = ""
    try:
        return list(lexer)
    except ValueError:
        return command.split()


def segments(command):
    current = []
    for token in tokens(command.replace("\n", " ; ")):
        if token in SEPARATORS:
            yield current
            current = []
        else:
            current.append(token)
    yield current


def is_git(token):
    return token == "git" or token.endswith("/git")


def git_arguments(segment):
    for index, token in enumerate(segment):
        if is_git(token):
            return skip_git_options(segment[index + 1:])
    return []


def skip_git_options(rest):
    index = 0
    while index < len(rest) and rest[index].startswith("-"):
        index += 2 if rest[index] in GIT_OPTIONS_WITH_VALUE else 1
    return rest[index:]


def short_cluster(arg):
    return arg.startswith("-") and not arg.startswith("--") and len(arg) > 1


def bulk_add(args):
    for arg in args:
        if arg == "--":
            continue
        if arg in BULK_ADD_ARGS or (short_cluster(arg) and BULK_ADD_LETTERS & set(arg[1:])):
            return True
    return False


def cluster_has_all(arg):
    for letter in arg[1:]:
        if letter == "a":
            return True
        if letter in COMMIT_LETTERS_WITH_VALUE:
            return False
    return False


def takes_separate_value(arg):
    return short_cluster(arg) and arg[-1] in COMMIT_LETTERS_WITH_VALUE and not cluster_has_all(arg)


def bulk_commit(args):
    skip = False
    for arg in args:
        if skip:
            skip = False
        elif arg == "--":
            return False
        elif arg in BULK_COMMIT_ARGS or (short_cluster(arg) and cluster_has_all(arg)):
            return True
        else:
            skip = takes_separate_value(arg)
    return False


def stages_everything(segment):
    args = git_arguments(segment)
    if not args:
        return False
    if args[0] in ADD_SUBCOMMANDS:
        return bulk_add(args[1:])
    if args[0] == "commit":
        return bulk_commit(args[1:])
    return False


def blocked_segment(command):
    for segment in segments(command):
        if stages_everything(segment):
            return " ".join(segment)
    return None


def command_from(payload):
    try:
        data = json.loads(payload)
    except ValueError:
        return ""
    command = data.get("tool_input", {}).get("command") if isinstance(data, dict) else None
    return command if isinstance(command, str) else ""


def main(stdin=None, stderr=None):
    stdin = stdin or sys.stdin
    stderr = stderr or sys.stderr
    segment = blocked_segment(command_from(stdin.read()))
    if segment is None:
        return 0
    print(MESSAGE.format(segment=segment), file=stderr)
    return BLOCK_EXIT


if __name__ == "__main__":
    sys.exit(main())
