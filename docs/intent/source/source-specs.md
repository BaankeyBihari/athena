# Source Parsing Specs

Design: `source-design.md`

## Normalization

- [x] **SRC-001**: When a source string has no scheme, no `@`, and the form `<host>:<path>` where `<host>` contains a `.`, the system shall normalize it to `https://<host>/<path>`, appending `.git` if `<path>` does not already end in `.git`.
- [x] **SRC-002**: When a source string is an scp-style SSH address (`<user>@<host>:<path>`), the system shall classify it as remote and keep the URL unchanged.
- [x] **SRC-003**: When a source string begins with `ssh://`, `https://`, `http://`, or `git://`, the system shall classify it as remote and remove trailing `/` characters, leaving the rest of the URL unchanged.
- [x] **SRC-004**: When a source string begins with `/`, `./`, `../`, `~/`, or `file://`, the system shall classify it as local and produce an absolute path, expanding a leading `~` to the home directory and removing a leading `file://`.
- [x] **SRC-005**: If a source string matches none of the forms in SRC-001 to SRC-004, then the system shall fail with an error listing the accepted source forms.

## Fragments

- [x] **SRC-FRAG-001**: When a source string contains `#`, the system shall treat the text before the first `#` as the base source and the text after it as the fragment.
- [x] **SRC-FRAG-002**: When parsing a fragment, the system shall split it at the first `:` into a ref (before) and a subpath (after); a fragment with no `:` shall be a ref only.
- [x] **SRC-FRAG-003**: When the ref part of a fragment is empty, the system shall record no ref.
- [x] **SRC-FRAG-004**: When a fragment contains a subpath, the system shall remove leading `/` characters from it.
- [x] **SRC-FRAG-005**: If a fragment subpath is empty after leading `/` removal, or contains a `..` component, then the system shall fail with an error naming the subpath.
- [x] **SRC-FRAG-006**: If a local source's fragment contains a non-empty ref, then the system shall fail with an error stating that local sources accept only a subpath fragment.
- [x] **SRC-FRAG-008**: When a local source path names a file, the system shall accept it as a local source without error; whether the entry's kind permits a file is decided by `add`.
- [x] **SRC-FRAG-007**: When a fragment starts with `/`, the system shall treat the whole fragment as a subpath with no ref, so `#/a/b` is equivalent to `#:/a/b`.

## Default Name

- [x] **SRC-NAME-001**: When a remote source has a subpath, the system shall derive its default name from the last component of the subpath.
- [x] **SRC-NAME-002**: When a remote source has no subpath, the system shall derive its default name from the last two components of the URL path (or the only component, when there is one), with a trailing `.git` removed and the components joined by `/`.
- [x] **SRC-NAME-003**: When a local source has a subpath, the system shall derive its default name from the last component of the subpath; otherwise from the last component of the local path.

## Subpath Patterns

- [x] **SRC-PAT-001**: When a parsed subpath contains `*`, `?`, `{`, or `}`, the system shall report it as a pattern and keep its text unchanged in the parsed source.
- [x] **SRC-PAT-002**: When matching a brace-free pattern against a folder or file path, the system shall compare them component by component, requiring the same number of components, where `*` matches any run of characters and `?` exactly one character within a component, never across `/`.
- [x] **SRC-PAT-003**: When matching a pattern component, the system shall not let `*` or `?` match a leading `.` of a folder name.
- [x] **SRC-PAT-004**: When expanding a pattern's braces, the system shall produce one brace-free pattern per combination of alternatives, in the order the alternatives are written.
- [x] **SRC-PAT-005**: If a pattern contains an unclosed `{`, an unmatched `}`, nested braces, braces with fewer than two alternatives, an empty alternative, or an alternative containing `/`, then the system shall fail with an error naming the pattern.
- [x] **SRC-NAME-005**: When the entry is a rule or an agent and the name derives from a file (SRC-NAME-001, SRC-NAME-003, or SRC-NAME-004), the system shall keep the file name's extension in the default name.
- [x] **SRC-NAME-004**: When a pattern subpath matches a folder, the system shall derive that entry's default name from the last component of the matched folder path.
