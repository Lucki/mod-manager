sources = files([
    'command.vala',
    'game.vala',
    'mod.vala',
    'utils.vala',
])

dependencies = [
    dependency('glib-2.0', version: '>=2.74'),
    dependency('gio-2.0'),
    meson.get_compiler('vala').find_library('posix'),
    meson.get_compiler('c').find_library('libtoml'),
    meson.get_compiler('vala').find_library('libtoml', dirs: vapi_dir),
]

mod_manager_sources = sources + files(['mod-manager.vala'])

executable('mod-manager', mod_manager_sources,
    dependencies: dependencies,
    install: true
)

pkg = import('pkgconfig')
lib = library('mod-manager', mod_manager_sources,
    dependencies: dependencies,
    install: true,
    install_dir: [true, true, true, true],
    vala_gir: 'mod-manager-0.1.gir'
)
pkg.generate(lib)

mod_manager_dep = declare_dependency(
    include_directories: include_directories('.'),
    sources: sources,
)
