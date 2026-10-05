# uze-platform

What UZE asks of the operating system, answered once per platform. Every
module is one concept with one API, and one implementation per platform
selected by `cfg` at that module's boundary (`mod unix;`/`mod windows;`
behind `use … as imp`): nothing that uses it branches on the operating
system. A platform that cannot answer says *unknown*, never *no*.

- `process`: ending a child with everything it started (process groups,
  Job Objects), asking one to stop; `process::pane`, a pane's processes as
  one unit and which of them is in front
- `probe`: what the kernel knows about a process this crate did not spawn
- `endpoint`: a local socket or named pipe only this user reaches
- `lock`: a lock across processes, released by the kernel when its holder dies
- `shell`: the shell an authored line runs in (`sh -c`, Windows PowerShell 5.1),
  its quoting, and a line that survives a host re-quoting it
- `executable`, `path`, `home`, `fs`, `fs_name`: where programs are, how paths
  compare and are spelled, private files, names NTFS cannot hold
- `environment`, `machine`, `mounts`, `cpu`, `target`, `tools`, `git`: the
  user's `Path`, what keeps UZE from working here, network mounts, the
  processor, the build target, the system's own tools, what Git needs here
- `clock`, `desktop`, `interrupt`, `stdio`: local time, opening a URL, Ctrl+C,
  the process's own streams

A leaf crate naming no domain, no UZE path and no harness, so `uze-core`,
`uze-git`, the terminal runtime and the binary all build on it without
depending on one another. Two architecture tests hold it there:
`only_uze_platform_names_a_platform` (no `cfg(unix|windows)`, `consts::OS`
or `EXE_SUFFIX` in production code anywhere else) and
`uze_platform_names_no_path_of_uze_s_own`.

```bash
cargo test -p uze-platform
```
