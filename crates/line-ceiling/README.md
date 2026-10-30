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
