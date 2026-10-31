# line-ceiling

The stack's source-size check (`mere:stack.7`): no source file over 600
physical lines unless its path is excluded (generated or upstream) or
excepted, each with its reason. An exception whose file has shrunk under the
ceiling or gone fails too.

`line-ceiling [--config PATH] FILE...` runs from the repository root and reads
`line-ceiling.toml` there; without one it uses the defaults. prek passes the
staged files at commit and every file under `prek run --all-files`.

```toml
ceiling = 600
extensions = ["rs", "ps1", "py", "js", "ts", "wgsl", "sh"]

[[exclude]]
path = "vendor/"
reason = "imported upstream sources"

[[exception]]
path = "src/tables.rs"
reason = "a generated lookup table checked in for build speed"
```

Exits 1 on a finding, 2 on a configuration or usage error.

A repository runs it, and `responsa check`, from its `.pre-commit-config.yaml`,
pinned by eidetic's release tags (both tools are standalone packages, so an
install fetches none of eidetic's git sources):

```yaml
repos:
  - repo: local
    hooks:
      - id: line-ceiling
        name: line ceiling
        entry: line-ceiling
        language: rust
        types: [text]
        additional_dependencies: ["cli:https://github.com/merely-made/eidetic:line-ceiling-v0.0.2:line-ceiling"]
      - id: responsa-check
        name: responsa check
        entry: responsa check
        language: rust
        pass_filenames: false
        always_run: true
        additional_dependencies: ["cli:https://github.com/merely-made/eidetic:responsa-v0.0.2:responsa"]
```
