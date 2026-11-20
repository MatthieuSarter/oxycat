# CHANGELOG


## v0.3.1 (2026-11-20)

### Bug Fixes

- **deps**: 🐛 replace unmaintained chrono with jiff
  ([`17f9618`](https://github.com/MatthieuSarter/oxycat/commit/17f9618af316236c5f452efc3cb05588a99d7675))

- **search**: 🐛 focus the search field at startup
  ([`74ea291`](https://github.com/MatthieuSarter/oxycat/commit/74ea29166d641c6eb858550f5521ffc34880768c))

- **search**: 🐛 open a folder result itself on double-click
  ([`acc8237`](https://github.com/MatthieuSarter/oxycat/commit/acc82372240c044b13b7dccc6fb8d6054ca91642))


## v0.3.0 (2026-11-15)

### Features

- **search**: ✨ add a "Search within results" checkbox
  ([`6e41403`](https://github.com/MatthieuSarter/oxycat/commit/6e4140356fe492c97746fa12968855210184d60f))

- **search**: ✨ compare folders on their children names
  ([`1c09eaa`](https://github.com/MatthieuSarter/oxycat/commit/1c09eaaabaf29e739a6e0c493de221cbf4db7561))

- **search**: ✨ group identical results across volumes
  ([`d84f028`](https://github.com/MatthieuSarter/oxycat/commit/d84f028ef750214941d6978926ab667622de928f))

- **search**: ✨ support AND, OR and parentheses in the search field
  ([`b2b2ed1`](https://github.com/MatthieuSarter/oxycat/commit/b2b2ed1191855ac568512e8ac4909f5561e1298e))


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
