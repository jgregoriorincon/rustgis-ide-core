# ide-core

Reads the catalogues of spatial data infrastructures (IDE) into one list of services, for the
`ide-*` [RustGIS](https://github.com/opengeos/RustGIS) plugins.

Every IDE publishes its catalogue its own way. Each plugin turns its catalogue into `Entry`s,
and this crate does the rest the same way for all of them:

- `entry`: one service, with only `id`, `name` and `url` required, and `clean` to drop what
  can't be used.
- `kind`: what a service is, read from its address (catalogues mislabel them).
- `url`: an ArcGIS address split into its layer and the `where` a catalogue put in it.
- `index`: the list as a plugin embeds it, and how it stands against the live catalogue.
- `browse`: the list as a panel walks it, category by category.
- `arcgis`: the addresses RustGIS reads a service's features or map image from.

Catalogues are input from the web: nothing here panics on them.

## Plugins that use it

| Plugin | Catalogue | Repository |
|---|---|---|
| `ide-bogota` | IDECA (Bogotá) | [rustgis-ide-bogota](https://github.com/jgregoriorincon/rustgis-ide-bogota) |

## Using it

Depend on a tagged version, so a change here never reaches a plugin until it asks for it:

```toml
[dependencies]
ide-core = { git = "https://github.com/jgregoriorincon/rustgis-ide-core", tag = "v0.1.0" }
```

To try a change here in a plugin before tagging it, point the plugin at your checkout without
editing its files:

```sh
cargo test --config 'patch."https://github.com/jgregoriorincon/rustgis-ide-core".ide-core.path="../rustgis-ide-core"'
```

## Releasing

Bump `version` in `Cargo.toml`, then tag it: `git tag v0.2.0 && git push origin v0.2.0`. A
change that breaks a plugin's code needs a new minor version while this is 0.x.

## License

[MIT](LICENSE)
