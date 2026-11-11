# CHANGELOG


## v0.2.0 (2026-11-11)

### Bug Fixes

- **catalog**: 🐛 open catalogs read-write only when their schema needs an upgrade
  ([`c269e0a`](https://github.com/MatthieuSarter/oxycat/commit/c269e0ac2995f3b7b4bbe725e70c0984672d89f8))

- **config**: 🐛 save oxycat.toml atomically
  ([`1251068`](https://github.com/MatthieuSarter/oxycat/commit/1251068aee3adbe0a6092a95d9b8c8d2784989ef))

- **core**: 🔒 refuse catalog entry names that would leave the source folder
  ([`579c846`](https://github.com/MatthieuSarter/oxycat/commit/579c84627d1added83c1b47e4ec0fc0a1af8639a))

### Features

- **gui**: ✨ log panics to a crash file and report startup and search-thread failures
  ([`0835de7`](https://github.com/MatthieuSarter/oxycat/commit/0835de78181a44e7b860576ac230b63606f92702))


## v0.1.0 (2026-11-11)

- Initial Release
