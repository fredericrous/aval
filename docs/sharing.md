# Sharing one decision across repositories

A decision made once should be readable everywhere it applies. `aval pack`
publishes a corpus's declarations; `aval add` vendors them into another
repository, which then resolves them as if they were its own.

```console
$ aval add github:acme/decisions
$ aval resolve stack.sql-layer --scope effect-stack
active   decisions:ADR-0002   @effect/sql
  scope: effect-stack
  vendored: from the `decisions` pack; change it there, not here
```

A source is `github:owner/repo`, `forgejo:host/owner/repo`, a git URL or a
path, each optionally `@<rev>`; the commit id that revision resolved to is what
gets recorded. `--as <name>` names the pack, which becomes the prefix on its
record ids.

A consumer needs no corpus of its own — a registry with `packs:` and no `dir:`
is enough, and in a repository with no registry at all `aval add` writes a
starter one. Transport is git and only git, so a private repository and a forge
behind a client certificate both work with your own credentials and no token
issued to this tool.

The vendored file is `.adr/packs/<name>.pack`, with the extension the producer's
own published file carries and no formatter claims. A `.adr/packs/<name>.yaml`
written before 1.3 still loads, and the next `aval add` moves it and rewrites
its registry line. What makes it still the published pack is what it
*declares*, not its bytes — so a formatter that reformatted it has changed
nothing, while a hand-edited decision is reported by `aval add --check` as
`edited`.

## A consumer cannot quietly disagree

A local record deciding a slot a pack already decides is two heads for one
slot, which is exit 5 — the invariant the model already had, and the reason
vendoring is worth anything. What it *can* do is answer the same key at a scope
of its own: one repository may hold a SQL layer in the browser and another in
the app server without either being a disagreement with the fleet's answer at
the fleet's scope.

## Nothing in a pack runs

Nothing in a pack is ever executed, so there is no trust prompt to match
`amont trust`. The review gate is the pull request that adds the file.

That "inert" scopes to execution. A pack's text does reach an agent's context,
so the hook and the MCP tools both say that a record's wording is data rather
than instruction, and a value carrying a control character or a bidi override
is refused — the reviewer and the model must see the same bytes.

## Knowing when a pack fell behind

A pack goes stale silently otherwise, so there are two ways to be told. The
session hook asks (see [in front of an agent](agents.md#vendored-packs-that-fell-behind)).
And a consumer's CI can ask, as an advisory job, where its runner holds a
credential that can read the source — a private corpus is reachable from a
private consumer's runner with a deploy key, not from a public one's without:

```yaml
  packs:
    name: vendored decisions are current (advisory, non-blocking)
    runs-on: ubuntu-latest
    continue-on-error: true
    env:
      AVAL_VERSION: 1.8.0
    steps:
      - uses: actions/checkout@v4
      - uses: webfactory/ssh-agent@v0.9.0   # or however the runner reaches the source
        with:
          ssh-private-key: ${{ secrets.DECISIONS_READ_KEY }}
      - run: |
          curl -fsSL https://raw.githubusercontent.com/fredericrous/aval/main/install/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - run: |
          if ! aval add --check --quiet --budget 30 > packs.txt; then
            cat packs.txt
            echo "::warning::vendored decisions are behind or edited — run aval add"
          fi
```

Non-blocking for the same reason a dependency advisory is: a decision that
changed upstream is information about the fleet, not a defect in the change
under review. The `AVAL_VERSION:` line is the pin `aval hook install` reads to
tell you when CI runs a different aval than you do.
