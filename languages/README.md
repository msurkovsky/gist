# Languages

Everything language-bound lives here, one directory per language, one file per tool.
Skills stay language-agnostic and resolve `languages/<lang>/<tool>.md` at run time. A
missing directory or file is answered with `unsupported`, never with a guessed command.

Add a language when a real project needs it:

```
cp -r languages/_template languages/<lang>
```

Fill `Detect` and every verb in `toolchain.md`. Verbs without a tool stay `none`.

Nothing is pre-seeded on purpose. The set grows with use.
