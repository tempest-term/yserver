# yserver-core

The default `native` feature preserves the Unix server, host X11 backend,
listeners, shared memory and descriptor-passing behavior.

For an in-process server on Windows, Android, iOS, macOS or Linux, use:

```toml
yserver-core = { git = "https://github.com/tempest-term/yserver.git", tag = "yserver-core-v1.7.1-tempest.2", default-features = false }
```

The portable core provides `MemoryStream`, a bounded duplex byte stream with
`Read`, `Write`, `try_clone`, timeouts and shutdown. Its cloned handles have
independent blocking modes; shutting down an endpoint affects all its clones.
Dropping its last handle closes the endpoint. Peer reads wake the core to retry
buffered output, so the core does not poll periodically for writable capacity.

Pass one side of `MemoryStream::pair()` into `setup_thread::spawn` (which accepts
`Into<Transport>`), and bridge the other side to the application's transport.
Use `run_core` with an empty listener collection and your own `Backend`.
The application must retain the setup registry and close it when the display
stops; `run_core` closes established memory clients when it returns.

Memory streams always have non-local, non-FD-passing policy, regardless of the
flags provided to the setup function. The normal Xauthority check still applies.
The `internal` flag may bypass it only when the embedding application has already
authenticated the stream at its own trust boundary, for example an SSH session.
A process-local connection does not make a forwarded client local.

With `native` disabled, native socket/host backends and SysV/POSIX shared memory,
DRI3 and ancillary FD implementations are not compiled. MIT-SHM and DRI3 are not
advertised. Protocol encoders remain portable and unchanged. On Unix the default
build accepts both native and memory clients; only actual native sockets enter
the native poll/FD path.

Validation:

```sh
cargo test -p yserver-core --locked
cargo test -p yserver-core --no-default-features --locked
cargo clippy -p yserver-core --no-default-features --all-targets -- -D warnings
cargo check -p yserver-core --no-default-features --target x86_64-pc-windows-gnu
cargo check -p yserver-core --no-default-features --target aarch64-linux-android
```

The Unix/backend test suite remains enabled for `native` builds. Embedded builds
have portable transport tests; downstream hosts should also exercise their
backend, authentication boundary and display lifecycle through independent X11
clients.
